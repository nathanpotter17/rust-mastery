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
