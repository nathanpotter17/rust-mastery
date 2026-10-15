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
