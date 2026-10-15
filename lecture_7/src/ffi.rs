// The raw C functions. Everything here is unsafe to call: the compiler can't check what C does with our pointers.

// The generated bindings: build.rs wrote them into OUT_DIR, and include! pastes that file in here at compile time.
include!(concat!(env!("OUT_DIR"), "/lodepng.rs"));

// One binding written by hand: C's free(), from the C library every Rust program on Linux already links against. This is all bindgen writes for a function, too.
unsafe extern "C" {
    pub fn free(pointer: *mut std::ffi::c_void);
}
