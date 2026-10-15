use crate::AppResult;
use crate::device::{DEPTH_FORMAT, DeviceContext};
use crate::memory::{BufferAllocation, MAX_FRAMES_IN_FLIGHT, MemoryContext};
use crate::scene::{Scene, UniformBufferObject, Vertex};

use ash::{Device, vk};
use std::mem::{offset_of, size_of};

// The renderer

// Everything that draws: the scene and its buffers, the pipeline, and the objects that keep the CPU and GPU in step.
pub struct Renderer {
    device: Device,
    memory: MemoryContext,
    scene: Scene,

    // Every mesh's vertices and indices, one after another, in two device-local buffers.
    vertex_buffer: BufferAllocation,
    index_buffer: BufferAllocation,

    // The pipeline, and the descriptors that point its uniform buffer at the ring.
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,

    // The render pass, and one framebuffer per swapchain image.
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,

    // One command buffer per frame in flight.
    command_buffers: Vec<vk::CommandBuffer>,

    // Synchronization. image_available and the fences are per frame in flight; render_finished is per swapchain image, because presentation may still be waiting on a frame's semaphore when that frame slot comes around again.
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight_fences: Vec<vk::Fence>,

    current_frame: usize,
}

impl Renderer {
    pub fn new(device_context: &DeviceContext, mut memory: MemoryContext) -> AppResult<Renderer> {
        let device = device_context.device.clone();
        let extent = device_context.swapchain_extent;
        let scene = Scene::new(extent.width as f32 / extent.height as f32);

        // Upload every mesh through the pool allocator and the staging buffer.
        let vertices: Vec<Vertex> = scene.meshes.iter().flat_map(|mesh| mesh.vertices.iter().copied()).collect();
        let indices: Vec<u32> = scene.meshes.iter().flat_map(|mesh| mesh.indices.iter().copied()).collect();
        let vertex_buffer = memory.create_buffer_with_data(
            &vertices,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            device_context.command_pool,
            device_context.queue,
        )?;
        let index_buffer = memory.create_buffer_with_data(
            &indices,
            vk::BufferUsageFlags::INDEX_BUFFER,
            device_context.command_pool,
            device_context.queue,
        )?;

        let descriptor_set_layout = create_descriptor_set_layout(&device)?;
        let render_pass = create_render_pass(&device, device_context.surface_format.format)?;
        let (pipeline, pipeline_layout) = create_pipeline(&device, render_pass, descriptor_set_layout)?;
        let framebuffers = create_framebuffers(&device, render_pass, device_context)?;
        let (descriptor_pool, descriptor_sets) = create_descriptor_sets(&device, descriptor_set_layout, &memory)?;

        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(device_context.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(MAX_FRAMES_IN_FLIGHT as u32);

        // SAFETY: valid infos on a live device. Fences start signaled, so the first wait on each returns at once.
        let (command_buffers, image_available, in_flight_fences) = unsafe {
            let command_buffers = device.allocate_command_buffers(&allocate_info)?;
            let mut image_available = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
            let mut in_flight_fences = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
            for _ in 0..MAX_FRAMES_IN_FLIGHT {
                image_available.push(device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?);
                in_flight_fences.push(device.create_fence(&vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED), None)?);
            }
            (command_buffers, image_available, in_flight_fences)
        };
        let render_finished = create_semaphores(&device, device_context.swapchain_images.len())?;

        memory.allocator.print_stats();

        Ok(Renderer {
            device,
            memory,
            scene,
            vertex_buffer,
            index_buffer,
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_sets,
            render_pass,
            framebuffers,
            command_buffers,
            image_available,
            render_finished,
            in_flight_fences,
            current_frame: 0,
        })
    }

