# Lecture 6: Calling C and C++

By the end of this lecture a Rust program drops a ball and a crate onto the ground in a tiny physics world written in C++, and prints where they land. Along the way it calls C functions with structs, arrays and text buffers, and C++ calls a Rust closure back for every bounce.

Most of the world's systems code is C and C++: operating systems, graphics drivers, physics engines, codecs. Rust calls it through the *foreign function interface* (FFI): Rust and C agree on how to call a function and how to lay out a struct, and Rust declares the C functions it wants. Writing those declarations by hand is tedious and easy to get wrong, so a tool generates them from the C header. This lecture writes a small library of our own, so every line on both sides of the boundary is ours to read:
- a C header that both languages share, and an `extern "C"` interface over C++ code;
- a build script (`build.rs`) that compiles C and C++ with the `cc` crate;
- `bindgen`, which reads the header and writes the Rust declarations, and what it generates for each kind of C declaration;
- `#[repr(C)]` structs, enums that C may return, opaque handles, out-parameters, text buffers and callbacks;
- a safe wrapper: `Drop` for C-owned objects, `Result` for status codes, and a closure behind a C function pointer.

The same design scales up: Rust bindings to big C++ libraries such as the Jolt physics engine work exactly this way, with a C interface (JoltC) over the C++ code, and bindgen over the C interface.

## 6.1 Project layout

### Why
The library is our own code, so it lives in the lecture's directory and in git, in `native/`. Cargo knows nothing about C or C++, so a build script compiles it before the Rust code, and generates the bindings at the same time.

### How
- **The layout:**
  - `native/mini.h`: the interface, in C.
  - `native/vec2.c`: vector math, text and status messages, in C.
  - `native/world.cpp`: the physics world, a C++ class behind C functions.
  - `build.rs`: compiles both, links them, and runs bindgen.
  - `src/ffi.rs`: the generated bindings.
  - `src/mini.rs`: the safe Rust interface over them.
  - `src/main.rs`: the program.
- **Build dependencies:** `[build-dependencies]` lists crates that only `build.rs` uses. They're compiled for the build script, and never end up in `lecture_6` itself. These are the first crates we download from crates.io, Rust's package registry; Cargo fetches them, and records their exact versions in `Cargo.lock`.
  - **`cc`** runs the system's C and C++ compilers, and tells Cargo how to link the result.
  - **`bindgen`** parses C headers with libclang, the library at the heart of the clang compiler, and writes Rust declarations for what it finds. It needs libclang installed (`libclang-dev` on Ubuntu).
  - `"1"` and `"0.73"` are version requirements: any version compatible with 1.x, or with 0.73.x.
- **`.cargo/config.toml`** configures Cargo for the whole workspace. Its `[env]` table sets environment variables for every build script: `CC` and `CXX` choose clang and clang++, which `cc` (and CMake, in lecture 8) then use. Without them, `cc` uses the system default, usually gcc.

### Code
From the repo root:
```bash
cargo new lecture_6
```
```bash
mkdir -p .cargo lecture_6/native
```

`.cargo/config.toml`:
```toml
# The C and C++ compilers build scripts use (cc in lectures 6 and 7, CMake for SDL3 in lecture 8).
[env]
CC = "clang"
CXX = "clang++"
```

`lecture_6/Cargo.toml`:
```toml
[package]
name = "lecture_6"
version = "0.1.0"
edition = "2024"

# build.rs runs before the crate compiles. Its dependencies are build-only: neither ends up in lecture_6 itself.
[build-dependencies]
bindgen = "0.73"
cc = "1"
```

## 6.2 The interface: `native/mini.h`

### Why
The header is the contract. The C file, the C++ file and bindgen all read it, so all three agree on every type and function. It has to be plain C: C is the language every other language can call, while C++'s classes, templates, overloads and exceptions have no meaning outside C++.

### How
- **`extern "C"` under C++:** a C++ compiler *mangles* function names, encoding their parameter types into the symbol, so overloads can coexist: `mini_world_step` might become `_Z15mini_world_stepP9MiniWorldf`. `extern "C"` turns that off, so the C++ file defines plain `mini_world_step`, which C and Rust can find. `#ifdef __cplusplus` hides the `extern "C" {` from C compilers, which don't know it.
- **`#define` constants:** the version numbers. bindgen turns simple numeric macros into Rust `const`s.
- **Structs by value:** `MiniVec2` is two floats. C passes and returns it by value, and so will Rust.
- **Arrays and text:** `mini_sum` takes a pointer and a count, C's way of passing an array. `mini_vec2_format` writes text into a buffer the caller owns, `snprintf`-style: it never writes past `capacity`, and returns the length the text needs. Called with a `NULL` buffer and a capacity of 0, it writes nothing, and just measures.
- **Enums:** `MiniStatus` and `MiniShape`. A C enum is an integer, and C code may hold any integer in one, named or not.
- **An opaque type:** `typedef struct MiniWorld MiniWorld;` declares the type without defining it. Callers can hold a `MiniWorld *` and pass it back, but can't see inside or make one themselves. Only `world.cpp` defines it. It's the C version of a class with all-private members.
- **Ownership is documented, not checked:** `mini_world_create` returns a new world, and the comment says every world must be destroyed exactly once. C has no way to say that in the type; the Rust wrapper will.
- **Out-parameters:** `mini_world_add_body` returns a status, and writes the new body's id through `uint32_t *id`. `mini_world_body` copies a body into a caller-owned `MiniBody`.
- **A callback:** `MiniContactFn` is a function pointer type. C++ calls it for every bounce, passing back the `void *user` pointer it was given along with the function. `void *` is "a pointer to anything": the library never looks at it, it only hands it back. That pair, a function and its context pointer, is how C does closures.

