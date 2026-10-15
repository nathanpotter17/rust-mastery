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
