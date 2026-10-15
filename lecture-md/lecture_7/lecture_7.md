# Lecture 7: A third-party C library

By the end of this lecture we'll encode a generated gradient as a PNG, in memory, write it to a file, load it back, and prove the round trip was lossless. The PNG work is done by LodePNG, a widely used PNG encoder and decoder written in C, which we vendor into the lecture and call through bindings.

Lecture 6 bound a library we wrote ourselves, with an interface designed for it. A real third-party library is bigger, wasn't written with Rust in mind, and has its own rules, especially about memory. This lecture covers:
- *vendoring*: copying a library's source into the project, pinned to an exact version;
- binding three functions out of a 2000-line header;
- memory that C allocates and Rust must give back to C's `free`, owned by a Rust type with `Drop`;
- `Deref`, so a C buffer acts like a Rust slice;
- error codes that can still leave memory to free.

## 7.1 Vendoring LodePNG

### Why
PNG means zlib compression, filters and checksums: a lot of code we don't want to write. **LodePNG** packs a complete encoder and decoder into one header and one source file, with no dependencies, which makes it easy to vendor. Vendoring means the build never depends on the network or on what's installed on the system, and the exact version is fixed.

### How
- **Where it goes:** `lecture_7/vendor/lodepng/`. The repo's `.gitignore` keeps every `lecture_*/vendor/` out of git, so third-party code stays out of our history; the commands below fetch it again.
- **The version:** the URLs name a commit, `e584a849`, LodePNG version 20261006, rather than the `master` branch, so they always fetch the same files.
- **`lodepng.cpp` as `lodepng.c`:** LodePNG's source compiles as C or as C++. Compiled as C++, it adds a C++ interface built on `std::vector`; compiled as C, it's plain C, which is all we need. The C compiler (clang or gcc) decides the language from the file extension, so even though `cc` calls the C compiler, a `.cpp` file would be compiled as C++. We save it as `lodepng.c`, as LodePNG's own documentation suggests for C projects.
- **The crate's dependencies** are lecture 6's: `cc` and `bindgen`, for the build script only.

### Code
From the repo root:
```bash
cargo new lecture_7
```
```bash
mkdir -p lecture_7/vendor/lodepng
```
```bash
curl -L -o lecture_7/vendor/lodepng/lodepng.h https://raw.githubusercontent.com/lvandeve/lodepng/e584a8490db1b76b81bf298c11c09d3b55d9c270/lodepng.h
```
```bash
curl -L -o lecture_7/vendor/lodepng/lodepng.c https://raw.githubusercontent.com/lvandeve/lodepng/e584a8490db1b76b81bf298c11c09d3b55d9c270/lodepng.cpp
```

`lecture_7/Cargo.toml`:
```toml
[package]
name = "lecture_7"
version = "0.1.0"
edition = "2024"

# build.rs runs before the crate compiles. Its dependencies are build-only: neither ends up in lecture_7 itself.
[build-dependencies]
bindgen = "0.73"
cc = "1"
```

## 7.2 Building it: `build.rs`

### Why
The same two steps as lecture 6: compile the C source, and generate bindings from the header. The difference is the size of the header, and that we want only a sliver of it.

### How
- **One C file:** `cc::Build::new().file("vendor/lodepng/lodepng.c").compile("lodepng")` builds `liblodepng.a` and links it. LodePNG needs nothing but the C standard library, which every Rust program on Linux already links.
- **Three functions:** `lodepng.h` declares well over a hundred functions, structs and enums. `allowlist_function` keeps only the three we call: `lodepng_encode24` (RGB pixels to PNG bytes), `lodepng_decode24` (PNG bytes to RGB pixels) and `lodepng_error_text`. The fewer declarations we bind, the fewer we have to trust.
- **`rerun-if-changed=vendor/lodepng`:** fetching a new version of the library reruns the build script.

### Code
`lecture_7/build.rs`:
```rust
use std::env;
use std::path::PathBuf;

// Cargo compiles and runs this program before compiling the crate. It talks back to Cargo by printing cargo:: lines.
fn main() {
    // Run this script again only when the vendored library changes, not on every build.
    println!("cargo::rerun-if-changed=vendor/lodepng");

    // Compile the C file into a static library, liblodepng.a, and tell Cargo to link it into lecture_7.
    cc::Build::new().file("vendor/lodepng/lodepng.c").compile("lodepng");

    // bindgen reads the header with libclang, the same C parser clang uses, and writes a Rust declaration for every C declaration we allow. The rest of lodepng.h is skipped.
    let bindings = bindgen::Builder::default()
        .header("vendor/lodepng/lodepng.h")
        .allowlist_function("lodepng_encode24|lodepng_decode24|lodepng_error_text")
        .rust_edition(bindgen::RustEdition::Edition2024)
        .generate()
        .expect("lodepng.h parses");

    // OUT_DIR is a directory Cargo gives each build script for generated files.
    let out = PathBuf::from(env::var("OUT_DIR").expect("Cargo sets OUT_DIR"));
    bindings.write_to_file(out.join("lodepng.rs")).expect("OUT_DIR is writable");
}
```