### Code
`lecture_6/native/mini.h`:
```c
#pragma once

#include <stddef.h>
#include <stdint.h>

// A C++ compiler must not mangle these names: C, Rust and C++ all call them by their plain C names.
#ifdef __cplusplus
extern "C" {
#endif

// Constants

#define MINI_VERSION_MAJOR 1
#define MINI_VERSION_MINOR 2

// Vectors, written in C (vec2.c)

typedef struct MiniVec2 {
    float x;
    float y;
} MiniVec2;

MiniVec2 mini_vec2_add(MiniVec2 a, MiniVec2 b);
MiniVec2 mini_vec2_scale(MiniVec2 v, float factor);
float mini_vec2_length(MiniVec2 v);

// Adds up `count` floats starting at `values`.
float mini_sum(const float *values, size_t count);

// Writes v as text into `buffer`: at most `capacity` bytes, ending in '\0' if capacity is at least 1. Returns the length the whole text needs, without the '\0', so a call with a NULL buffer and capacity 0 measures the text.
size_t mini_vec2_format(MiniVec2 v, char *buffer, size_t capacity);

// Status codes

typedef enum MiniStatus {
    MINI_OK = 0,
    MINI_INVALID_ARGUMENT = 1,
    MINI_INVALID_BODY = 2,
    MINI_OUT_OF_MEMORY = 3,
} MiniStatus;

// A message for any status, in static memory: never freed.
const char *mini_status_text(MiniStatus status);

// The world, written in C++ (world.cpp)

typedef enum MiniShape {
    MINI_SHAPE_CIRCLE = 0,
    MINI_SHAPE_BOX = 1,
} MiniShape;

typedef struct MiniBodyDesc {
    MiniShape shape;
    MiniVec2 position;
    MiniVec2 velocity;
    float size;       // a circle's radius, or half a box's side
    float bounciness; // 0 stops dead on the ground, 1 bounces back as high as it fell
} MiniBodyDesc;

typedef struct MiniBody {
    MiniVec2 position;
    MiniVec2 velocity;
    uint32_t bounces;
} MiniBody;

// Opaque: declared here, defined only in world.cpp, so callers can hold a MiniWorld * but never look inside.
typedef struct MiniWorld MiniWorld;

// Called for every bounce, with the `user` pointer given to mini_world_set_contact_callback.
typedef void (*MiniContactFn)(void *user, uint32_t body, float speed);

// NULL if out of memory. Every world must be passed to mini_world_destroy exactly once.
MiniWorld *mini_world_create(MiniVec2 gravity, float ground);
void mini_world_destroy(MiniWorld *world);

MiniStatus mini_world_add_body(MiniWorld *world, const MiniBodyDesc *desc, uint32_t *id);
MiniStatus mini_world_body(const MiniWorld *world, uint32_t id, MiniBody *body);
size_t mini_world_body_count(const MiniWorld *world);

// A NULL callback turns contact reports off.
void mini_world_set_contact_callback(MiniWorld *world, MiniContactFn callback, void *user);
void mini_world_step(MiniWorld *world, float dt);

#ifdef __cplusplus
} // extern "C"
#endif
```

## 6.3 The C half: `native/vec2.c`

### Why
Vector math, the array sum, the text formatting and the status messages are plain C, as many real libraries are.

### How
- **Compound literals:** `(MiniVec2){a.x + b.x, a.y + b.y}` builds a struct value in place (C99).
- **`sqrtf`** is the `float` square root from `<math.h>`.
- **`snprintf`** formats into a buffer of at most `capacity` bytes, ending the text with `'\0'` if there's room for one, and returns the length the full text would need, without the `'\0'`. With a capacity of 0 it writes nothing at all, so a null buffer is allowed. Arguments to variadic functions like `snprintf` are promoted, `float` to `double`, so the casts say so explicitly.
- **`mini_status_text`** returns string literals, which live for the whole program, so the caller never frees them.

### Code
`lecture_6/native/vec2.c`:
```c
#include "mini.h"

#include <math.h>
#include <stdio.h>

// Vectors

MiniVec2 mini_vec2_add(MiniVec2 a, MiniVec2 b) {
    return (MiniVec2){a.x + b.x, a.y + b.y};
}

MiniVec2 mini_vec2_scale(MiniVec2 v, float factor) {
    return (MiniVec2){v.x * factor, v.y * factor};
}

float mini_vec2_length(MiniVec2 v) {
    return sqrtf(v.x * v.x + v.y * v.y);
}

// Arrays and text

float mini_sum(const float *values, size_t count) {
    float total = 0.0f;
    for (size_t i = 0; i < count; ++i) {
        total += values[i];
    }
    return total;
}

size_t mini_vec2_format(MiniVec2 v, char *buffer, size_t capacity) {
    // snprintf writes at most capacity bytes, ending in '\0' if there's room for one, and returns the length the whole text needs. Variadic functions take floats as doubles.
    const int needed = snprintf(buffer, capacity, "(%.2f, %.2f)", (double)v.x, (double)v.y);
    return needed < 0 ? 0 : (size_t)needed;
}

// Status codes

const char *mini_status_text(MiniStatus status) {
    switch (status) {
        case MINI_OK:
            return "ok";
        case MINI_INVALID_ARGUMENT:
            return "invalid argument";
        case MINI_INVALID_BODY:
            return "no body with that id";
        case MINI_OUT_OF_MEMORY:
            return "out of memory";
    }
    return "unknown status";
}
```

