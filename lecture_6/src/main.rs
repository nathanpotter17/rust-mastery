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