## 7.3 The bindings: `src/ffi.rs`

### Why
As in lecture 6, the generated declarations go in their own module. LodePNG has one more requirement: the memory it returns was allocated with C's `malloc`, and must be freed with C's `free`. That's a C standard library function, so we declare it ourselves.

### How
- **The generated code** is the three functions, and nothing else:
  - `unsigned char **out` becomes `*mut *mut c_uchar`: a pointer to the caller's pointer variable, which C sets to the buffer it allocates. That's C's way of returning a pointer through a parameter.
  - `unsigned` becomes `c_uint` and `size_t` becomes `usize`. On our platforms `c_uint` is an alias for `u32`, so we pass `u32`s.
  - `lodepng_error_text` returns `*const c_char`: a C string.
- **A hand-written binding:** `unsafe extern "C" { pub fn free(pointer: *mut c_void); }` is what bindgen would write for `void free(void *)`, up to the parameter's name. Declarations are just promises about a symbol's name and signature; the linker finds `free` in the C library. Getting the signature wrong is undefined behavior, which is why we let bindgen write them whenever a header exists.

### Code
`lecture_7/src/ffi.rs`:
```rust
// The raw C functions. Everything here is unsafe to call: the compiler can't check what C does with our pointers.

// The generated bindings: build.rs wrote them into OUT_DIR, and include! pastes that file in here at compile time.
include!(concat!(env!("OUT_DIR"), "/lodepng.rs"));

// One binding written by hand: C's free(), from the C library every Rust program on Linux already links against. This is all bindgen writes for a function, too.
unsafe extern "C" {
    pub fn free(pointer: *mut std::ffi::c_void);
}
```

`target/debug/build/lecture_7-<hash>/out/lodepng.rs`, generated:
```rust
/* automatically generated by rust-bindgen 0.73.2 */

unsafe extern "C" {
    pub fn lodepng_decode24(
        out: *mut *mut ::std::os::raw::c_uchar,
        w: *mut ::std::os::raw::c_uint,
        h: *mut ::std::os::raw::c_uint,
        in_: *const ::std::os::raw::c_uchar,
        insize: usize,
    ) -> ::std::os::raw::c_uint;
}
unsafe extern "C" {
    pub fn lodepng_encode24(
        out: *mut *mut ::std::os::raw::c_uchar,
        outsize: *mut usize,
        image: *const ::std::os::raw::c_uchar,
        w: ::std::os::raw::c_uint,
        h: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_uint;
}
unsafe extern "C" {
    pub fn lodepng_error_text(code: ::std::os::raw::c_uint) -> *const ::std::os::raw::c_char;
}
```

## 7.4 The safe wrapper: `src/png.rs`

### Why
Every LodePNG call has rules that its header states only in comments: how big the input must be, who frees the output, and what an error code means. The wrapper turns each rule into something Rust enforces, so the rest of the program never writes `unsafe`.

### How
- **Errors:** LodePNG returns `0` for success, or an error code that `lodepng_error_text` describes. `PngError` wraps the code, its `Display` asks C for the text, and `check` turns a status into a `Result`, just as lecture 6's `MiniError` did.
- **Memory that C owns:**
  - **The rule:** memory from C's `malloc` must be released with C's `free`. Rust's allocator might be a different allocator altogether, so handing C's memory to a `Box` or a `Vec` would be undefined behavior.
  - **`CBuffer`** owns a pointer and a length, and its `Drop` calls `ffi::free`. That's lecture 2's `TempFile` pattern again, for memory: one owner, and a `Drop` that releases.
  - **`from_raw`** is an `unsafe fn`: calling it is a promise, spelled out in its `// SAFETY:` comment, that the pointer came from `malloc` and points at `len` bytes nobody else owns. It returns `None` for a null pointer.
- **`Deref`:** implementing `Deref<Target = [u8]>` lets a `CBuffer` be used wherever a `&[u8]` is expected: `encoded.len()`, `&encoded[..8]`, `*buffer == *other`. Method calls and `&` conversions apply `deref` automatically. `Vec<u8>` works the same way: it derefs to `[u8]`, which is where its slice methods come from.
- **Encoding:**
  - **The input size:** LodePNG reads `width * height * 3` bytes from `pixels`, so a shorter slice would make C read past the end. `assert_eq!` checks it in every build, not just debug builds: it guards memory safety, not just our own logic.
  - **Out-parameters:** `png` and `size` start as null and zero; `&mut png` converts to the `*mut *mut u8` C wants, and C writes the buffer's address and size into them.