## 6.4 The C++ half: `native/world.cpp`

### Why
The world is a real C++ class, with a `std::vector` and member functions. The C functions are a thin layer over it: they take the opaque pointer, and call into the class. This is the shape of every C interface over a C++ library.

### How
- **Defining the opaque type:** `struct MiniWorld { ... };` is the definition `mini.h` left out. In C++ it can hold anything, here a `std::vector<Body>`, a nested struct, and the callback.
- **The simulation:** each step adds gravity to every body's velocity, and the velocity to its position. A body that has sunk below the ground while falling is put back on top, and its vertical speed reversed and scaled by its bounciness. A box also loses half its sideways speed at every touch, as if by friction. Bounces slower than `rest_speed` settle the body instead, so a resting ball doesn't report a bounce every frame.
- **The callback:** for every real bounce, `step` calls `on_contact(user, i, speed)`, if one is set.
- **No exceptions across the boundary:** a C++ exception can't pass through C or Rust frames: there's nothing on the other side to catch it, and unwinding through them is undefined behavior. So every C function catches what it can and reports a status instead.
  - **`new (std::nothrow)`** returns `nullptr` instead of throwing `std::bad_alloc` when memory runs out.
  - **`push_back`** may throw `std::bad_alloc`, so `mini_world_add_body` catches it and returns `MINI_OUT_OF_MEMORY`.
- **Checking arguments:** a status code is the C interface's only way to say "no". `!(desc->size > 0.0f)` is written that way round so that NaN, which fails every comparison, is rejected too.
- **`extern "C" { ... }`:** the functions were already declared `extern "C"` in the header, which is enough; the block repeats it, so the C interface is plain to see. Its contents are indented one level.

### Code
`lecture_6/native/world.cpp`:
```cpp
#include "mini.h"

#include <cstddef>
#include <cstdint>
#include <new>
#include <vector>

// The C++ class

// Defined here and nowhere else, so C and Rust only ever see a MiniWorld *. Inside, it's ordinary C++: a class with a std::vector and member functions.
struct MiniWorld {
    struct Body {
        MiniShape shape;
        MiniVec2 position;
        MiniVec2 velocity;
        float size;
        float bounciness;
        std::uint32_t bounces = 0;
    };

    // A bounce slower than this settles the body on the ground instead.
    static constexpr float rest_speed = 0.5f;

    MiniVec2 gravity;
    float ground;
    std::vector<Body> bodies;
    MiniContactFn on_contact = nullptr;
    void *user = nullptr;

    void step(float dt) {
        for (std::size_t i = 0; i < bodies.size(); ++i) {
            Body &body = bodies[i];
            body.velocity = mini_vec2_add(body.velocity, mini_vec2_scale(gravity, dt));
            body.position = mini_vec2_add(body.position, mini_vec2_scale(body.velocity, dt));

            // Above the ground, or rising: nothing more to do.
            if (body.position.y - body.size >= ground || body.velocity.y >= 0.0f) {
                continue;
            }

            // Through the ground while falling: put the body back on top, and bounce.
            const float speed = -body.velocity.y;
            body.position.y = ground + body.size;
            body.velocity.y = speed >= rest_speed ? speed * body.bounciness : 0.0f;

            // Boxes slide, and friction halves their sideways speed at every touch; circles roll on.
            if (body.shape == MINI_SHAPE_BOX) {
                body.velocity.x *= 0.5f;
            }

            if (speed >= rest_speed) {
                ++body.bounces;
                if (on_contact != nullptr) {
                    on_contact(user, static_cast<std::uint32_t>(i), speed);
                }
            }
        }
    }
};

// The C interface

// No C++ exception may escape into C or Rust: there is no way to catch one on the other side. Each function turns failure into NULL or a status code.
extern "C" {

    MiniWorld *mini_world_create(MiniVec2 gravity, float ground) {
        // new (std::nothrow) returns nullptr instead of throwing std::bad_alloc.
        return new (std::nothrow) MiniWorld{gravity, ground, {}, nullptr, nullptr};
    }

    void mini_world_destroy(MiniWorld *world) {
        delete world;
    }

    MiniStatus mini_world_add_body(MiniWorld *world, const MiniBodyDesc *desc, std::uint32_t *id) {
        if (world == nullptr || desc == nullptr || id == nullptr) {
            return MINI_INVALID_ARGUMENT;
        }

        // Written so that NaN, which fails every comparison, is rejected too.
        if (!(desc->size > 0.0f) || !(desc->bounciness >= 0.0f && desc->bounciness <= 1.0f)) {
            return MINI_INVALID_ARGUMENT;
        }

        try {
            world->bodies.push_back({desc->shape, desc->position, desc->velocity, desc->size, desc->bounciness});
        } catch (const std::bad_alloc&) {
            return MINI_OUT_OF_MEMORY;
        }

        *id = static_cast<std::uint32_t>(world->bodies.size() - 1);
        return MINI_OK;
    }

    MiniStatus mini_world_body(const MiniWorld *world, std::uint32_t id, MiniBody *body) {
        if (world == nullptr || body == nullptr) {
            return MINI_INVALID_ARGUMENT;
        }
        if (id >= world->bodies.size()) {
            return MINI_INVALID_BODY;
        }

        const MiniWorld::Body &found = world->bodies[id];
        *body = MiniBody{found.position, found.velocity, found.bounces};
        return MINI_OK;
    }

    std::size_t mini_world_body_count(const MiniWorld *world) {
        return world == nullptr ? 0 : world->bodies.size();
    }

    void mini_world_set_contact_callback(MiniWorld *world, MiniContactFn callback, void *user) {
        if (world != nullptr) {
            world->on_contact = callback;
            world->user = user;
        }
    }

    void mini_world_step(MiniWorld *world, float dt) {
        if (world != nullptr) {
            world->step(dt);
        }
    }

} // extern "C"
```

