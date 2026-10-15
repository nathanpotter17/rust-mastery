# Rust Mastery

## Goals

- Learn Rust from the ground up, one self-contained project per lecture.
- Understand ownership, borrowing and lifetimes well enough to write `unsafe` code that upholds them.
- Call C and C++ from Rust: build native code with Cargo, generate bindings, and wrap them safely.
- Finish with a Vulkan renderer: a spinning cube, drawn with `ash` and SDL3.

## Lectures

Each lecture lives in `lecture_N/` and has a write-up in `lecture-md/lecture_N/lecture_N.md`, which builds the whole project from nothing.

1. **Fundamentals:** Cargo and tests, types and conversions, enums with data, pattern matching, expressions, structs and modules (a vending machine).
2. **Memory and ownership:** scopes, moves, borrows, `Drop`, `Box`, `Option`, `Rc`, layout and alignment, raw pointers and `unsafe` (one demo per idea).
3. **Contiguous data:** arrays, slices, `Vec`, `String` and `&str`, iterators (Conway's Game of Life).
4. **Type aliases and the standard library:** aliases, newtypes, associated types, `Option`, closures, iterator chains (a batch calculator).
5. **More of the standard library:** `Result` and error types, std traits, collections, files, time and scoped threads (a build planner).
6. **Our own C and C++ library:** compiling native code with `cc`, generating bindings with `bindgen`, and a safe wrapper (a tiny physics world).
7. **A third-party C library:** vendoring LodePNG, C-owned memory, error codes as `Result` (a PNG round trip).
8. **Vulkan:** instance, device, swapchain, a pool allocator, a pipeline and a frame loop (vkcube).

## Requirements

- Linux (Ubuntu, Wayland session). Lectures 1–5 also build anywhere Rust does.
- Rust 1.88 or newer (edition 2024), through rustup, with `clippy` and `rustfmt`.
- Lectures 6 and 7: `clang` and `libclang` (bindgen parses headers with libclang; `.cargo/config.toml` also makes the `cc` crate compile with clang).
- Lecture 8:
    - A Vulkan loader and a GPU driver with Vulkan 1.3, and the validation layers for debug builds (`vulkan-validationlayers` on Ubuntu).
    - `glslc` (from the Vulkan SDK, or Ubuntu's `glslc` package) to compile the shaders.
    - CMake, and SDL3's Wayland build dependencies: `pkg-config`, `libwayland-dev`, `libxkbcommon-dev` and `libdecor-0-dev`.

## Build System

The repo is one Cargo workspace with a crate per lecture, sharing one `Cargo.lock` and one `target/`.

Run `./build.bash`, choose a lecture, then a configuration: 1–3 build (Debug, Release, or Clippy's lints), 4–5 build and run (Enter picks 4, Debug). "All lectures" offers the builds only.

Without the script: `cargo run -p lecture_3` runs a lecture, and `cargo clippy --workspace` lints them all.
