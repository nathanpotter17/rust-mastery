# Lecture 8: A Vulkan cube

By the end of this lecture a window shows a cube with a different color on each face, spinning on two axes, drawn by the GPU through Vulkan, at the display's refresh rate. It survives being resized and minimized, and closes with Escape. It's in the spirit of `vkcube`, the classic Vulkan test program, built from scratch in Rust.

Vulkan is the most explicit graphics API there is: the program creates and destroys every object itself, allocates GPU memory itself, and synchronizes the CPU with the GPU itself. That makes it a fitting finale, because every idea in this course shows up:
- `unsafe` calls through generated bindings (lectures 6 and 7), with `Drop` destroying every Vulkan object exactly once (lecture 2);
- `#[repr(C)]` structs shared with the GPU, with their layout checked while compiling (lectures 2 and 6);
- a pool allocator for GPU memory, a free list of offsets in big blocks, and a per-frame ring buffer;
- `Result`, `?` and `Box<dyn Error>` everywhere a call can fail (lecture 5);
- a build script that compiles shaders (lecture 6);
- slices, iterators, closures and `Option` throughout (lectures 3 and 4).

## 8.1 Project layout and dependencies

### Why
Two crates do the platform work:
- **`ash`** is the Rust binding to Vulkan: every Vulkan function, struct and enum, generated from Khronos's specification, much as bindgen generated lecture 6's. It adds builder methods to structs, and turns Vulkan's error codes into `Result`s, but leaves every call `unsafe` and every object's lifetime to us.
- **`sdl3`** binds SDL3, the cross-platform library that opens windows, delivers keyboard and window events, and knows how to connect Vulkan to the platform's windows.

### How
- **The files:**
  - `shaders/basic.vert` and `shaders/basic.frag`: the GPU programs, in GLSL.
  - `build.rs`: compiles the shaders to SPIR-V.
  - `src/scene.rs`: the cube, the camera, and the matrix math.
  - `src/memory.rs`: GPU memory: a pool allocator, a ring buffer, and a staging buffer.
  - `src/device.rs`: the Vulkan instance, GPU, device, swapchain and depth buffer.
  - `src/renderer.rs`: the pipeline, the descriptors, the frame's commands, and synchronization.
  - `src/main.rs`: the window and the frame loop.
- **`ash` with `linked`:** the Vulkan loader (`libvulkan.so`, installed with the GPU driver) is linked at build time, so `Entry::linked()` needs no run-time loading.
- **`sdl3` with `build-from-source-static`:** SDL3's C source comes as another crate, `sdl3-src`, which Cargo downloads like any dependency. A build script builds it with CMake, and links it statically. The first build takes a minute or two. On Linux, SDL's Wayland support needs `pkg-config`, `libwayland-dev`, `libxkbcommon-dev` and `libdecor-0-dev`.
- **Crate features:** `features = [...]` turns on optional parts of a dependency, chosen by its authors.
- **More tools:** `glslc` compiles the shaders, and the Vulkan validation layers (`vulkan-validationlayers` on Ubuntu) check every Vulkan call in debug builds.

### Code
From the repo root:
```bash
cargo new lecture_8
```
```bash
mkdir -p lecture_8/shaders
```

`lecture_8/Cargo.toml`:
```toml
[package]
name = "lecture_8"
version = "0.1.0"
edition = "2024"

[dependencies]
# Vulkan bindings. "linked" links the Vulkan loader at build time, so Entry::linked() needs no dlopen.
ash = { version = "0.38", features = ["linked"] }
# SDL3, compiled from its C source by the crate's build script (CMake) and linked statically.
sdl3 = { version = "0.20", features = ["build-from-source-static"] }
```

## 8.2 Shaders: `shaders/` and `build.rs`

### Why
A GPU runs its own programs, *shaders*. For each vertex of the cube, the vertex shader works out where on screen it lands; for each pixel a triangle covers, the fragment shader works out its color. Vulkan takes shaders as SPIR-V, a binary format, so we write them in GLSL and compile them as part of the build.