    // Draws and presents one frame, delta_time seconds after the last. Returns true if the swapchain no longer matches the window and must be recreated.
    pub fn render(&mut self, device_context: &DeviceContext, delta_time: f32) -> AppResult<bool> {
        let frame = self.current_frame;
        let fence = self.in_flight_fences[frame];

        // SAFETY: every handle is live; the fence wait guarantees the GPU is done with this frame slot's command buffer and ring segment before we reuse them.
        unsafe {
            // 1. Wait until the GPU has finished the last frame that used this slot.
            self.device.wait_for_fences(&[fence], true, u64::MAX)?;

            // 2. Acquire a swapchain image. OUT_OF_DATE: the window changed size; try again after recreating the swapchain.
            let image_index = match device_context.swapchain_loader.acquire_next_image(
                device_context.swapchain,
                u64::MAX,
                self.image_available[frame],
                vk::Fence::null(),
            ) {
                Ok((image_index, _)) => image_index,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
                Err(error) => return Err(error.into()),
            };

            // Reset the fence only now that we'll certainly submit work that signals it.
            self.device.reset_fences(&[fence])?;

            // 3. Advance the scene, and start this frame's ring segment over.
            self.scene.update(delta_time);
            self.memory.ring.begin_frame(frame);

            // 4. Record.
            let command_buffer = self.command_buffers[frame];
            self.device.reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())?;
            self.device.begin_command_buffer(command_buffer, &vk::CommandBufferBeginInfo::default())?;
            self.record(command_buffer, device_context, image_index as usize);
            self.device.end_command_buffer(command_buffer)?;

            // 5. Submit: wait for the image before writing color, signal render_finished when done, and the fence for our next turn.
            let wait_semaphores = [self.image_available[frame]];
            let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let command_buffers = [command_buffer];
            let signal_semaphores = [self.render_finished[image_index as usize]];
            let submit = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);
            self.device.queue_submit(device_context.queue, &[submit], fence)?;

            // 6. Present, once rendering has finished. Ok(true) means suboptimal: it worked, but the swapchain should be recreated.
            let swapchains = [device_context.swapchain];
            let image_indices = [image_index];
            let present =
                vk::PresentInfoKHR::default().wait_semaphores(&signal_semaphores).swapchains(&swapchains).image_indices(&image_indices);
            let stale = match device_context.swapchain_loader.queue_present(device_context.queue, &present) {
                Ok(suboptimal) => suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
                Err(error) => return Err(error.into()),
            };

            self.current_frame = (frame + 1) % MAX_FRAMES_IN_FLIGHT;
            Ok(stale)
        }
    }

    // The frame's commands: one render pass, one draw per mesh.
    // SAFETY (for callers): command_buffer is recording, and every handle is live.
    unsafe fn record(&mut self, command_buffer: vk::CommandBuffer, device_context: &DeviceContext, image_index: usize) {
        let extent = device_context.swapchain_extent;
        let clear_values = [
            vk::ClearValue { color: vk::ClearColorValue { float32: [0.1, 0.1, 0.1, 1.0] } },
            vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } },
        ];
        let render_area = vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent };
        let begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[image_index])
            .render_area(render_area)
            .clear_values(&clear_values);

        // SAFETY: see above.
        unsafe {
            self.device.cmd_begin_render_pass(command_buffer, &begin_info, vk::SubpassContents::INLINE);
            self.device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

            // Viewport and scissor are dynamic state, so the pipeline survives a resize.
            let viewport =
                vk::Viewport { x: 0.0, y: 0.0, width: extent.width as f32, height: extent.height as f32, min_depth: 0.0, max_depth: 1.0 };
            self.device.cmd_set_viewport(command_buffer, 0, &[viewport]);
            self.device.cmd_set_scissor(command_buffer, 0, &[render_area]);

            self.device.cmd_bind_vertex_buffers(command_buffer, 0, &[self.vertex_buffer.buffer], &[0]);
            self.device.cmd_bind_index_buffer(command_buffer, self.index_buffer.buffer, 0, vk::IndexType::UINT32);

            let view = self.scene.camera.view_matrix();
            let proj = self.scene.camera.projection_matrix();
            let mut vertex_offset = 0;
            let mut first_index = 0;

            for mesh in &self.scene.meshes {
                // Each draw's matrices go into the ring at their own aligned offset in the same VkBuffer; the dynamic offset tells the descriptor where. No descriptor is ever rewritten.
                let ubo = UniformBufferObject { model: mesh.transform, view, proj };
                let slice = self.memory.ring.push(&ubo).expect("the ring segment has room: make RING_BUFFER_SIZE bigger");
                self.device.cmd_bind_descriptor_sets(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_layout,
                    0,
                    &[self.descriptor_sets[self.current_frame]],
                    &[slice.offset as u32],
                );

                self.device.cmd_draw_indexed(command_buffer, mesh.indices.len() as u32, 1, first_index, vertex_offset, 0);
                vertex_offset += mesh.vertices.len() as i32;
                first_index += mesh.indices.len() as u32;
            }

            self.device.cmd_end_render_pass(command_buffer);
        }
    }

    // After the swapchain was recreated: new framebuffers, as many render_finished semaphores as there are new images, and the camera's new aspect ratio.
    pub fn recreate_framebuffers(&mut self, device_context: &DeviceContext) -> AppResult<()> {
        // SAFETY: after device_wait_idle the GPU uses none of the old objects.
        unsafe {
            self.device.device_wait_idle()?;
            for &framebuffer in &self.framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }
            for &semaphore in &self.render_finished {
                self.device.destroy_semaphore(semaphore, None);
            }
        }
        self.framebuffers.clear();
        self.render_finished.clear();

        self.render_finished = create_semaphores(&self.device, device_context.swapchain_images.len())?;
        self.framebuffers = create_framebuffers(&self.device, self.render_pass, device_context)?;

        let extent = device_context.swapchain_extent;
        self.scene.camera.update_aspect(extent.width, extent.height);
        Ok(())
    }
}