## 6.5 Building it: `build.rs`

### Why
`cargo build` compiles Rust. To build C and C++ too, and to generate bindings from a header, a crate provides a *build script*: a Rust program in `build.rs`, at the crate's root, that Cargo compiles and runs before compiling the crate itself.

### How
- **Talking to Cargo:** a build script prints instructions on standard output, one per line, starting `cargo::`.
  - **`cargo::rerun-if-changed=native`** reruns the script when something in `native/` changes, instead of whenever any file in the package changes. (`cc` adds similar lines for the environment variables it reads, such as `CC` and `CXXFLAGS`, and a change to `build.rs` itself always reruns it.)
  - **Linking:** `cc` prints the lines that link what it builds, so we never write them ourselves: `cargo:rustc-link-search` for the directory, and `cargo:rustc-link-lib=static=mini_c` for each library. (`cargo:` with one colon is the older spelling, which Cargo still accepts.) The build script's output is saved in `target/debug/build/lecture_6-<hash>/output`.
- **Compiling with `cc`:**
  - **`cc::Build`** collects source files and flags, and `compile("mini_c")` compiles them into the static library `libmini_c.a` in Cargo's build directory, and links it into the crate. `warnings(true)` turns on the compiler's common warnings, which Cargo shows as build warnings.
  - **Two builds:** C and C++ need different compilers and flags, so each language gets its own `Build`. `cpp(true)` compiles as C++, with clang++, and `std("c++20")` picks the standard.
  - **The C++ standard library:** the C++ code uses `std::vector` and `operator new`, which live in the C++ standard library. `cpp(true)` also links it, printing `cargo:rustc-link-lib=stdc++` on Linux. Rust links executables with the C toolchain, which wouldn't add it by itself.
- **Generating bindings with `bindgen`:**
  - **`header`** names the header to parse. bindgen runs libclang on it, exactly as a C compiler would, and turns each declaration into Rust.
  - **`allowlist_item`** keeps only the declarations whose names match the pattern (a regular expression), plus whatever they depend on. Without it, we'd also get everything `<stdint.h>` and `<stddef.h>` declare.
  - **Enums:** bindgen offers several Rust forms for a C enum. `rustified_enum("MiniShape")` makes a real Rust `enum`, which is only safe for enums that Rust *sends* to C: a Rust enum must always hold one of its variants, and C could return any integer. `newtype_enum("MiniStatus")` makes a newtype around the integer, with a constant per name, so any value C returns is valid. `prepend_enum_name(false)` keeps C's names as they are, instead of adding the enum's name in front.
  - **Derives:** `derive_debug` and `derive_copy` add `#[derive(Debug, Copy, Clone)]` to the generated structs. C structs are plain data, so copying one is always fine.
  - **`rust_edition`** makes the output match our edition: in 2024, a block of foreign declarations must be written `unsafe extern`.
  - **`write_to_file`:** the result is written into `OUT_DIR`, a directory Cargo gives each build script for generated files, deep inside `target/`. Generated code never sits among our sources, and is regenerated whenever the header changes.
- **`expect`:** a build script reports failure by panicking; Cargo shows the message and stops the build.

### Code
`lecture_6/build.rs`:
```rust
use std::env;
use std::path::PathBuf;

// Cargo compiles and runs this program before compiling the crate. It talks back to Cargo by printing cargo:: lines.
fn main() {
    // Run this script again only when the library's sources change, not on every build.
    println!("cargo::rerun-if-changed=native");

    // Compile each language into its own static library (libmini_c.a, libmini_cpp.a), and tell Cargo to link both into lecture_6. cpp(true) compiles as C++, and also links the C++ standard library, which the C++ code needs.
    cc::Build::new().file("native/vec2.c").warnings(true).compile("mini_c");
    cc::Build::new().cpp(true).std("c++20").file("native/world.cpp").warnings(true).compile("mini_cpp");

    // bindgen parses the header with libclang, the C parser clang itself uses, and writes a Rust declaration for each C declaration the allowlist lets through.
    let bindings = bindgen::Builder::default()
        .header("native/mini.h")
        .allowlist_item("(mini|Mini|MINI)_?.*")
        // An enum we only pass to C becomes a Rust enum. One that C returns becomes a newtype that accepts any number, since C may return a value the enum doesn't name.
        .rustified_enum("MiniShape")
        .newtype_enum("MiniStatus")
        .prepend_enum_name(false)
        // The generated structs get Debug, Clone and Copy, and the code is written for edition 2024.
        .derive_debug(true)
        .derive_copy(true)
        .rust_edition(bindgen::RustEdition::Edition2024)
        .generate()
        .expect("mini.h parses");

    // OUT_DIR is a directory Cargo gives each build script for generated files.
    let out = PathBuf::from(env::var("OUT_DIR").expect("Cargo sets OUT_DIR"));
    bindings.write_to_file(out.join("mini.rs")).expect("OUT_DIR is writable");
}
```

