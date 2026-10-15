use crate::ffi;

use std::error::Error;
use std::ffi::CStr;
use std::fmt;
use std::ops::Deref;
use std::ptr::{self, NonNull};
use std::slice;

// Errors

// LodePNG reports failure as a number, and lodepng_error_text turns the number into a message.
#[derive(Debug)]
pub struct PngError(u32);

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // SAFETY: lodepng_error_text returns a '\0'-terminated string that lives for the whole program, for any code.
        let text = unsafe { CStr::from_ptr(ffi::lodepng_error_text(self.0)) };
        write!(f, "LodePNG error {}: {}", self.0, text.to_string_lossy())
    }
}

impl Error for PngError {}

// 0 means success; anything else is an error code.
fn check(status: u32) -> Result<(), PngError> {
    if status == 0 { Ok(()) } else { Err(PngError(status)) }
}

// Memory that C owns

// A buffer from C's malloc. It must go back to C's free, never to Rust's allocator, so this type owns it and frees it in Drop.
pub struct CBuffer {
    pointer: NonNull<u8>,
    len: usize,
}

impl CBuffer {
    // None for a null pointer.
    // SAFETY: a non-null pointer must come from malloc, point at len initialized bytes, and be owned by nothing else.
    unsafe fn from_raw(pointer: *mut u8, len: usize) -> Option<CBuffer> {
        NonNull::new(pointer).map(|pointer| CBuffer { pointer, len })
    }
}

// Deref makes a CBuffer act like the [u8] it holds: buffer.len(), &buffer[..8], *buffer == *other.
impl Deref for CBuffer {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        // SAFETY: from_raw's caller promised len initialized bytes, and we never hand out a way to free them.
        unsafe { slice::from_raw_parts(self.pointer.as_ptr(), self.len) }
    }
}

impl Drop for CBuffer {
    fn drop(&mut self) {
        // SAFETY: the pointer came from malloc, and only this CBuffer frees it.
        unsafe { ffi::free(self.pointer.as_ptr().cast()) };
    }
}

// Images

// 8-bit RGB pixels, row-major: 3 bytes per pixel.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: CBuffer,
}

impl Image {
    pub fn at(&self, x: u32, y: u32) -> [u8; 3] {
        let start = (y as usize * self.width as usize + x as usize) * 3;
        [self.pixels[start], self.pixels[start + 1], self.pixels[start + 2]]
    }
}

// Encoding and decoding

// Encodes width * height RGB pixels as a PNG, in memory.
pub fn encode_rgb(pixels: &[u8], width: u32, height: u32) -> Result<CBuffer, PngError> {
    // LodePNG reads width * height * 3 bytes from pixels, so a shorter slice would make it read past the end. assert! stays on in release builds: this check guards memory safety.
    assert_eq!(pixels.len(), width as usize * height as usize * 3, "pixels must hold width * height RGB pixels");

    // Out-parameters: C writes its results through pointers to these.
    let mut png: *mut u8 = ptr::null_mut();
    let mut size: usize = 0;

    // SAFETY: pixels is as long as checked above, and &mut png and &mut size are valid to write through.
    let status = unsafe { ffi::lodepng_encode24(&mut png, &mut size, pixels.as_ptr(), width, height) };

    // Take ownership before checking the status, so that a buffer is freed even if C hands one back with an error.
    // SAFETY: png is null or a malloc'd buffer of size bytes, and it's ours now.
    let png = unsafe { CBuffer::from_raw(png, size) };
    check(status)?;
    Ok(png.expect("LodePNG returns a buffer on success"))
}

// Decodes a PNG from memory into RGB pixels, whatever color type it stores.
pub fn decode_rgb(png: &[u8]) -> Result<Image, PngError> {
    let mut pixels: *mut u8 = ptr::null_mut();
    let mut width = 0;
    let mut height = 0;

    // SAFETY: png's pointer and length describe valid bytes; the three out-parameters are valid to write through.
    let status = unsafe { ffi::lodepng_decode24(&mut pixels, &mut width, &mut height, png.as_ptr(), png.len()) };

    // On failure, the size is unknown, but the buffer (if any) still needs freeing: an empty view of it is enough for that.
    let len = if status == 0 { width as usize * height as usize * 3 } else { 0 };

    // SAFETY: pixels is null or a malloc'd buffer of len bytes, and it's ours now.
    let pixels = unsafe { CBuffer::from_raw(pixels, len) };
    check(status)?;
    Ok(Image { width, height, pixels: pixels.expect("LodePNG returns pixels on success") })
}
