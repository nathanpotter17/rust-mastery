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