// Descriptors

// One binding: a uniform buffer for the vertex shader, found through a dynamic offset.
fn create_descriptor_set_layout(device: &Device) -> AppResult<vk::DescriptorSetLayout> {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX)];
    let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings);

    // SAFETY: a valid info on a live device.
    Ok(unsafe { device.create_descriptor_set_layout(&info, None)? })
}

// A descriptor pool, and the descriptor sets allocated from it.
type DescriptorSets = (vk::DescriptorPool, Vec<vk::DescriptorSet>);

// One set per frame in flight, each pointing at the start of the ring with the size of one UniformBufferObject. Where each draw's data really is comes in as a dynamic offset when the set is bound.
fn create_descriptor_sets(device: &Device, layout: vk::DescriptorSetLayout, memory: &MemoryContext) -> AppResult<DescriptorSets> {
    let pool_sizes =
        [vk::DescriptorPoolSize::default().ty(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC).descriptor_count(MAX_FRAMES_IN_FLIGHT as u32)];
    let pool_info = vk::DescriptorPoolCreateInfo::default().pool_sizes(&pool_sizes).max_sets(MAX_FRAMES_IN_FLIGHT as u32);

    // SAFETY: valid infos on a live device; the ring's buffer outlives the sets.
    unsafe {
        let pool = device.create_descriptor_pool(&pool_info, None)?;
        let layouts = [layout; MAX_FRAMES_IN_FLIGHT];
        let sets =
            device.allocate_descriptor_sets(&vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts))?;

        for &set in &sets {
            let buffer_infos =
                [vk::DescriptorBufferInfo::default().buffer(memory.ring.buffer()).offset(0).range(size_of::<UniformBufferObject>() as u64)];
            let write = vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC)
                .buffer_info(&buffer_infos);
            device.update_descriptor_sets(&[write], &[]);
        }

        Ok((pool, sets))
    }
}

// The pipeline

// SPIR-V is a stream of 32-bit words, but include_bytes! gives bytes with no alignment promise, so we copy them into u32s.
fn spirv_words(bytes: &[u8]) -> Vec<u32> {
    let (words, rest) = bytes.as_chunks::<4>();
    assert!(rest.is_empty(), "SPIR-V is a whole number of 32-bit words");
    words.iter().map(|&word| u32::from_le_bytes(word)).collect()
}

