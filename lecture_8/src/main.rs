mod device;
mod memory;
mod renderer;
mod scene;

use device::{DeviceContext, WINDOW_NAME};
use memory::{ENABLE_VALIDATION, HEIGHT, MemoryContext, WIDTH};
use renderer::Renderer;

use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Keycode;
use std::error::Error;
use std::time::Instant;

// Every fallible function in this program returns this: any error, boxed.
pub type AppResult<T> = Result<T, Box<dyn Error>>;

// Builds the swapchain again at the window's current pixel size, and everything that depends on it.
fn recreate(window: &sdl3::video::Window, device_context: &mut DeviceContext, renderer: &mut Renderer) -> AppResult<()> {
    // A zero-sized window can't have a swapchain; drawing resumes when it has a size again.
    let (width, height) = window.size_in_pixels();
    if width == 0 || height == 0 {
        return Ok(());
    }

    device_context.recreate_swapchain(width, height)?;
    renderer.recreate_framebuffers(device_context)
}

fn main() -> AppResult<()> {
    println!("\n╔════════════════════════════════════╗");
    println!("║        VULKAN RENDERER             ║");
    println!("║   VMA-Style Memory Architecture    ║");
    println!("╚════════════════════════════════════╝");

    // SDL3: a resizable window that Vulkan can draw to.
    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let window = video.window(WINDOW_NAME, WIDTH, HEIGHT).position_centered().resizable().vulkan().build()?;

    let mut device_context = DeviceContext::new(&window, ENABLE_VALIDATION)?;
    println!("✓ Device initialized (UBO alignment: {})", device_context.min_ubo_alignment);

    // The memory system: pool allocator, ring buffer and staging buffer.
    let memory = MemoryContext::new(device_context.device.clone(), device_context.memory_properties, device_context.min_ubo_alignment)?;
    println!("✓ Memory system initialized");

    // The renderer takes ownership of the memory context.
    let mut renderer = Renderer::new(&device_context, memory)?;
    println!("✓ Renderer initialized");

    let mut event_pump = sdl.event_pump()?;
    let mut minimized = false;
    let mut last_frame = Instant::now();
    println!("\nStarting render loop...\n");

    'running: loop {
        // A minimized window has nothing to draw to: sleep until an event arrives, instead of spinning. An Option chains in as 0 or 1 events.
        let waited = if minimized { Some(event_pump.wait_event()) } else { None };

        for event in waited.into_iter().chain(event_pump.poll_iter()) {
            match event {
                Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'running,

                // SDL3 reports the window's size in points in Resized; the swapchain needs pixels.
                Event::Window { win_event: WindowEvent::PixelSizeChanged(width, height), .. } => {
                    println!("Window resized: {width}x{height}");
                    recreate(&window, &mut device_context, &mut renderer)?;
                }

                Event::Window { win_event: WindowEvent::Minimized, .. } => minimized = true,
                Event::Window { win_event: WindowEvent::Restored | WindowEvent::Maximized, .. } => minimized = false,

                _ => {}
            }
        }

        // While minimized, keep the clock current, so the cube doesn't jump when the window comes back.
        if minimized {
            last_frame = Instant::now();
            continue;
        }

        // The cube turns by real time, so it spins at the same speed at any refresh rate.
        let now = Instant::now();
        let delta_time = now.duration_since(last_frame).as_secs_f32();
        last_frame = now;

        // true: the swapchain went stale (out of date or suboptimal), so build it again before the next frame.
        if renderer.render(&device_context, delta_time)? {
            recreate(&window, &mut device_context, &mut renderer)?;
        }
    }

    // Locals drop in reverse order: renderer (and the MemoryContext inside it), then device_context, then the window and SDL.
    println!("\nShutting down...");
    Ok(())
}
