use crate::ffi::{self, MiniBody, MiniBodyDesc, MiniStatus, MiniVec2};

use std::error::Error;
use std::ffi::{CStr, c_void};
use std::fmt;
use std::ops::Add;
use std::ptr::{self, NonNull};

// Vectors

// The generated types live in our crate, so we can give them methods and traits like any of our own types.
impl MiniVec2 {
    pub fn new(x: f32, y: f32) -> MiniVec2 {
        MiniVec2 { x, y }
    }

    pub fn length(self) -> f32 {
        // SAFETY: plain values in, a plain value out: nothing for C to get wrong.
        unsafe { ffi::mini_vec2_length(self) }
    }
}

impl Add for MiniVec2 {
    type Output = MiniVec2;

    fn add(self, other: MiniVec2) -> MiniVec2 {
        // SAFETY: as for length.
        unsafe { ffi::mini_vec2_add(self, other) }
    }
}

// Display formats through C's snprintf, to show a C function filling a buffer we own. The classic two calls: measure, then write.
impl fmt::Display for MiniVec2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: with capacity 0, C writes nothing, and only returns the length the text needs.
        let needed = unsafe { ffi::mini_vec2_format(*self, ptr::null_mut(), 0) };

        // Room for the text and its '\0'.
        let mut buffer = vec![0u8; needed + 1];

        // SAFETY: the pointer and capacity describe our buffer exactly, and C writes at most that many bytes.
        unsafe { ffi::mini_vec2_format(*self, buffer.as_mut_ptr().cast(), buffer.len()) };

        // The text runs up to the first '\0'.
        let text = CStr::from_bytes_until_nul(&buffer).expect("snprintf ends the text with a '\\0'");
        f.write_str(&text.to_string_lossy())
    }
}

// A slice is a pointer and a length: the two arguments C wants.
pub fn sum(values: &[f32]) -> f32 {
    // SAFETY: values.as_ptr() points at values.len() floats, which C only reads.
    unsafe { ffi::mini_sum(values.as_ptr(), values.len()) }
}

// Errors

// Any status but MINI_OK, with C's message for it.
#[derive(Debug)]
pub struct MiniError(MiniStatus);

impl fmt::Display for MiniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: mini_status_text returns a '\0'-terminated string in static memory, for any status.
        let text = unsafe { CStr::from_ptr(ffi::mini_status_text(self.0)) };
        write!(f, "{} (status {})", text.to_string_lossy(), self.0.0)
    }
}

impl Error for MiniError {}

fn check(status: MiniStatus) -> Result<(), MiniError> {
    if status == MiniStatus::MINI_OK { Ok(()) } else { Err(MiniError(status)) }
}

// The world

// Owns one C++ MiniWorld through its pointer. NonNull: never null, and (unlike a reference) free of any borrow.
pub struct World {
    raw: NonNull<ffi::MiniWorld>,
}

impl World {
    pub fn new(gravity: MiniVec2, ground: f32) -> Result<World, MiniError> {
        // SAFETY: plain values in; the result is a new world or null.
        let raw = unsafe { ffi::mini_world_create(gravity, ground) };
        NonNull::new(raw).map(|raw| World { raw }).ok_or(MiniError(MiniStatus::MINI_OUT_OF_MEMORY))
    }

    pub fn add_body(&mut self, desc: &MiniBodyDesc) -> Result<u32, MiniError> {
        let mut id = 0;

        // SAFETY: raw is a live world, desc is only read, and &mut id is valid to write.
        check(unsafe { ffi::mini_world_add_body(self.raw.as_ptr(), desc, &mut id) })?;
        Ok(id)
    }

    pub fn body(&self, id: u32) -> Result<MiniBody, MiniError> {
        // MaybeUninit would avoid this zeroing, but a zeroed MiniBody is valid and simpler.
        let mut body = MiniBody { position: MiniVec2::new(0.0, 0.0), velocity: MiniVec2::new(0.0, 0.0), bounces: 0 };

        // SAFETY: raw is a live world, and &mut body is valid to write.
        check(unsafe { ffi::mini_world_body(self.raw.as_ptr(), id, &mut body) })?;
        Ok(body)
    }

    pub fn body_count(&self) -> usize {
        // SAFETY: raw is a live world.
        unsafe { ffi::mini_world_body_count(self.raw.as_ptr()) }
    }

    // Advances the world by dt seconds, calling on_contact(body, speed) for every bounce. The closure may borrow anything: C only holds on to it until step returns.
    pub fn step<F: FnMut(u32, f32)>(&mut self, dt: f32, mut on_contact: F) {
        // C can't call a Rust closure, only a function pointer. contact_trampoline::<F> is a plain function, made for this F, that turns `user` back into the closure and calls it.
        let user: *mut F = &mut on_contact;

        // SAFETY: raw is a live world. user points at on_contact, which outlives the step, and the callback is cleared before on_contact goes away.
        unsafe {
            ffi::mini_world_set_contact_callback(self.raw.as_ptr(), Some(contact_trampoline::<F>), user.cast());
            ffi::mini_world_step(self.raw.as_ptr(), dt);
            ffi::mini_world_set_contact_callback(self.raw.as_ptr(), None, ptr::null_mut());
        }
    }
}

// Called by C. extern "C" gives it C's calling convention, so it fits MiniContactFn. A panic can't unwind into C: if the closure panics, the program aborts.
unsafe extern "C" fn contact_trampoline<F: FnMut(u32, f32)>(user: *mut c_void, body: u32, speed: f32) {
    // SAFETY: step passed a *mut F as user, and the closure is alive for the whole step.
    let on_contact = unsafe { &mut *user.cast::<F>() };
    on_contact(body, speed);
}

// The C++ destructor runs here, once, when the World is dropped.
impl Drop for World {
    fn drop(&mut self) {
        // SAFETY: raw came from mini_world_create, and is destroyed only here.
        unsafe { ffi::mini_world_destroy(self.raw.as_ptr()) };
    }
}