- **Taking ownership before checking:** both LodePNG functions can return a buffer together with an error. The encoder's cleanup always hands over whatever it has written so far, and the decoder can fail after allocating its output. So both functions wrap whatever pointer came back in a `CBuffer` first, and only then check the status. If the call failed, `?` returns the error, the `CBuffer` is dropped, and the memory is freed either way. On a failed decode the size is unknown, so the buffer gets length `0`: it will never be read, only freed.
- **`Image`** keeps the decoded pixels in their `CBuffer`, with no copy into a `Vec`. `at(x, y)` reads one pixel's three bytes as an array.

### Code
`lecture_7/src/png.rs`:
```rust
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
```

## 7.5 The program: `src/main.rs`

### Why
`main` runs the round trip: generate, encode, write, read, decode, compare. The C library only ever sees memory; reading and writing the file is plain Rust.

### How
- **`make_gradient`** builds the test image: red grows left to right, green grows top to bottom, and blue stays at 128, so the corner pixels are known values. `extend` appends each pixel's three bytes. `x * 255 / (width - 1)` is at most 255, so the `as u8` loses nothing.
- **The PNG signature:** every PNG starts with the same 8 bytes; bytes 1 to 3 spell `PNG`. `String::from_utf8_lossy` turns bytes into text.
- **`fs::write(&path, &*encoded)`:** `*encoded` derefs the `CBuffer` to `[u8]`, and `&` makes the slice `fs::write` wants. `fs::read` returns a `Vec<u8>`, which derefs to `&[u8]` for `decode_rgb` in the same way.
- **Comparing:** `*loaded.pixels != *original` compares a C-owned buffer with a `Vec`, both as `[u8]`. A mismatch returns an error: `"...".into()` converts the text into a `Box<dyn Error>`.
- **Destructuring an array:** `let [red, green, blue] = loaded.at(x, y);` names the three bytes.
- **Garbage in:** `decode_rgb(b"not a png")` shows a C error code coming back as a Rust error with LodePNG's message. `b"..."` is a byte string, a `&[u8; N]`.

### Code
`lecture_7/src/main.rs`:
```rust
mod ffi;
mod png;

use std::error::Error;
use std::fs;

// Red increases left to right, green top to bottom, blue is constant.
fn make_gradient(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);
    for y in 0..height {
        for x in 0..width {
            pixels.extend([(x * 255 / (width - 1)) as u8, (y * 255 / (height - 1)) as u8, 128]);
        }
    }
    pixels
}

fn main() -> Result<(), Box<dyn Error>> {
    let (width, height) = (64, 48);
    let original = make_gradient(width, height);

    // Encode in memory. Every PNG starts with the same 8 bytes; bytes 1 to 3 spell PNG.
    let encoded = png::encode_rgb(&original, width, height)?;
    println!("Encoded {} bytes, starting {:?} ({})", encoded.len(), &encoded[..8], String::from_utf8_lossy(&encoded[1..4]));

    // The file is plain std: the C library only ever sees memory.
    let path = std::env::temp_dir().join("lecture_7_gradient.png");
    fs::write(&path, &*encoded)?;
    println!("Wrote {}", path.display());

    let loaded = png::decode_rgb(&fs::read(&path)?)?;
    println!("Loaded {}x{}", loaded.width, loaded.height);

    // PNG is lossless, so every byte should survive the round trip. Deref lets us compare the C buffer with the Vec as two [u8]s.
    if *loaded.pixels != *original {
        return Err("the round trip changed the pixels".into());
    }

    for (x, y) in [(0, 0), (63, 0), (0, 47), (63, 47)] {
        let [red, green, blue] = loaded.at(x, y);
        println!("  pixel ({x:2}, {y:2}) = rgb({red:3}, {green:3}, {blue:3})");
    }
    println!("Round trip OK");

    // C's error codes come back as Rust errors, with LodePNG's own message.
    if let Err(error) = png::decode_rgb(b"not a png") {
        println!("Decoding garbage: {error}");
    }

    Ok(())
}
```

## 7.6 Build and run

```bash
cargo run -p lecture_7
```

```text
Encoded 169 bytes, starting [137, 80, 78, 71, 13, 10, 26, 10] (PNG)
Wrote /tmp/lecture_7_gradient.png
Loaded 64x48
  pixel ( 0,  0) = rgb(  0,   0, 128)
  pixel (63,  0) = rgb(255,   0, 128)
  pixel ( 0, 47) = rgb(  0, 255, 128)
  pixel (63, 47) = rgb(255, 255, 128)
Round trip OK
Decoding garbage: LodePNG error 27: PNG file is smaller than a PNG header
```

Reading the output:
- **169 bytes** hold 64 × 48 × 3 = 9216 bytes of pixels: a smooth gradient compresses very well.
- **The signature,** `137 80 78 71 13 10 26 10`, is the same at the start of every PNG.
- **The corners** show the gradient: red is 0 on the left and 255 on the right, green is 0 at the top and 255 at the bottom, and blue is 128 everywhere. Every byte came back unchanged.
- **Garbage:** nine bytes are too short to be a PNG, and LodePNG's own message says so.

In lecture 8 we put pixels on screen: a Vulkan renderer, with SDL3 for the window, and a GPU memory allocator of our own.
