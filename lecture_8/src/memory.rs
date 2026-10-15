use crate::AppResult;

use ash::{Device, vk};
use std::collections::HashMap;
use std::ptr::NonNull;

// Constants

pub const WIDTH: u32 = 1920;
pub const HEIGHT: u32 = 1080;

// The validation layer checks every Vulkan call in debug builds, and is left out of release builds.
pub const ENABLE_VALIDATION: bool = cfg!(debug_assertions);

// Frames the CPU may record while the GPU still works on earlier ones. The renderer and the ring buffer both use it.
pub const MAX_FRAMES_IN_FLIGHT: usize = 2;

// 64 MB per VkDeviceMemory block: big enough that a whole program needs only a few, far below the ~4096 allocations many drivers allow.
const POOL_BLOCK_SIZE: u64 = 64 * 1024 * 1024;

// Per-frame data (uniforms) for all frames in flight: 2 MB per frame holds about 8000 draws of 256-byte slots.
const RING_BUFFER_SIZE: u64 = 4 * 1024 * 1024;

// The largest single upload through the staging buffer.
const STAGING_BUFFER_SIZE: u64 = 32 * 1024 * 1024;

// Helpers

// Rounds value up to a multiple of alignment, which must be a power of two.
const fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

// A byte count in the largest unit that keeps it at least 1.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

// The first memory type allowed by type_bits (from a VkMemoryRequirements) that has every flag in `required`.
pub fn find_memory_type(properties: &vk::PhysicalDeviceMemoryProperties, type_bits: u32, required: vk::MemoryPropertyFlags) -> Option<u32> {
    (0..properties.memory_type_count)
        .find(|&i| type_bits & (1 << i) != 0 && properties.memory_types[i as usize].property_flags.contains(required))
}

// Pool blocks

// A free stretch of a block, in bytes.
struct FreeRegion {
    offset: u64,
    size: u64,
}

// One big VkDeviceMemory, handed out in pieces. The free list is sorted by offset: allocating takes the first region that fits, and freeing merges a region with free neighbors at once, so free space never splinters for good.
struct PoolBlock {
    memory: vk::DeviceMemory,
    size: u64,
    free_regions: Vec<FreeRegion>,
    total_free: u64,
}

impl PoolBlock {
    fn new(device: &Device, size: u64, memory_type_index: u32) -> AppResult<PoolBlock> {
        let info = vk::MemoryAllocateInfo::default().allocation_size(size).memory_type_index(memory_type_index);

        // SAFETY: device is a live device, and the info describes a valid allocation.
        let memory = unsafe { device.allocate_memory(&info, None)? };
        Ok(PoolBlock { memory, size, free_regions: vec![FreeRegion { offset: 0, size }], total_free: size })
    }

    // The offset of `size` bytes aligned to `alignment`, or None if no free region fits. The padding before the aligned offset stays free.
    fn allocate(&mut self, size: u64, alignment: u64) -> Option<u64> {
        let index = self.free_regions.iter().position(|region| align_up(region.offset, alignment) + size <= region.offset + region.size)?;

        let region = &self.free_regions[index];
        let aligned = align_up(region.offset, alignment);
        let padding = aligned - region.offset;
        let remaining = region.size - padding - size;

        // The region becomes up to two: the padding before the allocation, and what's left after it.
        match (padding > 0, remaining > 0) {
            (true, true) => {
                self.free_regions[index].size = padding;
                self.free_regions.insert(index + 1, FreeRegion { offset: aligned + size, size: remaining });
            }
            (true, false) => self.free_regions[index].size = padding,
            (false, true) => self.free_regions[index] = FreeRegion { offset: aligned + size, size: remaining },
            (false, false) => {
                self.free_regions.remove(index);
            }
        }

        self.total_free -= size;
        Some(aligned)
    }

    // Returns a region to the free list, merged with the free regions on either side.
    fn free(&mut self, offset: u64, size: u64) {
        self.total_free += size;

        // partition_point: the index of the first region past offset, found by binary search.
        let index = self.free_regions.partition_point(|region| region.offset < offset);
        self.free_regions.insert(index, FreeRegion { offset, size });

        // Merge with the region after it, then with the one before.
        if index + 1 < self.free_regions.len() && offset + size == self.free_regions[index + 1].offset {
            self.free_regions[index].size += self.free_regions[index + 1].size;
            self.free_regions.remove(index + 1);
        }
        if index > 0 {
            let previous = &self.free_regions[index - 1];
            if previous.offset + previous.size == offset {
                self.free_regions[index - 1].size += self.free_regions[index].size;
                self.free_regions.remove(index);
            }
        }
    }
}