## 6.6 The generated bindings

### Why
bindgen's output is ordinary Rust, and the best way to understand FFI is to read it. It's not a file we type in: the build writes it. After building, it's in `target/debug/build/lecture_6-<hash>/out/mini.rs`:
```bash
find target -path '*lecture_6*' -name mini.rs
```

### How
Each C declaration becomes one Rust declaration:
- **`#define MINI_VERSION_MAJOR 1`** becomes `pub const MINI_VERSION_MAJOR: u32 = 1;`.
- **`typedef struct MiniVec2 { float x; float y; } MiniVec2;`** becomes a struct with the same fields, marked `#[repr(C)]`, so Rust lays it out exactly as C does (lecture 2). C's `float` is Rust's `f32`.
- **Layout checks:** after each struct comes a `const _: () = { ... };` block that checks, while compiling, that the Rust struct has the size, alignment and field offsets libclang computed for the C one. Each line indexes a one-element array with the difference between the two values, which is `0` when they agree; any other difference either overflows the subtraction or indexes out of bounds, and the build fails at that line, which names the check. If the header and the Rust side ever disagree, the build stops instead of corrupting memory.
- **Functions** become declarations inside `unsafe extern "C" { ... }`. `extern "C"` says they use C's calling convention, and have no Rust body: the linker finds them in our C libraries. Calling any of them is `unsafe`, because the compiler can't check what C does with its arguments.
- **Types:** `uint32_t` becomes `u32` and `size_t` becomes `usize`. `char` becomes `c_char`, an alias (lecture 4) for whatever type C's `char` is on the target platform. `void *` becomes `*mut c_void`: `c_void` is a type of its own, which stands for C's `void` behind a pointer.
- **Pointers:** `const MiniBodyDesc *` becomes `*const MiniBodyDesc`, and `uint32_t *` becomes `*mut u32`. A `const` on the pointed-to type picks `*const`; anything else is `*mut`.
- **`MiniStatus`** is a `#[repr(transparent)]` newtype around `c_uint` (`#[repr(transparent)]` guarantees it's passed exactly like the integer inside), with an associated constant per name, such as `MiniStatus::MINI_OK`.
- **`MiniShape`** is a `#[repr(u32)]` Rust enum, the same size as C's.
- **The opaque `MiniWorld`** becomes a struct with one zero-sized private field, `_unused: [u8; 0]`. Rust code can't build one or look inside, but `*mut MiniWorld` is a distinct pointer type, so a world can't be confused with any other pointer.
- **`MiniContactFn`** becomes `Option<unsafe extern "C" fn(...)>`. A Rust function pointer is never null, and C's may be, so `None` stands for `NULL`, at no extra cost (lecture 2's `Option<Box<T>>` trick again).