fn create_pipeline(
    device: &Device,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> AppResult<(vk::Pipeline, vk::PipelineLayout)> {
    // build.rs compiled the shaders into OUT_DIR, and include_bytes! embeds them in the executable.
    let vertex_code = spirv_words(include_bytes!(concat!(env!("OUT_DIR"), "/basic.vert.spv")));
    let fragment_code = spirv_words(include_bytes!(concat!(env!("OUT_DIR"), "/basic.frag.spv")));

    // Vertex input: one buffer of Vertex structs, with position at location 0 and color at location 1.
    let bindings = [vk::VertexInputBindingDescription::default()
        .binding(0)
        .stride(size_of::<Vertex>() as u32)
        .input_rate(vk::VertexInputRate::VERTEX)];
    let attributes = [
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(offset_of!(Vertex, position) as u32),
        vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(offset_of!(Vertex, color) as u32),
    ];
    let vertex_input =
        vk::PipelineVertexInputStateCreateInfo::default().vertex_binding_descriptions(&bindings).vertex_attribute_descriptions(&attributes);

    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    // The viewport and scissor themselves are dynamic: set while recording.
    let viewport_state = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);

    // Triangles counter-clockwise on screen face us; the back faces are culled.
    let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE);

    let multisampling = vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);

    // A fragment is drawn only if it's nearer than what's already there, and then its depth is stored.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS);

    let blend_attachments =
        [vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA).blend_enable(false)];
    let color_blending = vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);

    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

    let set_layouts = [descriptor_set_layout];
    let layout_info = vk::PipelineLayoutCreateInfo::default().set_layouts(&set_layouts);

    // SAFETY: valid infos on a live device. The shader modules are only needed while the pipeline is created.
    unsafe {
        let vertex_module = device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&vertex_code), None)?;
        let fragment_module = device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&fragment_code), None)?;
        let stages = [
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::VERTEX).module(vertex_module).name(c"main"),
            vk::PipelineShaderStageCreateInfo::default().stage(vk::ShaderStageFlags::FRAGMENT).module(fragment_module).name(c"main"),
        ];

        let pipeline_layout = device.create_pipeline_layout(&layout_info, None)?;
        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state)
            .layout(pipeline_layout)
            .render_pass(render_pass)
            .subpass(0);

        // On failure, ash returns the pipelines that did get made along with the error; we only want the error.
        let pipelines = device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None).map_err(|(_, error)| error)?;

        device.destroy_shader_module(vertex_module, None);
        device.destroy_shader_module(fragment_module, None);
        Ok((pipelines[0], pipeline_layout))
    }
}

// The render pass and framebuffers

// Two attachments: the swapchain image (cleared, drawn, then presented) and the depth buffer (cleared, and thrown away after the pass).
fn create_render_pass(device: &Device, format: vk::Format) -> AppResult<vk::RenderPass> {
    let attachments = [
        vk::AttachmentDescription::default()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR),
        vk::AttachmentDescription::default()
            .format(DEPTH_FORMAT)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
    ];

    let color_refs = [vk::AttachmentReference::default().attachment(0).layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
    let depth_ref = vk::AttachmentReference::default().attachment(1).layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL);
    let subpasses = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_refs)
        .depth_stencil_attachment(&depth_ref)];

    // The pass's color and depth writes (including the clears) wait for earlier work: color output, which chains to the submit's wait for the acquired image, and the previous frame's depth writes to the shared depth buffer, which finish in the late fragment tests.
    let dependencies = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS)
        .src_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)];

    let info = vk::RenderPassCreateInfo::default().attachments(&attachments).subpasses(&subpasses).dependencies(&dependencies);

    // SAFETY: a valid info on a live device.
    Ok(unsafe { device.create_render_pass(&info, None)? })
}

// One framebuffer per swapchain image, each pairing that image with the shared depth buffer.
fn create_framebuffers(device: &Device, render_pass: vk::RenderPass, device_context: &DeviceContext) -> AppResult<Vec<vk::Framebuffer>> {
    let extent = device_context.swapchain_extent;
    device_context
        .swapchain_image_views
        .iter()
        .map(|&view| {
            let attachments = [view, device_context.depth_image_view];
            let info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);

            // SAFETY: the views are live and match the render pass's attachments.
            Ok(unsafe { device.create_framebuffer(&info, None)? })
        })
        .collect()
}

fn create_semaphores(device: &Device, count: usize) -> AppResult<Vec<vk::Semaphore>> {
    // SAFETY: a valid info on a live device.
    (0..count).map(|_| Ok(unsafe { device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)? })).collect()
}

// Cleanup

impl Drop for Renderer {
    fn drop(&mut self) {
        // SAFETY: after device_wait_idle the GPU uses nothing, and each object is destroyed once.
        unsafe {
            let _ = self.device.device_wait_idle();

            for &fence in &self.in_flight_fences {
                self.device.destroy_fence(fence, None);
            }
            for &semaphore in self.render_finished.iter().chain(&self.image_available) {
                self.device.destroy_semaphore(semaphore, None);
            }

            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            for &framebuffer in &self.framebuffers {
                self.device.destroy_framebuffer(framebuffer, None);
            }
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_render_pass(self.render_pass, None);
        }

        // The mesh buffers go back to the pool. The ring, the staging buffer and every pool block are freed when `memory` drops, right after this.
        self.memory.allocator.free_buffer(self.vertex_buffer.handle);
        self.memory.allocator.free_buffer(self.index_buffer.handle);
    }
}
