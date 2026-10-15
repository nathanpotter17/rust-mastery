use std::fs;
use std::mem::{align_of, offset_of, size_of};
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

// A type that says when it's dropped

// Drop runs automatically when a value's owner goes away; we never call it ourselves. This one prints its name, so the output shows exactly when that happens.
#[derive(Clone)]
struct Tracer {
    name: char,
}

impl Drop for Tracer {
    fn drop(&mut self) {
        println!("    [{}] dropped", self.name);
    }
}

// Scopes

fn demo_scopes() {
    println!("Scopes:");
    // A leading _ marks a variable we never read again.
    let _a = Tracer { name: 'a' };
    {
        let _b = Tracer { name: 'b' };
        println!("    end of the inner scope");
    }
    println!("    end of the outer scope");
}

// Moves and copies

// Takes ownership: the Tracer is dropped when this function returns.
fn take(tracer: Tracer) {
    println!("    took {}", tracer.name);
}

fn demo_moves() {
    println!("Moves and copies:");
    let m = Tracer { name: 'm' };

    // A move: the value now belongs to `moved`, and m can't be used again.
    let moved = m;
    take(moved);
    println!("    back from take");

    // clone() makes an explicit copy, so the function gets its own Tracer and we keep ours.
    let c = Tracer { name: 'c' };
    take(c.clone());
    println!("    c is still ours: {}", c.name);

    // An i32 is Copy: assigning copies it, and both stay usable.
    let x = 5;
    let y = x;
    println!("    x = {x}, y = {y}");
}

// Borrowing

// &Tracer: borrows the caller's Tracer to read it.
fn show(tracer: &Tracer) {
    println!("    borrowed {}", tracer.name);
}

// &mut Tracer: borrows it to change it.
fn rename(tracer: &mut Tracer, name: char) {
    tracer.name = name;
}

fn demo_borrows() {
    println!("Borrowing:");
    let mut t = Tracer { name: 't' };
    show(&t);
    rename(&mut t, 'u');
    show(&t);

    // Any number of shared borrows at once, or one mutable borrow, never both.
    let first = &t;
    let second = &t;
    println!("    two readers: {} and {}", first.name, second.name);
}

// The heap

fn demo_box() {
    println!("Box:");
    // Box::new puts the Tracer on the heap. The Box, a pointer, owns it.
    let boxed = Box::new(Tracer { name: 'h' });

    // Moving a Box moves the pointer; the Tracer stays where it is.
    let owner = boxed;
    println!("    owner holds {}; a Box is {} bytes, and so is an Option<Box>", owner.name, size_of::<Box<Tracer>>());
    assert_eq!(size_of::<Option<Box<Tracer>>>(), size_of::<Box<Tracer>>());

    // drop() takes ownership and ends the value's life early.
    drop(owner);
    println!("    after drop(owner)");
}

fn demo_rc() {
    println!("Rc and Weak:");
    // Rc: several owners of one value, counted. Rc::clone adds an owner; it copies a pointer, not the Tracer.
    let shared = Rc::new(Tracer { name: 's' });
    let another = Rc::clone(&shared);

    // A Weak watches without owning. upgrade() reaches the value only while an owner is left.
    let observer = Rc::downgrade(&shared);
    println!("    owners: {}", Rc::strong_count(&shared));

    drop(shared);
    println!("    owners: {}, observer can reach it: {}", Rc::strong_count(&another), observer.upgrade().is_some());

    drop(another);
    println!("    observer can reach it: {}", observer.upgrade().is_some());
}

// Resources

// A file that is deleted when its owner goes away. Drop ties any resource (memory, a file, a lock, a GPU object) to an owner.
struct TempFile {
    path: PathBuf,
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Nothing useful can be done about a failure here, so the result is ignored on purpose.
        let _ = fs::remove_file(&self.path);
    }
}