`target/debug/build/lecture_6-<hash>/out/mini.rs`, generated:
```rust
/* automatically generated by rust-bindgen 0.73.2 */

pub const MINI_VERSION_MAJOR: u32 = 1;
pub const MINI_VERSION_MINOR: u32 = 2;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MiniVec2 {
    pub x: f32,
    pub y: f32,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of MiniVec2"][::std::mem::size_of::<MiniVec2>() - 8usize];
    ["Alignment of MiniVec2"][::std::mem::align_of::<MiniVec2>() - 4usize];
    ["Offset of field: MiniVec2::x"][::std::mem::offset_of!(MiniVec2, x) - 0usize];
    ["Offset of field: MiniVec2::y"][::std::mem::offset_of!(MiniVec2, y) - 4usize];
};
unsafe extern "C" {
    pub fn mini_vec2_add(a: MiniVec2, b: MiniVec2) -> MiniVec2;
}
unsafe extern "C" {
    pub fn mini_vec2_scale(v: MiniVec2, factor: f32) -> MiniVec2;
}
unsafe extern "C" {
    pub fn mini_vec2_length(v: MiniVec2) -> f32;
}
unsafe extern "C" {
    pub fn mini_sum(values: *const f32, count: usize) -> f32;
}
unsafe extern "C" {
    pub fn mini_vec2_format(v: MiniVec2, buffer: *mut ::std::os::raw::c_char, capacity: usize) -> usize;
}
impl MiniStatus {
    pub const MINI_OK: MiniStatus = MiniStatus(0);
    pub const MINI_INVALID_ARGUMENT: MiniStatus = MiniStatus(1);
    pub const MINI_INVALID_BODY: MiniStatus = MiniStatus(2);
    pub const MINI_OUT_OF_MEMORY: MiniStatus = MiniStatus(3);
}
#[repr(transparent)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct MiniStatus(pub ::std::os::raw::c_uint);
unsafe extern "C" {
    pub fn mini_status_text(status: MiniStatus) -> *const ::std::os::raw::c_char;
}
#[repr(u32)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum MiniShape {
    MINI_SHAPE_CIRCLE = 0,
    MINI_SHAPE_BOX = 1,
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MiniBodyDesc {
    pub shape: MiniShape,
    pub position: MiniVec2,
    pub velocity: MiniVec2,
    pub size: f32,
    pub bounciness: f32,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of MiniBodyDesc"][::std::mem::size_of::<MiniBodyDesc>() - 28usize];
    ["Alignment of MiniBodyDesc"][::std::mem::align_of::<MiniBodyDesc>() - 4usize];
    ["Offset of field: MiniBodyDesc::shape"][::std::mem::offset_of!(MiniBodyDesc, shape) - 0usize];
    ["Offset of field: MiniBodyDesc::position"][::std::mem::offset_of!(MiniBodyDesc, position) - 4usize];
    ["Offset of field: MiniBodyDesc::velocity"][::std::mem::offset_of!(MiniBodyDesc, velocity) - 12usize];
    ["Offset of field: MiniBodyDesc::size"][::std::mem::offset_of!(MiniBodyDesc, size) - 20usize];
    ["Offset of field: MiniBodyDesc::bounciness"][::std::mem::offset_of!(MiniBodyDesc, bounciness) - 24usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MiniBody {
    pub position: MiniVec2,
    pub velocity: MiniVec2,
    pub bounces: u32,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of MiniBody"][::std::mem::size_of::<MiniBody>() - 20usize];
    ["Alignment of MiniBody"][::std::mem::align_of::<MiniBody>() - 4usize];
    ["Offset of field: MiniBody::position"][::std::mem::offset_of!(MiniBody, position) - 0usize];
    ["Offset of field: MiniBody::velocity"][::std::mem::offset_of!(MiniBody, velocity) - 8usize];
    ["Offset of field: MiniBody::bounces"][::std::mem::offset_of!(MiniBody, bounces) - 16usize];
};
#[repr(C)]
#[derive(Debug)]
pub struct MiniWorld {
    _unused: [u8; 0],
}
pub type MiniContactFn = ::std::option::Option<unsafe extern "C" fn(user: *mut ::std::os::raw::c_void, body: u32, speed: f32)>;
unsafe extern "C" {
    pub fn mini_world_create(gravity: MiniVec2, ground: f32) -> *mut MiniWorld;
}
unsafe extern "C" {
    pub fn mini_world_destroy(world: *mut MiniWorld);
}
unsafe extern "C" {
    pub fn mini_world_add_body(world: *mut MiniWorld, desc: *const MiniBodyDesc, id: *mut u32) -> MiniStatus;
}
unsafe extern "C" {
    pub fn mini_world_body(world: *const MiniWorld, id: u32, body: *mut MiniBody) -> MiniStatus;
}
unsafe extern "C" {
    pub fn mini_world_body_count(world: *const MiniWorld) -> usize;
}
unsafe extern "C" {
    pub fn mini_world_set_contact_callback(world: *mut MiniWorld, callback: MiniContactFn, user: *mut ::std::os::raw::c_void);
}
unsafe extern "C" {
    pub fn mini_world_step(world: *mut MiniWorld, dt: f32);
}
```

## 6.7 The raw module: `src/ffi.rs`

### Why
The generated file needs a home in the crate. Keeping it in its own module means the rest of the program refers to it as `ffi::`, so every call across the boundary stands out.

### How
- **`include!`** pastes another file's contents in at compile time. `env!("OUT_DIR")` reads the environment variable Cargo sets while compiling a crate that has a build script, and `concat!` joins it with the file name, all at compile time.
- **`#![allow(...)]`** with `!` applies to the enclosing item, here the whole module. The C names break Rust's naming conventions (`MINI_SHAPE_BOX` would be `MiniShapeBox`), and not every binding is used, so these two lints would only be noise here. They stay on for the rest of the crate.

### Code
`lecture_6/src/ffi.rs`:
```rust
// The raw C interface, exactly as bindgen wrote it. Every function is unsafe to call: the compiler can't check what C does with our pointers.

// The names are C's (MINI_SHAPE_BOX, not MiniShapeBox), and we don't use every binding, so these lints would only be noise here.
#![allow(non_camel_case_types, dead_code)]

// build.rs wrote the bindings into OUT_DIR, and include! pastes that file in here at compile time.
include!(concat!(env!("OUT_DIR"), "/mini.rs"));
```

## 6.8 The safe wrapper: `src/mini.rs`

### Why
The bindings are complete but dangerous: every call is `unsafe`, nothing frees a world, and every status must be checked by hand. The wrapper is where each of C's documented rules becomes something Rust checks: ownership becomes `Drop`, status codes become `Result`, and pointers become references and slices. Code outside this file never writes `unsafe`.

### How
- **Methods on generated types:** the bindings are in our crate, so we can add methods and trait impls to their types like any of our own: `MiniVec2::new`, `length`, `Add` (so `a + b` calls `mini_vec2_add`), and `Display`.
- **`unsafe` blocks:** each says, in its `// SAFETY:` comment, why the call is valid. Plain values in and a plain value out, as for `mini_vec2_length`, leave nothing for C to get wrong.
- **Text from C, in two calls:**
  - **Measure:** `Display` first calls `mini_vec2_format` with a null pointer (`ptr::null_mut()`) and a capacity of 0. C writes nothing, and returns the length the text needs.
  - **Write:** a `Vec<u8>` of that length plus one, for the `'\0'`, then holds the text. `as_mut_ptr().cast()` turns its pointer into the `*mut c_char` C wants. However long the numbers get, the text is never cut short.
  - **`CStr::from_bytes_until_nul`** finds the `'\0'` and views the text before it as a `&CStr`, C's kind of string. `to_string_lossy()` converts it to Rust text, replacing any invalid UTF-8.