### How
- **The vertex shader:**
  - **Uniforms:** `layout(binding = 0) uniform UniformBufferObject` reads three matrices from a uniform buffer: model (the cube's rotation), view (the camera) and projection (perspective). Our Rust `UniformBufferObject` must match it byte for byte.
  - **Inputs:** `layout(location = 0) in vec3 inPosition` and `location = 1` `inColor` come from the vertex buffer, one `Vertex` per vertex.
  - **Outputs:** `gl_Position` is the vertex's position in *clip space*, the GPU's screen coordinates before perspective; multiplying projection × view × model × position takes it there. `fragColor` is passed on, interpolated across the triangle.
- **The fragment shader** writes the interpolated color, scaled by a simple fake lighting term: an ambient level of 0.3, plus more for brighter colors (by perceived luminance, the classic 0.299/0.587/0.114 weights).
- **`build.rs`:**
  - **Running a program:** `std::process::Command` runs `glslc` for each shader, writing `basic.vert.spv` and `basic.frag.spv` into `OUT_DIR`. `status()` waits for it and returns its exit status, and the `assert!` fails the build if it failed.
  - **Flags:** `--target-env=vulkan1.3` compiles for Vulkan 1.3, and `-Werror` makes any shader warning an error.
  - **`rerun-if-changed`** per shader: editing a shader (or `build.rs` itself) reruns the script, and nothing else does.
- **Embedding:** the renderer includes the SPIR-V with `include_bytes!(concat!(env!("OUT_DIR"), "/basic.vert.spv"))`, so the shaders are part of the executable, and there are no files to find at run time.

### Code
`lecture_8/shaders/basic.vert`:
```glsl
#version 450

// The matrices for this draw, from the ring buffer, through a dynamic uniform buffer descriptor (set 0, binding 0).
layout(binding = 0) uniform UniformBufferObject {
    mat4 model;
    mat4 view;
    mat4 proj;
} ubo;

// Per vertex, from the vertex buffer: locations 0 and 1 match the pipeline's vertex attributes.
layout(location = 0) in vec3 inPosition;
layout(location = 1) in vec3 inColor;

layout(location = 0) out vec3 fragColor;

void main() {
    gl_Position = ubo.proj * ubo.view * ubo.model * vec4(inPosition, 1.0);
    fragColor = inColor;
}
```

`lecture_8/shaders/basic.frag`:
```glsl
#version 450

layout(location = 0) in vec3 fragColor;
layout(location = 0) out vec4 outColor;

void main() {
    // Simple fake lighting: brighter colors (by perceived luminance) get more light, on top of a fixed ambient level.
    float ambient = 0.3;
    float brightness = dot(fragColor, vec3(0.299, 0.587, 0.114));
    float light = ambient + (1.0 - ambient) * brightness;

    outColor = vec4(fragColor * light, 1.0);
}
```

`lecture_8/build.rs`:
```rust
use std::env;
use std::path::PathBuf;
use std::process::Command;

// Compiles each GLSL shader to SPIR-V with glslc, into OUT_DIR, where include_bytes! picks it up.
fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").expect("Cargo sets OUT_DIR"));

    for shader in ["basic.vert", "basic.frag"] {
        let source = format!("shaders/{shader}");
        println!("cargo::rerun-if-changed={source}");

        let status = Command::new("glslc")
            .arg(&source)
            .arg("-o")
            .arg(out.join(format!("{shader}.spv")))
            .args(["--target-env=vulkan1.3", "-Werror"])
            .status()
            .expect("glslc runs (it ships with the Vulkan SDK, or the shaderc package)");
        assert!(status.success(), "glslc failed on {source}");
    }
}
```

## 8.3 The scene: `src/scene.rs`

### Why
Everything the cube is, and everything that moves it, is CPU-side data and math: vertices, indices, a camera, and 4 × 4 matrices. We write the few matrix functions we need ourselves, so there's no math library to learn alongside Vulkan.

### How
- **Aliases:** `Mat4` is `[[f32; 4]; 4]` and `Vec3` is `[f32; 3]` (lecture 4). A matrix is stored column by column: `m[column][row]`, the order in which GLSL reads a `mat4` from memory. So the translation part of the view matrix is its last array, `m[3]`.
- **`#[repr(C)]` and the GPU:** `Vertex` is two `Vec3`s at offsets 0 and 12, 24 bytes per vertex; the pipeline's vertex layout (8.6) reads them from exactly there, using `offset_of!`. `UniformBufferObject` is three `Mat4`s, 192 bytes, laid out as GLSL's `std140` rules expect for three `mat4`s, with no padding. Const asserts check both layouts while compiling. Both derive `Copy`, so their bytes can be copied straight into GPU memory.
- **The camera:**
  - **`view_matrix`** is a right-handed look-at matrix: it moves and turns the world so the camera sits at the origin, looking down −z, with `up` pointing up. `f`, `s` and `u` are the camera's forward, side and up directions, built with cross products.
  - **`projection_matrix`** is a perspective projection into Vulkan's clip space, which differs from OpenGL's: depth runs from 0 (near) to 1 (far), and y points *down* the screen, hence the `-f`.
  - **`update_aspect`** keeps the cube from stretching when the window changes shape.
- **The cube:**
  - **24 vertices:** each face needs its own color, and a corner shared by three faces needs three colors, so each face gets its own four vertices.
  - **Indices:** each face is two triangles, `0-1-2` and `2-3-0` of its four corners, listed counter-clockwise as seen from outside. The GPU uses the winding order to tell front faces from back faces, and skips the back ones (8.6).
  - **`extend`** appends the six indices of a face at once.
- **The scene** is a list of meshes, a camera, and a rotation angle. `update(delta_time)` advances the angle by 45 degrees per second, and sets the cube's transform to a rotation around y times a rotation around x, the latter 0.7 times as fast. `first_mut()` returns an `Option<&mut Mesh>`, so an empty scene would just not spin.
- **The helpers:**
  - **`rotation_matrix`** builds a rotation around any axis with Rodrigues' formula, written column by column. `sin_cos()` computes both at once.
  - **`multiply_matrices`** is the matrix product `a * b`, which applies `b` first and then `a`. The textbook formula, row `j` of column `i` is the sum over `k` of `a`'s row `j`, column `k` times `b`'s row `k`, column `i`, becomes `a[k][j] * b[i][k]` in `m[column][row]` storage. `iter_mut().enumerate()` walks the result's columns and rows, and each element is a `map(...).sum()` over `k`.
  - **The cube's transform** is `rotation_y * rotation_x`: each vertex is turned around x, then around y.

### Code
`lecture_8/src/scene.rs`:
```rust
// Matrices are [[f32; 4]; 4], stored column by column: m[column][row], the order GLSL's mat4 reads from memory.
use std::mem::{offset_of, size_of};

pub type Mat4 = [[f32; 4]; 4];
pub type Vec3 = [f32; 3];

// GPU data

// One corner of the cube. #[repr(C)]: the fields sit in this order, at offsets 0 and 12, as the pipeline's vertex layout says.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: Vec3,
    pub color: Vec3,
}

// What the vertex shader reads from its uniform buffer: three matrices, 192 bytes, in GLSL's std140 layout.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UniformBufferObject {
    pub model: Mat4,
    pub view: Mat4,
    pub proj: Mat4,
}

// The shaders read these structs byte for byte, so their layout is checked while compiling.
const _: () = assert!(size_of::<Vertex>() == 24 && offset_of!(Vertex, position) == 0 && offset_of!(Vertex, color) == 12);
const _: () = assert!(size_of::<UniformBufferObject>() == 192);

// The camera

pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov: f32, // vertical, in degrees
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Camera {
        Camera { position: [3.0, 3.0, 3.0], target: [0.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], fov: 45.0, aspect, near: 0.1, far: 100.0 }
    }

    // A right-handed look-at matrix: the camera sits at position, looking at target, with -z pointing forward.
    pub fn view_matrix(&self) -> Mat4 {
        let f = normalize(sub3(self.target, self.position));
        let s = normalize(cross3(f, self.up));
        let u = cross3(s, f);

        [
            [s[0], u[0], -f[0], 0.0],
            [s[1], u[1], -f[1], 0.0],
            [s[2], u[2], -f[2], 0.0],
            [-dot3(s, self.position), -dot3(u, self.position), dot3(f, self.position), 1.0],
        ]
    }

    // A perspective projection to Vulkan's clip space: depth 0 to 1, and y pointing down, hence the -f.
    pub fn projection_matrix(&self) -> Mat4 {
        let f = 1.0 / (self.fov.to_radians() / 2.0).tan();

        [
            [f / self.aspect, 0.0, 0.0, 0.0],
            [0.0, -f, 0.0, 0.0],
            [0.0, 0.0, self.far / (self.near - self.far), -1.0],
            [0.0, 0.0, (self.near * self.far) / (self.near - self.far), 0.0],
        ]
    }

    pub fn update_aspect(&mut self, width: u32, height: u32) {
        self.aspect = width as f32 / height as f32;
    }
}

// Meshes

pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub transform: Mat4,
}

impl Mesh {
    // A unit cube with one flat color per face. Each face has its own four vertices, since a corner shared by three faces needs three colors.
    pub fn cube() -> Mesh {
        let positions = [
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
            [0.5, -0.5, -0.5],
            [-0.5, -0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [0.5, 0.5, -0.5],
        ];

        // Each face: its four corners counter-clockwise as seen from outside, and its color.
        let faces = [
            ([0, 1, 2, 3], [1.0, 0.0, 0.0]), // front, red
            ([4, 5, 6, 7], [0.0, 1.0, 0.0]), // back, green
            ([3, 2, 7, 6], [0.0, 0.0, 1.0]), // top, blue
            ([5, 4, 1, 0], [1.0, 1.0, 0.0]), // bottom, yellow
            ([1, 4, 7, 2], [1.0, 0.0, 1.0]), // right, magenta
            ([5, 0, 3, 6], [0.0, 1.0, 1.0]), // left, cyan
        ];

        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);

        for (corners, color) in faces {
            let base = vertices.len() as u32;
            for corner in corners {
                vertices.push(Vertex { position: positions[corner], color });
            }

            // Two triangles per face: 0-1-2 and 2-3-0.
            indices.extend([base, base + 1, base + 2, base + 2, base + 3, base]);
        }

        Mesh { vertices, indices, transform: identity_matrix() }
    }
}

// The scene

pub struct Scene {
    pub meshes: Vec<Mesh>,
    pub camera: Camera,
    rotation: f32,
}

impl Scene {
    pub fn new(aspect: f32) -> Scene {
        Scene { meshes: vec![Mesh::cube()], camera: Camera::new(aspect), rotation: 0.0 }
    }

    // Spins the cube 45 degrees per second around y, and 0.7 times as fast around x.
    pub fn update(&mut self, delta_time: f32) {
        self.rotation += delta_time * 45.0_f32.to_radians();

        if let Some(cube) = self.meshes.first_mut() {
            let y_rotation = rotation_matrix(self.rotation, [0.0, 1.0, 0.0]);
            let x_rotation = rotation_matrix(self.rotation * 0.7, [1.0, 0.0, 0.0]);
            cube.transform = multiply_matrices(y_rotation, x_rotation);
        }
    }
}

// Vector and matrix helpers

fn normalize(v: Vec3) -> Vec3 {
    let length = dot3(v, v).sqrt();
    if length > 0.0 { [v[0] / length, v[1] / length, v[2] / length] } else { v }
}

fn cross3(a: Vec3, b: Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot3(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn identity_matrix() -> Mat4 {
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]
}

// A rotation around `axis` by `angle` radians (counter-clockwise, looking down the axis), built with Rodrigues' formula. Each inner array is a column.
fn rotation_matrix(angle: f32, axis: Vec3) -> Mat4 {
    let [x, y, z] = normalize(axis);
    let (s, c) = angle.sin_cos();
    let oc = 1.0 - c;

    [
        [oc * x * x + c, oc * x * y + z * s, oc * z * x - y * s, 0.0],
        [oc * x * y - z * s, oc * y * y + c, oc * y * z + x * s, 0.0],
        [oc * z * x + y * s, oc * y * z - x * s, oc * z * z + c, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

// The matrix product a * b: applying it applies b first, then a. With m[column][row] storage, row j of column i is the sum over k of a's row j in column k times b's row k in column i.
fn multiply_matrices(a: Mat4, b: Mat4) -> Mat4 {
    let mut result = [[0.0; 4]; 4];
    for (i, column) in result.iter_mut().enumerate() {
        for (j, value) in column.iter_mut().enumerate() {
            *value = (0..4).map(|k| a[k][j] * b[i][k]).sum();
        }
    }
    result
}
```

## 8.4 GPU memory: `src/memory.rs`

### Why
A GPU has its own memory, and Vulkan makes us manage it. `vkAllocateMemory` is slow, and drivers allow only a few thousand allocations at once, so real renderers allocate a few big blocks and hand out pieces of them, as AMD's widely used Vulkan Memory Allocator (VMA) does. We write a small allocator in that style. Data also has to *get* into GPU memory, and some of it changes every frame; those need two more tools, a staging buffer and a ring buffer.

### How
- **Memory types:** a GPU offers several *memory types*, each with property flags.
  - **`DEVICE_LOCAL`** memory is fastest for the GPU, but usually not visible to the CPU.
  - **`HOST_VISIBLE`** memory can be *mapped*: the CPU gets a pointer into it and writes through that. `HOST_COHERENT` means those writes reach the GPU with no explicit flush.
  - **Choosing one:** every buffer reports, in its memory requirements, a bit mask of the types it may use. `find_memory_type` returns the first allowed type with every flag we require. `(0..count).find(...)` scans the indexes, and `1 << i` tests bit `i`.
- **Pool blocks:** a `PoolBlock` is one 64 MB `VkDeviceMemory`, with a list of its free regions, sorted by offset.
  - **Allocating** takes the first region where an aligned piece fits (*first fit*). The region becomes up to two: the padding before the aligned offset, which stays free, and whatever is left after the piece. The `match` on `(padding > 0, remaining > 0)` handles the four cases.
  - **Freeing** inserts the region back in order (`partition_point` finds the spot by binary search), and merges it with a free neighbor on either side, so free space doesn't splinter for good.
- **The allocator:** `GpuAllocator` keeps the blocks per memory type in a `HashMap<u32, Vec<PoolBlock>>`, and a record of every live buffer under a `BufferHandle`.
  - **`create_buffer`** creates a `VkBuffer`, asks for its memory requirements (size, alignment, allowed types), takes a piece of a device-local block, and *binds* the buffer to that piece. A buffer bigger than a block gets a new block, twice its size, of its own.
  - **`if ... && let Some(offset) = ...`** is a *let chain* (edition 2024): an `if` whose conditions mix ordinary tests and pattern matches.
  - **`free_buffer`** destroys the buffer and returns its piece to its block. `let ... else { return; }` handles an unknown handle.
  - **`Drop`** destroys any remaining buffers, then frees every block. `drain()` empties a map while iterating over it.
- **`MappedBuffer`** is a buffer in host-visible, coherent memory, mapped once and kept mapped. Its `destroy` is an `unsafe fn` rather than a `Drop`, because it needs the `Device`, and the owner calls it at the right moment.
- **The staging buffer:** to fill a device-local buffer, `create_buffer_with_data` copies the data into the mapped staging buffer, records a command buffer with one `cmd_copy_buffer` into the device-local buffer, submits it, and waits for the queue to go idle. It's generic over `T: Copy`, so it takes a slice of `Vertex` or of `u32` alike, and `size_of_val(data)` is the slice's size in bytes.
- **The ring buffer:** per-frame data, here each draw's matrices, needs a fresh place every frame, because the GPU may still be reading the previous frame's.
  - **Segments:** the ring's 4 MB are split into one segment per frame in flight. Each frame fills only its own, from the top: `begin_frame` resets the cursor, after the renderer has waited for that frame's fence, so the GPU is done with that segment.
  - **`push`** copies a `T` to the cursor and returns its offset. Offsets are rounded up to 256, the strictest uniform-buffer alignment any Vulkan GPU may require.
  - **Dynamic offsets:** the descriptor (8.6) points at the start of the ring, and each draw supplies its offset when binding it. No descriptor is ever rewritten.
- **`MemoryContext`** owns the allocator, the ring and the staging buffer, and destroys the two mapped buffers in its `Drop`. Its fields drop after its own `drop`, so the allocator's `Drop` runs last.
- **`AppResult<T>`** is `main.rs`'s alias for `Result<T, Box<dyn Error>>`, used by every module through `crate::AppResult`. `ok_or("...")?` turns a missing memory type into an error with a message.

### Code
`lecture_8/src/memory.rs`:
```rust
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
```

## 8.5 Instance, GPU and device: `src/device.rs`

### Why
Before anything is drawn, Vulkan needs a chain of objects: an instance (our connection to Vulkan), a surface (the window, as Vulkan sees it), a physical device (a GPU), a logical device (our session with that GPU), a queue (where work is submitted), and a swapchain (the images that take turns being shown in the window). The depth buffer and command pool are made here too, since everything else uses them.

### How
- **The instance:**
  - **Extensions:** SDL knows which instance extensions this platform's windows need, and `vulkan_instance_extensions` returns their names. Vulkan wants them as C strings, so each becomes a `CString` (an owned, `'\0'`-terminated string), and the instance gets a `Vec` of pointers to them. The `CString`s must outlive the call, which is why they have a variable of their own.
  - **Collecting `Result`s:** `collect::<Result<_, _>>()?` turns an iterator of `Result`s into one `Result` of a `Vec`, stopping at the first error.
  - **Validation:** in debug builds, we add the `VK_LAYER_KHRONOS_validation` layer, which checks every call against the specification, and the debug-utils extension, which reports what it finds.
- **The debug messenger:** `debug_callback` is an `unsafe extern "system" fn`, the calling convention Vulkan uses for callbacks. The layer calls it with warnings and errors, and we print them. Returning `FALSE` lets the call go ahead. `(*data).p_message` reads the message through the raw pointer the layer passed, inside an `unsafe` block with its own `// SAFETY:` comment.
- **The surface:** SDL creates it for our window. SDL's and ash's handle types are different Rust types for the same Vulkan handle, so the instance handle goes in, and the surface comes out, as raw integers: `as_raw()` and `from_raw()`.
- **Choosing a GPU:**
  - **Usable GPUs:** `filter` keeps the GPUs with Vulkan 1.3, the version the shaders are compiled for. Then `filter_map` keeps each one that has a queue family able to draw (`GRAPHICS`) and present to our surface, together with that family's index. A graphics queue can always copy as well. The `?` inside the closure skips a GPU with no such family: it returns `None` from the closure.
  - **The best one:** `min_by_key` takes the best-ranked of the rest: a discrete GPU, then an integrated one, then anything else, such as `llvmpipe`, which runs Vulkan on the CPU.
- **The device:** one queue from the chosen family, at priority 1.0, and the swapchain extension. `min_uniform_buffer_offset_alignment` is the GPU's alignment rule for uniform-buffer offsets, which the ring buffer needs.
- **The surface format:** whichever format the surface lists first. The order carries no promise, but every surface offers at least one.
- **The command pool:** command buffers are allocated from it. `RESET_COMMAND_BUFFER` lets each one be reset and recorded again every frame.
- **Swapchain-sized objects:** the swapchain, a view of each of its images, and the depth buffer all depend on the window's size, so `create_targets` makes all of them, and `recreate_swapchain` replaces all of them when the size changes. `new` makes the context with null handles first, then calls `recreate_swapchain` once, so one code path builds them both times.
  - **The extent:** most platforms report the surface's size; Wayland reports `u32::MAX`, meaning "whatever size the swapchain picks", so we use the window's pixel size, within the surface's limits.
  - **The image count:** one more than the minimum, so we're less likely to wait for the presentation engine to give an image back, unless the surface sets a maximum (`0` means none). With FIFO, some waiting is the point: it's what holds the loop to the refresh rate.
  - **FIFO:** frames are shown in order, one per display refresh, like vertical sync. Every Vulkan driver supports it.
  - **`old_swapchain`:** passing the old swapchain lets the driver reuse its resources. It's destroyed once the new one exists.
  - **Image views** say how to interpret an image: here, a plain 2D view of the whole image, as color or as depth.
  - **The depth buffer** is an image in device-local memory, the size of the swapchain, in the `D32_SFLOAT` format: one 32-bit float per pixel.
- **`destroy_targets`** destroys the views, the depth buffer and optionally the swapchain. `std::mem::take` returns each handle and leaves a null one behind (ash's handles are `Default`, and the default is null), so a failure halfway through recreating can't lead to destroying anything twice. Destroying a null handle does nothing.
- **`Drop`** destroys everything in reverse order of creation: children before parents, the device before the instance. It waits for the GPU to go idle first; `let _ =` ignores the result, since a `drop` can't return an error.
- **Failing during setup:** if a step of `new` fails, `?` returns early, and the objects made before it aren't destroyed. The program is about to exit with the error anyway, and the driver releases everything a process leaves behind; a library would have to undo those steps itself.
- **`_entry`:** the `Entry` holds Vulkan's top-level function pointers. The field keeps it alive as long as the instance; the `_` says nothing reads it.

### Code
`lecture_8/src/device.rs`:
```rust
use crate::AppResult;
use crate::memory::find_memory_type;

use ash::ext::debug_utils;
use ash::khr::{surface, swapchain};
use ash::vk::{self, Handle};
use ash::{Device, Entry, Instance};
use sdl3::video::{VkInstance, Window};
use std::ffi::{CStr, CString, c_char, c_void};

const APP_NAME: &CStr = c"VKE001";
pub const WINDOW_NAME: &str = "VKE001";
pub const DEPTH_FORMAT: vk::Format = vk::Format::D32_SFLOAT;

// The validation layer's messages

// Called by the validation layer, from inside whichever Vulkan call it's checking.
unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _: *mut c_void,
) -> vk::Bool32 {
    // SAFETY: the layer passes valid callback data, whose message is a '\0'-terminated string.
    let message = unsafe { CStr::from_ptr((*data).p_message) };
    println!("[{severity:?}] [{kind:?}]: {}", message.to_string_lossy());

    // FALSE: let the Vulkan call go ahead.
    vk::FALSE
}

// Swapchain-sized images

// Everything whose size follows the window's: the swapchain, a view of each of its images, and the depth buffer.
struct Targets {
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    extent: vk::Extent2D,
    depth_image: vk::Image,
    depth_memory: vk::DeviceMemory,
    depth_view: vk::ImageView,
}

// A 2D view of a whole single-layer, single-mip image.
fn create_view(device: &Device, image: vk::Image, format: vk::Format, aspect: vk::ImageAspectFlags) -> AppResult<vk::ImageView> {
    let range = vk::ImageSubresourceRange { aspect_mask: aspect, base_mip_level: 0, level_count: 1, base_array_layer: 0, layer_count: 1 };
    let info =
        vk::ImageViewCreateInfo::default().image(image).view_type(vk::ImageViewType::TYPE_2D).format(format).subresource_range(range);

    // SAFETY: device and image are live, and the view matches the image's format and layout.
    Ok(unsafe { device.create_image_view(&info, None)? })
}

// The device context

// The objects every frame needs: instance, GPU, device, queue, surface, swapchain and depth buffer.
pub struct DeviceContext {
    _entry: Entry,
    pub instance: Instance,
    pub device: Device,
    pub physical_device: vk::PhysicalDevice,
    pub queue: vk::Queue,
    pub surface: vk::SurfaceKHR,
    pub surface_loader: surface::Instance,
    pub surface_format: vk::SurfaceFormatKHR,
    pub swapchain_loader: swapchain::Device,
    pub swapchain: vk::SwapchainKHR,
    pub swapchain_images: Vec<vk::Image>,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub swapchain_extent: vk::Extent2D,
    pub command_pool: vk::CommandPool,
    pub depth_image: vk::Image,
    pub depth_image_view: vk::ImageView,
    pub depth_image_memory: vk::DeviceMemory,
    pub memory_properties: vk::PhysicalDeviceMemoryProperties,
    pub min_ubo_alignment: u64,
    debug: Option<(debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
}

impl DeviceContext {
    pub fn new(window: &Window, enable_validation: bool) -> AppResult<DeviceContext> {
        // SAFETY: each Vulkan call gets valid create infos and live parent objects, created just above it. Everything made here is destroyed in Drop.
        unsafe {
            // "linked": the Vulkan loader was linked at build time, so this can't fail.
            let entry = Entry::linked();
            let app_info = vk::ApplicationInfo::default().application_name(APP_NAME).engine_name(APP_NAME).api_version(vk::API_VERSION_1_3);

            // The instance extensions SDL needs to make a surface for this window, plus debug utils for validation messages.
            let sdl_extensions = window.vulkan_instance_extensions().map_err(|error| format!("SDL3 Vulkan extensions: {error}"))?;
            let extension_names: Vec<CString> = sdl_extensions.iter().map(|name| CString::new(name.as_str())).collect::<Result<_, _>>()?;
            let mut extensions: Vec<*const c_char> = extension_names.iter().map(|name| name.as_ptr()).collect();
            let mut layers: Vec<*const c_char> = Vec::new();
            if enable_validation {
                extensions.push(debug_utils::NAME.as_ptr());
                layers.push(c"VK_LAYER_KHRONOS_validation".as_ptr());
            }

            let instance_info = vk::InstanceCreateInfo::default()
                .application_info(&app_info)
                .enabled_extension_names(&extensions)
                .enabled_layer_names(&layers);
            let instance = entry.create_instance(&instance_info, None)?;

            // Warnings and errors from the validation layer go to debug_callback.
            let debug = if enable_validation {
                let loader = debug_utils::Instance::new(&entry, &instance);
                let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
                    .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING)
                    .message_type(
                        vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                            | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                            | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                    )
                    .pfn_user_callback(Some(debug_callback));
                let messenger = loader.create_debug_utils_messenger(&info, None)?;
                Some((loader, messenger))
            } else {
                None
            };

            // SDL creates the surface; its handle types are SDL's, so the handles cross over as raw integers.
            let surface_raw = window
                .vulkan_create_surface(instance.handle().as_raw() as usize as VkInstance)
                .map_err(|error| format!("Vulkan surface: {error}"))?;
            let surface = vk::SurfaceKHR::from_raw(surface_raw as u64);
            let surface_loader = surface::Instance::new(&entry, &instance);

            // Every GPU with Vulkan 1.3 (the shaders are compiled for it) and a queue family that can draw and present to our surface; a graphics queue can always copy too. Discrete GPUs first, then integrated, then anything else (such as llvmpipe, which runs on the CPU).
            let rank = |kind: vk::PhysicalDeviceType| match kind {
                vk::PhysicalDeviceType::DISCRETE_GPU => 0,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
                _ => 2,
            };
            let (physical_device, queue_family_index) = instance
                .enumerate_physical_devices()?
                .into_iter()
                .filter(|&physical_device| instance.get_physical_device_properties(physical_device).api_version >= vk::API_VERSION_1_3)
                .filter_map(|physical_device| {
                    let families = instance.get_physical_device_queue_family_properties(physical_device);
                    let index = (0..families.len() as u32).find(|&index| {
                        families[index as usize].queue_flags.contains(vk::QueueFlags::GRAPHICS)
                            && surface_loader.get_physical_device_surface_support(physical_device, index, surface).unwrap_or(false)
                    })?;
                    Some((physical_device, index))
                })
                .min_by_key(|&(physical_device, _)| rank(instance.get_physical_device_properties(physical_device).device_type))
                .ok_or("no GPU can draw to this window")?;

            let priorities = [1.0];
            let queue_infos = [vk::DeviceQueueCreateInfo::default().queue_family_index(queue_family_index).queue_priorities(&priorities)];
            let device_extensions = [swapchain::NAME.as_ptr()];
            let device_info = vk::DeviceCreateInfo::default().queue_create_infos(&queue_infos).enabled_extension_names(&device_extensions);
            let device = instance.create_device(physical_device, &device_info, None)?;

            let queue = device.get_device_queue(queue_family_index, 0);
            let memory_properties = instance.get_physical_device_memory_properties(physical_device);
            let properties = instance.get_physical_device_properties(physical_device);
            let min_ubo_alignment = properties.limits.min_uniform_buffer_offset_alignment;
            println!("✓ Using GPU: {}", properties.device_name_as_c_str().unwrap_or(c"<unknown>").to_string_lossy());

            // Whichever format the surface lists first. The order carries no promise; every surface offers at least one format.
            let surface_format = *surface_loader
                .get_physical_device_surface_formats(physical_device, surface)?
                .first()
                .ok_or("the surface has no formats")?;

            let swapchain_loader = swapchain::Device::new(&instance, &device);

            let pool_info = vk::CommandPoolCreateInfo::default()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);
            let command_pool = device.create_command_pool(&pool_info, None)?;

            let mut context = DeviceContext {
                _entry: entry,
                instance,
                device,
                physical_device,
                queue,
                surface,
                surface_loader,
                surface_format,
                swapchain_loader,
                swapchain: vk::SwapchainKHR::null(),
                swapchain_images: Vec::new(),
                swapchain_image_views: Vec::new(),
                swapchain_extent: vk::Extent2D::default(),
                command_pool,
                depth_image: vk::Image::null(),
                depth_image_view: vk::ImageView::null(),
                depth_image_memory: vk::DeviceMemory::null(),
                memory_properties,
                min_ubo_alignment,
                debug,
            };

            let (width, height) = window.size_in_pixels();
            context.recreate_swapchain(width, height)?;
            Ok(context)
        }
    }

    // Builds the swapchain and depth buffer at the window's new size, replacing the old ones. The first call has nothing to replace.
    pub fn recreate_swapchain(&mut self, width: u32, height: u32) -> AppResult<()> {
        // SAFETY: after device_wait_idle the GPU uses none of the old objects, so they can be destroyed; the new ones are made from valid infos.
        unsafe {
            self.device.device_wait_idle()?;
            let old_swapchain = self.swapchain;
            self.destroy_targets(false);

            let targets = self.create_targets(width, height, old_swapchain)?;
            if old_swapchain != vk::SwapchainKHR::null() {
                self.swapchain_loader.destroy_swapchain(old_swapchain, None);
            }

            self.swapchain = targets.swapchain;
            self.swapchain_images = targets.images;
            self.swapchain_image_views = targets.views;
            self.swapchain_extent = targets.extent;
            self.depth_image = targets.depth_image;
            self.depth_image_memory = targets.depth_memory;
            self.depth_image_view = targets.depth_view;
        }
        Ok(())
    }

    // SAFETY (for callers): the device must be idle.
    unsafe fn create_targets(&self, width: u32, height: u32, old_swapchain: vk::SwapchainKHR) -> AppResult<Targets> {
        unsafe {
            let caps = self.surface_loader.get_physical_device_surface_capabilities(self.physical_device, self.surface)?;

            // u32::MAX means the swapchain decides the surface's size (as on Wayland): use the window's pixel size, within the surface's limits.
            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                vk::Extent2D {
                    width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                    height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
                }
            };

            // One image more than the minimum, so we're less likely to wait for the presentation engine to release one. A max_image_count of 0 means no limit.
            let mut image_count = caps.min_image_count + 1;
            if caps.max_image_count > 0 {
                image_count = image_count.min(caps.max_image_count);
            }

            // FIFO: wait for the display's vertical blank, so the frame rate is the refresh rate. Every Vulkan driver supports it.
            let swapchain_info = vk::SwapchainCreateInfoKHR::default()
                .surface(self.surface)
                .min_image_count(image_count)
                .image_format(self.surface_format.format)
                .image_color_space(self.surface_format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(vk::PresentModeKHR::FIFO)
                .clipped(true)
                .old_swapchain(old_swapchain);
            let swapchain = self.swapchain_loader.create_swapchain(&swapchain_info, None)?;

            let images = self.swapchain_loader.get_swapchain_images(swapchain)?;
            let views = images
                .iter()
                .map(|&image| create_view(&self.device, image, self.surface_format.format, vk::ImageAspectFlags::COLOR))
                .collect::<AppResult<Vec<_>>>()?;

            // The depth buffer: one image, in device-local memory, the size of the swapchain.
            let depth_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(DEPTH_FORMAT)
                .extent(vk::Extent3D { width: extent.width, height: extent.height, depth: 1 })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let depth_image = self.device.create_image(&depth_info, None)?;

            let requirements = self.device.get_image_memory_requirements(depth_image);
            let memory_type_index =
                find_memory_type(&self.memory_properties, requirements.memory_type_bits, vk::MemoryPropertyFlags::DEVICE_LOCAL)
                    .ok_or("no device-local memory type for the depth buffer")?;
            let allocate_info = vk::MemoryAllocateInfo::default().allocation_size(requirements.size).memory_type_index(memory_type_index);
            let depth_memory = self.device.allocate_memory(&allocate_info, None)?;
            self.device.bind_image_memory(depth_image, depth_memory, 0)?;
            let depth_view = create_view(&self.device, depth_image, DEPTH_FORMAT, vk::ImageAspectFlags::DEPTH)?;

            Ok(Targets { swapchain, images, views, extent, depth_image, depth_memory, depth_view })
        }
    }

    // Destroys the views and the depth buffer, and the swapchain too if `swapchain` is true. Destroying a null handle does nothing, so this is safe before the first create.
    // SAFETY (for callers): the device must be idle.
    unsafe fn destroy_targets(&mut self, swapchain: bool) {
        unsafe {
            for &view in &self.swapchain_image_views {
                self.device.destroy_image_view(view, None);
            }
            self.swapchain_image_views.clear();

            // Null handles left behind can't be destroyed twice, even if making the new ones fails.
            self.device.destroy_image_view(std::mem::take(&mut self.depth_image_view), None);
            self.device.destroy_image(std::mem::take(&mut self.depth_image), None);
            self.device.free_memory(std::mem::take(&mut self.depth_image_memory), None);

            if swapchain {
                self.swapchain_loader.destroy_swapchain(std::mem::take(&mut self.swapchain), None);
            }
        }
    }
}

// Children before parents: everything made from the device goes before the device, and the device and surface before the instance.
impl Drop for DeviceContext {
    fn drop(&mut self) {
        // SAFETY: after device_wait_idle the GPU uses nothing, and the renderer, which holds objects made from this device, was dropped first.
        unsafe {
            let _ = self.device.device_wait_idle();
            self.destroy_targets(true);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            if let Some((loader, messenger)) = &self.debug {
                loader.destroy_debug_utils_messenger(*messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
```

## 8.6 Drawing: `src/renderer.rs`

### Why
The renderer owns everything that draws: the scene and its buffers, the pipeline that tells the GPU how to draw, the descriptors that connect the shaders to the ring buffer, the command buffers, and the semaphores and fences that keep the CPU, the GPU and the display in step.

### How
- **Uploading the cube:** all meshes' vertices go into one buffer, and all their indices into another, through the staging buffer. `flat_map(... .iter().copied())` flattens the meshes' `Vec`s into one sequence.
- **Frames in flight:** the CPU records frame N+1 while the GPU still draws frame N. Each of the two frame slots has its own command buffer, ring segment, "image available" semaphore and fence.
  - **A fence** is a signal from the GPU to the CPU: submitted work signals it when done, and `wait_for_fences` waits for it. The fences start signaled, so the first wait on each returns at once.
  - **A semaphore** is a signal from one piece of GPU work to another: "the image is acquired" to "start writing color", and "rendering is finished" to "present".
  - **`render_finished` per swapchain image:** presentation may still hold a frame's semaphore when that frame slot comes around again, so these are per image instead of per frame.
- **`render`, step by step:**
  1. **Wait** for this slot's fence: the GPU has finished the last frame that used this slot's command buffer and ring segment.
  2. **Acquire** a swapchain image, which signals `image_available` when it's ready to be written. `ERROR_OUT_OF_DATE_KHR` means the swapchain no longer matches the window: we return `true`, and `main` recreates it. The fence is reset only after a successful acquire, so a frame we skip doesn't leave it unsignaled forever.
  3. **Update** the scene by the real time since the last frame, and reset the ring segment.
  4. **Record** the frame's commands.
  5. **Submit:** wait for `image_available` before writing color, signal `render_finished` when done, and signal the fence for this slot.
  6. **Present** the image once `render_finished` is signaled. `Ok(true)` means *suboptimal*: it was shown, but the swapchain should be recreated. Either way, `render` returns whether the swapchain went stale.
- **`record`:** one render pass that clears the image to dark gray and the depth buffer to 1.0 (the far end), sets the viewport and scissor to the window, binds the pipeline and the buffers, and then, for each mesh, pushes its matrices into the ring, binds the descriptor set with that offset, and draws its indices. `record` is an `unsafe fn` because the command buffer must be recording; its body still marks its operations with an `unsafe` block, since edition 2024 warns about unsafe operations that aren't in one, even inside an `unsafe fn`.
- **Recreating:** after the swapchain is rebuilt, the framebuffers must be too, since they point at its images, and there may be a different number of images, so a different number of `render_finished` semaphores.
- **Descriptors:** a *descriptor* tells a shader where a resource is.
  - **The layout** has one binding: a `UNIFORM_BUFFER_DYNAMIC` for the vertex shader. "Dynamic" means the offset is given each time the set is bound, rather than written into the descriptor.
  - **The sets:** one per frame in flight, allocated from a pool, each pointing at offset 0 of the ring with the size of one `UniformBufferObject`.
  - **`type DescriptorSets`** names the `(pool, sets)` pair the function returns.
- **The pipeline** is the GPU's entire configuration for drawing, fixed at creation:
  - **Shader stages**, from SPIR-V. `include_bytes!` gives a `&[u8]` with no alignment promise, and Vulkan wants 32-bit words, so `spirv_words` copies the bytes into `u32`s with `as_chunks::<4>()` and `u32::from_le_bytes`.
  - **Vertex input:** one buffer of `Vertex`es, 24 bytes apart, position at location 0 and color at location 1, both three floats, at the offsets `offset_of!` reports.
  - **Rasterization:** fill triangles; counter-clockwise ones face us, and back faces are culled, so only the cube's outside is drawn.
  - **Depth:** a fragment is drawn only if it's nearer than what's already there (`LESS`), and then its depth is written.
  - **Dynamic state:** the viewport and scissor are set while recording, so the pipeline survives a resize.
  - **The layout** names the descriptor set layout the shaders use. The shader modules are only needed until the pipeline exists.
- **The render pass** describes the attachments and what happens to them: the swapchain image is cleared, drawn to, and left in the layout for presenting; the depth buffer is cleared, used, and thrown away (`DONT_CARE`). The *dependency* makes the pass's color and depth writes, including the clears, wait for earlier work:
  - **Color:** waiting on the color-output stage chains to the submit's wait for the acquired image, which happens at that stage.
  - **Depth:** the depth buffer is shared by every frame, so this frame's clear must come after the previous frame's depth writes. Those finish in the late fragment tests, so that's the source stage, and `DEPTH_STENCIL_ATTACHMENT_WRITE` the source access: a write after a write needs both.
- **Framebuffers** pair each swapchain image's view with the shared depth view.
- **`Drop`** waits for the GPU to go idle, destroys everything the renderer made, and returns the mesh buffers to the allocator. The `MemoryContext` field drops after that, freeing the ring, the staging buffer and every block.

### Code
`lecture_8/src/renderer.rs`:
```rust
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
```

## 8.7 The frame loop: `src/main.rs`

### Why
`main` opens the window, builds the three contexts in order, and runs the frame loop: handle every pending event, then draw one frame, until the window closes.

### How
- **`AppResult`** is declared here, at the crate root, so every module can use `crate::AppResult`.
- **The window:** SDL3, `resizable()` and `vulkan()`. `sdl3::init()` returns a `Result` whose error converts to `Box<dyn Error>`, so `?` works on it, and on everything after it.
- **Construction order:** the `DeviceContext`, then a `MemoryContext` made from its device, then the `Renderer`, which takes ownership of the `MemoryContext`. Locals drop in reverse order, so at the end of `main` the renderer and its memory go first, then the device context, then the window and SDL: every Vulkan object is destroyed before the device it came from.
- **`device.clone()`:** ash's `Device` is a handle plus a table of function pointers. Cloning it copies those, and doesn't create a second device; the `DeviceContext` alone destroys it.
- **Events:**
  - **`'running: loop`** is a labeled loop, so `break 'running` inside the `for` over events leaves the outer loop.
  - **Patterns:** `Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. }` matches either variant; `..` ignores the other fields.
  - **Resizing:** SDL3 reports a window's size in *points* in `Resized`, and in pixels in `PixelSizeChanged`; on a high-DPI display they differ, and the swapchain needs pixels.
  - **Minimizing:** a minimized window has nothing to draw to, so `minimized` stops rendering, and the loop blocks in `wait_event()` instead of spinning. That event is then handled with the rest: `waited.into_iter().chain(event_pump.poll_iter())` puts an `Option`'s 0 or 1 events in front of the pending ones.
- **Recreating:** `recreate` rebuilds the swapchain at the window's current pixel size, then the renderer's framebuffers. A zero-sized window can't have a swapchain, so it waits for a real size. Both a resize event and a stale swapchain from `render` lead here.
- **Time:** `Instant` measures the real time between frames, so the cube turns at the same speed at any refresh rate. While minimized, the clock is kept current, so the cube doesn't jump when the window comes back.

### Code
`lecture_8/src/main.rs`:
```rust
mod device;
mod memory;
mod renderer;
mod scene;

use device::{DeviceContext, WINDOW_NAME};
use memory::{ENABLE_VALIDATION, HEIGHT, MemoryContext, WIDTH};
use renderer::Renderer;

use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Keycode;
use std::error::Error;
use std::time::Instant;

// Every fallible function in this program returns this: any error, boxed.
pub type AppResult<T> = Result<T, Box<dyn Error>>;

// Builds the swapchain again at the window's current pixel size, and everything that depends on it.
fn recreate(window: &sdl3::video::Window, device_context: &mut DeviceContext, renderer: &mut Renderer) -> AppResult<()> {
    // A zero-sized window can't have a swapchain; drawing resumes when it has a size again.
    let (width, height) = window.size_in_pixels();
    if width == 0 || height == 0 {
        return Ok(());
    }

    device_context.recreate_swapchain(width, height)?;
    renderer.recreate_framebuffers(device_context)
}

fn main() -> AppResult<()> {
    println!("\n╔════════════════════════════════════╗");
    println!("║        VULKAN RENDERER             ║");
    println!("║   VMA-Style Memory Architecture    ║");
    println!("╚════════════════════════════════════╝");

    // SDL3: a resizable window that Vulkan can draw to.
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video.window(WINDOW_NAME, WIDTH, HEIGHT).position_centered().resizable().vulkan().build()?;

    let mut device_context = DeviceContext::new(&window, ENABLE_VALIDATION)?;
    println!("✓ Device initialized (UBO alignment: {})", device_context.min_ubo_alignment);

    // The memory system: pool allocator, ring buffer and staging buffer.
    let memory = MemoryContext::new(device_context.device.clone(), device_context.memory_properties, device_context.min_ubo_alignment)?;
    println!("✓ Memory system initialized");

    // The renderer takes ownership of the memory context.
    let mut renderer = Renderer::new(&device_context, memory)?;
    println!("✓ Renderer initialized");

    let mut event_pump = sdl.event_pump()?;
    let mut minimized = false;
    let mut last_frame = Instant::now();
    println!("\nStarting render loop...\n");

    'running: loop {
        // A minimized window has nothing to draw to: sleep until an event arrives, instead of spinning. An Option chains in as 0 or 1 events.
        let waited = if minimized { Some(event_pump.wait_event()) } else { None };

        for event in waited.into_iter().chain(event_pump.poll_iter()) {
            match event {
                Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'running,

                // SDL3 reports the window's size in points in Resized; the swapchain needs pixels.
                Event::Window { win_event: WindowEvent::PixelSizeChanged(width, height), .. } => {
                    println!("Window resized: {width}x{height}");
                    recreate(&window, &mut device_context, &mut renderer)?;
                }

                Event::Window { win_event: WindowEvent::Minimized, .. } => minimized = true,
                Event::Window { win_event: WindowEvent::Restored | WindowEvent::Maximized, .. } => minimized = false,

                _ => {}
            }
        }

        // While minimized, keep the clock current, so the cube doesn't jump when the window comes back.
        if minimized {
            last_frame = Instant::now();
            continue;
        }

        // The cube turns by real time, so it spins at the same speed at any refresh rate.
        let now = Instant::now();
        let delta_time = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;

        // true: the swapchain went stale (out of date or suboptimal), so build it again before the next frame.
        if renderer.render(&device_context, delta_time)? {
            recreate(&window, &mut device_context, &mut renderer)?;
        }
    }

    // Locals drop in reverse order: renderer (and the MemoryContext inside it), then device_context, then the window and SDL.
    println!("\nShutting down...");
    Ok(())
}
```

## 8.8 Build and run

```bash
cargo run -p lecture_8
```

Or run `./build.bash`, choose `lecture_8`, and press Enter. The first build compiles SDL3 from source, which takes a minute or two.

On the machine this was written on:
```text

╔════════════════════════════════════╗
║        VULKAN RENDERER             ║
║   VMA-Style Memory Architecture    ║
╚════════════════════════════════════╝
✓ Using GPU: NVIDIA GeForce RTX 5070 Laptop GPU
✓ Device initialized (UBO alignment: 64)
[GpuAllocator] Heap 0: 7.96 GB (VRAM)
[GpuAllocator] Heap 1: 45.92 GB (System)
[RingBuffer] 4.00 MB total, 2.00 MB/frame, alignment 256
[MemoryContext] Staging belt: 32.00 MB
✓ Memory system initialized
[GpuAllocator] New 64 MB block for memory type 1 (now 1 blocks)
  [type 1] block 64.00 MB  used 720 B  free 64.00 MB  regions 1
[GpuAllocator] 1 blocks, 720 B/64.00 MB used, 2 live buffers
✓ Renderer initialized

Starting render loop...

Window resized: 1920x1080

Shutting down...
```

A window opens with the spinning cube; close it, or press Escape. In a debug build the validation layer is on, and any mistake it finds is printed as a `[WARNING]` or `[ERROR]` line. There should be none, including while resizing and minimizing the window.

Reading the output:
- **The GPU:** the best-ranked GPU that can present to the window.
- **Memory:** the heaps the GPU reports, the ring buffer's segments, and the staging buffer. The only pool block holds the cube's 24 vertices (576 bytes) and 36 indices (144 bytes): 720 bytes of a 64 MB block, in 2 live buffers.
- **`Window resized`** appears whenever the window's pixel size changes, including once at startup on some platforms, as the compositor settles the window's size.

That's the whole course: from a vending machine to a GPU-driven renderer, with every line on both sides of the C boundary written or read along the way.
