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