- **Slices to C:** `sum(values: &[f32])` passes `values.as_ptr()` and `values.len()`: a slice is exactly the pointer and length C wants. Rust code calls `mini::sum(&[1.5, 2.5, 3.0])`, and can't get the length wrong.
- **Status codes to `Result`:**
  - **`MiniError`** wraps a `MiniStatus`. Its `Display` asks C for the message: `CStr::from_ptr` reads a C string from a pointer, up to its `'\0'`, which is `unsafe` because only the caller can promise the pointer is valid.
  - **`check`** turns `MINI_OK` into `Ok(())` and anything else into an `Err`, so every call can end with `?`.
- **Owning a C++ object:**
  - **The handle:** `World` holds a `NonNull<ffi::MiniWorld>`: the opaque pointer, never null. `World::new` checks for C's `NULL` once, and returns an error instead.
  - **`Drop`** calls `mini_world_destroy`, so the C++ destructor runs exactly once, when the `World` is dropped. Rust's ownership rules now enforce the comment in `mini.h`.
  - **No `Clone`:** two `World`s holding one pointer would destroy it twice.
  - **Threads:** a raw pointer is neither `Send` nor `Sync` (lecture 5), and so neither is `World`, so it can't be shared between threads. That's the safe default: the C++ class wasn't written for that.
- **Out-parameters:** `add_body` passes `&mut id`, and a `&mut u32` converts to the `*mut u32` C wants. `desc: &MiniBodyDesc` converts to `*const MiniBodyDesc` the same way. `body` starts from a zeroed `MiniBody` for C to fill in. (`MaybeUninit` could skip the zeroing, at the cost of more `unsafe`.)
- **Closures to C, through a trampoline:**
  - **The problem:** C can only call a plain function pointer, and a Rust closure is a function *and* its captured data. C's answer is the `void *user` pointer, handed back on every call.
  - **The generic trampoline:** `contact_trampoline::<F>` is an `extern "C"` function, so it has C's calling convention and fits `MiniContactFn`. It's generic over the closure's type, `F`, so the compiler stamps out one for each closure type, and each one knows exactly what kind of closure its `user` points at: it casts `user` back to `*mut F`, and calls the closure. No allocation, and no `dyn`.
  - **`step`** points `user` at its own `on_contact` parameter, registers the trampoline, steps the world, and clears the callback again before returning. C only holds the pointer while `on_contact` is alive, so the closure may borrow anything, as `main`'s does.
  - **`let user: *mut F = &mut on_contact;`** makes the raw pointer from a mutable reference; `user.cast()` turns it into the `*mut c_void` C wants.
  - **Panics:** unwinding out of an `extern "C"` function is not allowed, so if the closure panics, Rust aborts the whole program instead of letting the panic run into C++.

### Code
`lecture_6/src/mini.rs`:
```rust
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
```

## 6.9 The program: `src/main.rs`

### Why
`main` exercises each kind of binding through the safe wrapper: constants, structs by value, slices, text, layout, enums, errors, an owned C++ object and a callback.

### How
- **The C side:** the version macro, vector math through `Add` and `length`, `Display` through `snprintf`, and a slice through `mini_sum`. The sizes printed are C's, as bindgen's layout checks guarantee.
- **The bodies:** `MiniBodyDesc` is a generated struct, built with a struct literal like any Rust struct. Its `shape` field takes the Rust enum `MiniShape`.
- **`crate_`:** `crate` is a keyword, so the variable gets a trailing underscore.
- **Errors:** a body of size `-1` comes back as an `Err`, and `{error}` prints C's own message for its status. So does asking for body 7.
- **The callback:** each of the 180 frames passes a new closure, `|body, speed| contacts.push((frame, body, speed))`, which borrows `contacts` mutably and reads `frame`. C++ calls it for each bounce, from inside `mini_world_step`: 4 times in all. When the loop ends, `contacts` is ours again.
- **Positions** print through `MiniVec2`'s `Display`, which calls back into C. `to_string()` comes free with `Display`, and makes the first position a `String`, so `{:<16}` can pad it: our `Display` writes its text with `write_str`, which ignores widths.
- **`frame as f32 * dt`:** `frame` is a loop counter, an `i32`, so it's converted for the time.
- **The end of `demo_world`** drops the `World`, which destroys the C++ object.