// The pool allocator

// Identifies one buffer made by the GpuAllocator.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferHandle(u64);

// What the allocator remembers about a buffer, to free it later.
struct AllocationRecord {
    buffer: vk::Buffer,
    memory_type_index: u32,
    block_index: usize,
    offset: u64,
    size: u64,
}

// A buffer and the handle to free it with.
pub struct BufferAllocation {
    pub handle: BufferHandle,
    pub buffer: vk::Buffer,
}

// A pool allocator in the style of AMD's Vulkan Memory Allocator (VMA): a few big VkDeviceMemory blocks per memory type, each VkBuffer bound to a piece of one. The number of vkAllocateMemory calls stays tiny however many buffers there are.
pub struct GpuAllocator {
    device: Device,
    memory_properties: vk::PhysicalDeviceMemoryProperties,
    pools: HashMap<u32, Vec<PoolBlock>>,
    allocations: HashMap<BufferHandle, AllocationRecord>,
    next_handle: u64,
}

impl GpuAllocator {
    pub fn new(device: Device, memory_properties: vk::PhysicalDeviceMemoryProperties) -> GpuAllocator {
        for (i, heap) in memory_properties.memory_heaps[..memory_properties.memory_heap_count as usize].iter().enumerate() {
            let kind = if heap.flags.contains(vk::MemoryHeapFlags::DEVICE_LOCAL) { "VRAM" } else { "System" };
            println!("[GpuAllocator] Heap {i}: {} ({kind})", format_bytes(heap.size));
        }

        GpuAllocator { device, memory_properties, pools: HashMap::new(), allocations: HashMap::new(), next_handle: 1 }
    }

    // A device-local VkBuffer, bound to a piece of a pool block.
    pub fn create_buffer(&mut self, size: u64, usage: vk::BufferUsageFlags) -> AppResult<BufferAllocation> {
        let info = vk::BufferCreateInfo::default().size(size).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: device is live and the info is valid. The buffer is destroyed by free_buffer or Drop.
        let buffer = unsafe { self.device.create_buffer(&info, None)? };
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let memory_type_index =
            find_memory_type(&self.memory_properties, requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
                .ok_or("no device-local memory type for this buffer")?;
        let (block_index, offset) = self.pool_allocate(memory_type_index, requirements.size, requirements.alignment)?;

        // SAFETY: the piece at offset is free, large enough and aligned as the buffer requires.
        let memory = self.pools[&memory_type_index][block_index].memory;
        unsafe { self.device.bind_buffer_memory(buffer, memory, offset)? };

        let handle = BufferHandle(self.next_handle);
        self.next_handle += 1;
        self.allocations.insert(handle, AllocationRecord { buffer, memory_type_index, block_index, offset, size: requirements.size });

        Ok(BufferAllocation { handle, buffer })
    }

    // Destroys the buffer, and returns its piece of memory to its block.
    pub fn free_buffer(&mut self, handle: BufferHandle) {
        let Some(record) = self.allocations.remove(&handle) else {
            return;
        };

        // SAFETY: the buffer came from create_buffer, and the GPU is done with it (the caller waited).
        unsafe { self.device.destroy_buffer(record.buffer, None) };
        if let Some(block) = self.pools.get_mut(&record.memory_type_index).and_then(|blocks| blocks.get_mut(record.block_index)) {
            block.free(record.offset, record.size);
        }
    }

    // One line per block, then a total.
    pub fn print_stats(&self) {
        let mut total_used = 0;
        let mut total_size = 0;
        let mut block_count = 0;

        for (memory_type, blocks) in &self.pools {
            for block in blocks {
                let used = block.size - block.total_free;
                total_used += used;
                total_size += block.size;
                block_count += 1;
                println!(
                    "  [type {memory_type}] block {}  used {}  free {}  regions {}",
                    format_bytes(block.size),
                    format_bytes(used),
                    format_bytes(block.total_free),
                    block.free_regions.len()
                );
            }
        }

        println!(
            "[GpuAllocator] {block_count} blocks, {}/{} used, {} live buffers",
            format_bytes(total_used),
            format_bytes(total_size),
            self.allocations.len()
        );
    }

    // A piece of some block of this memory type, making a new block if none has room.
    fn pool_allocate(&mut self, memory_type_index: u32, size: u64, alignment: u64) -> AppResult<(usize, u64)> {
        let blocks = self.pools.entry(memory_type_index).or_default();

        for (index, block) in blocks.iter_mut().enumerate() {
            if block.total_free >= size
                && let Some(offset) = block.allocate(size, alignment)
            {
                return Ok((index, offset));
            }
        }

        // A buffer bigger than a block gets a block of its own, twice its size rounded up to a megabyte.
        let block_size = POOL_BLOCK_SIZE.max(align_up(size * 2, 1024 * 1024));
        let mut block = PoolBlock::new(&self.device, block_size, memory_type_index)?;
        let offset = block.allocate(size, alignment).ok_or("a new block is too small")?;
        blocks.push(block);

        println!(
            "[GpuAllocator] New {} MB block for memory type {memory_type_index} (now {} blocks)",
            block_size / (1024 * 1024),
            blocks.len()
        );
        Ok((blocks.len() - 1, offset))
    }
}

// Whatever is still allocated is destroyed with the allocator.
impl Drop for GpuAllocator {
    fn drop(&mut self) {
        // SAFETY: the owner waited for the GPU to go idle, and nothing uses these buffers or blocks any more.
        unsafe {
            for (_, record) in self.allocations.drain() {
                self.device.destroy_buffer(record.buffer, None);
            }
            for (_, blocks) in self.pools.drain() {
                for block in blocks {
                    self.device.free_memory(block.memory, None);
                }
            }
        }
    }
}

// Host-visible buffers

// A VkBuffer in host-visible, host-coherent memory, mapped for its whole life: the CPU writes through the pointer, and the GPU sees the writes without any flush.
struct MappedBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: NonNull<u8>,
}