fn demo_resource() {
    println!("A file deleted by Drop:");
    let path = std::env::temp_dir().join("lecture_2.txt");
    {
        // expect() stops the program with this message if writing fails.
        fs::write(&path, "hello").expect("the temp directory is writable");
        let _file = TempFile { path: path.clone() };
        println!("    inside the scope, exists: {}", path.exists());
    }
    println!("    after the scope, exists: {}", path.exists());
}

// Layout

// #[repr(C)] keeps the fields in declaration order, as C does. Padding keeps each field aligned: 7 bytes after tag, and 7 after flag.
#[repr(C)]
struct Loose {
    tag: u8,
    value: f64,
    flag: u8,
}

// The same fields, largest first: the two u8s share one padded tail.
#[repr(C)]
struct Tight {
    value: f64,
    tag: u8,
    flag: u8,
}

// Without #[repr(C)], Rust may reorder the fields, and does.
struct Reordered {
    tag: u8,
    value: f64,
    flag: u8,
}

// align(16): every Vec4 starts at a multiple of 16, as SIMD instructions want for four floats.
#[repr(C, align(16))]
#[derive(Default)]
struct Vec4 {
    x: f32,
    y: f32,
    z: f32,
    w: f32,
}

fn demo_layout() {
    println!("Layout:");
    println!(
        "    Loose      size {:>2}, align {:>2}, offsets tag {}, value {}, flag {}",
        size_of::<Loose>(),
        align_of::<Loose>(),
        offset_of!(Loose, tag),
        offset_of!(Loose, value),
        offset_of!(Loose, flag)
    );
    println!(
        "    Tight      size {:>2}, align {:>2}, offsets tag {}, value {}, flag {}",
        size_of::<Tight>(),
        align_of::<Tight>(),
        offset_of!(Tight, tag),
        offset_of!(Tight, value),
        offset_of!(Tight, flag)
    );
    println!(
        "    Reordered  size {:>2}, align {:>2}, offsets tag {}, value {}, flag {}",
        size_of::<Reordered>(),
        align_of::<Reordered>(),
        offset_of!(Reordered, tag),
        offset_of!(Reordered, value),
        offset_of!(Reordered, flag)
    );
    println!("    Vec4       size {:>2}, align {:>2}", size_of::<Vec4>(), align_of::<Vec4>());
}

// Raw pointers

fn demo_raw_pointers() {
    println!("Raw pointers:");
    let on_stack = Vec4::default();
    let on_heap = Box::new(Vec4 { w: 1.0, ..Vec4::default() });

    // A raw pointer is a plain address: the compiler checks nothing about it. Making one is safe.
    let stack_pointer: *const Vec4 = &on_stack;
    let heap_pointer: *const Vec4 = &*on_heap;
    println!(
        "    16-aligned: on the stack {}, on the heap {}",
        stack_pointer.addr().is_multiple_of(16),
        heap_pointer.addr().is_multiple_of(16)
    );

    // Reading through a raw pointer is unsafe: we promise it points at a live value.
    // SAFETY: heap_pointer points into on_heap, which is alive and unchanged.
    let w = unsafe { (*heap_pointer).w };
    println!("    w through the pointer: {w}, x on the stack: {}", on_stack.x);

    // debug_assert! checks for bugs in our own code, in debug builds only.
    debug_assert!(heap_pointer.addr().is_multiple_of(align_of::<Vec4>()), "a Box must respect align(16)");
}

// The program

fn main() -> ExitCode {
    // cfg!(debug_assertions) reports whether debug assertions are on: true in debug builds, false in release builds.
    if cfg!(debug_assertions) {
        println!("Debug build: debug_assert!() is checked\n");
    } else {
        println!("Release build: debug_assert!() is skipped\n");
    }

    demo_scopes();
    demo_moves();
    demo_borrows();
    demo_box();
    demo_rc();
    demo_resource();
    demo_layout();
    demo_raw_pointers();

    ExitCode::SUCCESS
}