### Code
`lecture_6/src/main.rs`:
```rust
mod ffi;
mod mini;

use ffi::{MiniBody, MiniBodyDesc, MiniShape, MiniVec2};
use mini::World;

use std::error::Error;
use std::mem::{align_of, size_of};

// The C side

fn demo_c() {
    // #define constants become Rust consts.
    println!("mini {}.{}", ffi::MINI_VERSION_MAJOR, ffi::MINI_VERSION_MINOR);

    // Structs pass to and from C by value, and print with the Debug bindgen derived.
    let a = MiniVec2::new(3.0, 4.0);
    let b = MiniVec2::new(0.5, -1.0);
    println!("{a:?} has length {}", a.length());
    println!("{a} + {b} = {}", a + b);

    // An array becomes a pointer and a length on its way to C.
    println!("sum of [1.5, 2.5, 3.0] = {}", mini::sum(&[1.5, 2.5, 3.0]));

    // #[repr(C)] gives the Rust structs C's layout, and bindgen's generated asserts check it while compiling.
    println!("size_of: MiniVec2 {}, MiniBodyDesc {}, MiniBody {}", size_of::<MiniVec2>(), size_of::<MiniBodyDesc>(), size_of::<MiniBody>());
    println!("MiniShape: {} bytes, aligned to {}, like a C enum", size_of::<MiniShape>(), align_of::<MiniShape>());
}

// The C++ side

fn demo_world() -> Result<(), Box<dyn Error>> {
    let mut world = World::new(MiniVec2::new(0.0, -9.81), 0.0)?;

    let ball_desc = MiniBodyDesc {
        shape: MiniShape::MINI_SHAPE_CIRCLE,
        position: MiniVec2::new(0.0, 5.0),
        velocity: MiniVec2::new(1.0, 0.0),
        size: 0.5,
        bounciness: 0.7,
    };
    let crate_desc = MiniBodyDesc {
        shape: MiniShape::MINI_SHAPE_BOX,
        position: MiniVec2::new(3.0, 2.0),
        velocity: MiniVec2::new(2.0, 3.0),
        size: 0.25,
        bounciness: 0.2,
    };

    // crate is a keyword, so the variable is crate_.
    let ball = world.add_body(&ball_desc)?;
    let crate_ = world.add_body(&crate_desc)?;
    println!("{} bodies: ball {ball}, crate {crate_}", world.body_count());

    // Bad input comes back as an error, with the library's own message.
    if let Err(error) = world.add_body(&MiniBodyDesc { size: -1.0, ..crate_desc }) {
        println!("a body of size -1: {error}");
    }
    if let Err(error) = world.body(7) {
        println!("body 7: {error}");
    }

    // The closure borrows contacts mutably while C++ calls it, through the trampoline.
    let mut contacts = Vec::new();
    println!("{:>5}  {:<16} crate", "time", "ball");

    let dt = 1.0 / 60.0;
    for frame in 1..=180 {
        world.step(dt, |body, speed| contacts.push((frame, body, speed)));

        if frame % 30 == 0 {
            let time = frame as f32 * dt;
            println!("{time:>4.1}s  {:<16} {}", world.body(ball)?.position.to_string(), world.body(crate_)?.position);
        }
    }

    println!("{} bounces:", contacts.len());
    for (frame, body, speed) in &contacts {
        let name = if *body == ball { "ball" } else { "crate" };
        println!("    frame {frame:>3}: {name:<5} hit the ground at {speed:.2} m/s");
    }

    println!("bounces counted by C++: ball {}, crate {}", world.body(ball)?.bounces, world.body(crate_)?.bounces);
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    demo_c();
    println!();
    demo_world()
}
```

## 6.10 Build and run

```bash
cargo run -p lecture_6
```

```text
mini 1.2
MiniVec2 { x: 3.0, y: 4.0 } has length 5
(3.00, 4.00) + (0.50, -1.00) = (3.50, 3.00)
sum of [1.5, 2.5, 3.0] = 7
size_of: MiniVec2 8, MiniBodyDesc 28, MiniBody 20
MiniShape: 4 bytes, aligned to 4, like a C enum

2 bodies: ball 0, crate 1
a body of size -1: invalid argument (status 1)
body 7: no body with that id (status 2)
 time  ball             crate
 0.5s  (0.50, 3.73)     (4.00, 2.23)
 1.0s  (1.00, 0.81)     (4.97, 0.29)
 1.5s  (1.50, 2.56)     (5.21, 0.25)
 2.0s  (2.00, 1.86)     (5.21, 0.25)
 2.5s  (2.50, 1.26)     (5.21, 0.25)
 3.0s  (3.00, 1.08)     (5.21, 0.25)
4 bounces:
    frame  57: ball  hit the ground at 9.32 m/s
    frame  58: crate hit the ground at 6.48 m/s
    frame  73: crate hit the ground at 1.16 m/s
    frame 136: ball  hit the ground at 6.39 m/s
bounces counted by C++: ball 2, crate 2
```

Reading the output:
- **The C side:** `MiniVec2 { x: 3.0, y: 4.0 }` is bindgen's derived `Debug`, and `(3.00, 4.00)` is C's `snprintf`. `MiniBodyDesc` is 28 bytes: a 4-byte enum and six floats, with no padding.
- **Errors** carry C's own messages and status numbers.
- **The world:** the ball (bounciness 0.7) hits the ground at frame 57 at 9.32 m/s, and again at frame 136, more slowly. The crate starts lower, but thrown upward at 3 m/s, so it lands only a frame after the ball; with bounciness 0.2 it barely leaves the ground again, and friction stops it sliding at x = 5.21.
- **Bounces:** the callback saw 4, and C++'s own counters agree: 2 each.

In lecture 7 we call a library we didn't write: LodePNG, a PNG encoder and decoder in C. Its memory is allocated by C and must be freed by C, which is the next thing the wrapper has to handle.