impl MappedBuffer {
    fn new(
        device: &Device,
        memory_properties: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
        usage: vk::BufferUsageFlags,
    ) -> AppResult<MappedBuffer> {
        let info = vk::BufferCreateInfo::default().size(size).usage(usage).sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: valid create and allocate infos on a live device; the memory is bound before it's mapped, and destroy() releases all three.
        unsafe {
            let buffer = device.create_buffer(&info, None)?;
            let requirements = device.get_buffer_memory_requirements(buffer);
            let required = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
            let memory_type_index = find_memory_type(memory_properties, requirements.memory_type_bits, required)
                .ok_or("no host-visible, coherent memory type")?;

            let allocate_info = vk::MemoryAllocateInfo::default().allocation_size(requirements.size).memory_type_index(memory_type_index);
            let memory = device.allocate_memory(&allocate_info, None)?;
            device.bind_buffer_memory(buffer, memory, 0)?;

            let mapped = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())?;
            let mapped = NonNull::new(mapped.cast::<u8>()).ok_or("vkMapMemory returned null")?;
            Ok(MappedBuffer { buffer, memory, mapped })
        }
    }

    // SAFETY (for callers): the GPU must be done with the buffer.
    unsafe fn destroy(&self, device: &Device) {
        unsafe {
            device.unmap_memory(self.memory);
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
    }
}

// The ring buffer

// Where one push landed: the byte offset into the ring's VkBuffer.
pub struct RingSlice {
    pub offset: u64,
}

// A bump allocator for data that lives one frame, such as uniforms. The buffer is split into one segment per frame in flight, and each frame fills only its own, so the CPU never overwrites data the GPU may still be reading. Each draw's uniforms are found with a dynamic offset when the descriptor set is bound (UNIFORM_BUFFER_DYNAMIC), so the descriptors never change.
pub struct RingBuffer {
    mapped: MappedBuffer,
    frame_size: u64,
    alignment: u64,
    current_frame: usize,
    frame_offset: u64,
}

impl RingBuffer {
    fn new(device: &Device, memory_properties: &vk::PhysicalDeviceMemoryProperties, alignment: u64) -> AppResult<RingBuffer> {
        let usage = vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::VERTEX_BUFFER;
        let mapped = MappedBuffer::new(device, memory_properties, RING_BUFFER_SIZE, usage)?;
        let frame_size = RING_BUFFER_SIZE / MAX_FRAMES_IN_FLIGHT as u64;

        println!("[RingBuffer] {} total, {}/frame, alignment {alignment}", format_bytes(RING_BUFFER_SIZE), format_bytes(frame_size));
        Ok(RingBuffer { mapped, frame_size, alignment, current_frame: 0, frame_offset: 0 })
    }

    pub fn buffer(&self) -> vk::Buffer {
        self.mapped.buffer
    }

    // Starts writing frame_index's segment from the top. Call this only after waiting for that frame's fence.
    pub fn begin_frame(&mut self, frame_index: usize) {
        self.current_frame = frame_index;
        self.frame_offset = 0;
    }

    // Copies `data` into the current frame's segment, or returns None if the segment is full.
    pub fn push<T: Copy>(&mut self, data: &T) -> Option<RingSlice> {
        let size = std::mem::size_of::<T>() as u64;
        if self.frame_offset + size > self.frame_size {
            return None;
        }

        let offset = self.current_frame as u64 * self.frame_size + self.frame_offset;
        self.frame_offset += align_up(size, self.alignment);

        // SAFETY: offset + size lies inside the mapped buffer, in this frame's segment, which the GPU is no longer reading.
        unsafe {
            let destination = self.mapped.mapped.as_ptr().add(offset as usize);
            std::ptr::copy_nonoverlapping(std::ptr::from_ref(data).cast::<u8>(), destination, size as usize);
        }
        Some(RingSlice { offset })
    }
}

// The memory context

// Owns all of the program's GPU memory: the pool allocator, the ring buffer, and a staging buffer for uploads.
pub struct MemoryContext {
    device: Device,
    pub allocator: GpuAllocator,
    pub ring: RingBuffer,
    staging: MappedBuffer,
}

impl MemoryContext {
    pub fn new(device: Device, memory_properties: vk::PhysicalDeviceMemoryProperties, min_ubo_alignment: u64) -> AppResult<MemoryContext> {
        let allocator = GpuAllocator::new(device.clone(), memory_properties);

        // Ring slots start at multiples of 256: the largest minUniformBufferOffsetAlignment Vulkan allows, so any GPU accepts them.
        let ring = RingBuffer::new(&device, &memory_properties, min_ubo_alignment.max(256))?;
        let staging = MappedBuffer::new(&device, &memory_properties, STAGING_BUFFER_SIZE, vk::BufferUsageFlags::TRANSFER_SRC)?;

        println!("[MemoryContext] Staging belt: {}", format_bytes(STAGING_BUFFER_SIZE));
        Ok(MemoryContext { device, allocator, ring, staging })
    }

    // A device-local buffer holding `data`: copied into the staging buffer, then by the GPU into the new buffer. Waits for the copy to finish.
    pub fn create_buffer_with_data<T: Copy>(
        &mut self,
        data: &[T],
        usage: vk::BufferUsageFlags,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> AppResult<BufferAllocation> {
        let size = size_of_val(data) as u64;
        assert!(size <= STAGING_BUFFER_SIZE, "an upload of {size} bytes doesn't fit the staging buffer");

        let allocation = self.allocator.create_buffer(size, usage | vk::BufferUsageFlags::TRANSFER_DST)?;

        // SAFETY: the staging buffer holds STAGING_BUFFER_SIZE bytes, no other upload is in flight, and every Vulkan object here is live. The queue wait means the copy is done before the command buffer is freed.
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr().cast::<u8>(), self.staging.mapped.as_ptr(), size as usize);

            let allocate_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);
            let command_buffer = self.device.allocate_command_buffers(&allocate_info)?[0];

            let begin_info = vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device.begin_command_buffer(command_buffer, &begin_info)?;
            let region = vk::BufferCopy::default().size(size);
            self.device.cmd_copy_buffer(command_buffer, self.staging.buffer, allocation.buffer, &[region]);
            self.device.end_command_buffer(command_buffer)?;

            let command_buffers = [command_buffer];
            let submit = vk::SubmitInfo::default().command_buffers(&command_buffers);
            self.device.queue_submit(queue, &[submit], vk::Fence::null())?;
            self.device.queue_wait_idle(queue)?;
            self.device.free_command_buffers(command_pool, &command_buffers);
        }

        Ok(allocation)
    }
}

// The ring and the staging buffer are destroyed here; the allocator's own Drop runs after this, as a field.
impl Drop for MemoryContext {
    fn drop(&mut self) {
        // SAFETY: the renderer waited for the GPU to go idle before dropping its MemoryContext.
        unsafe {
            self.ring.mapped.destroy(&self.device);
            self.staging.destroy(&self.device);
        }
    }
}
