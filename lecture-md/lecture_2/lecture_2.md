# Lecture 2: Memory, ownership and borrowing

Every value in a Rust program lives somewhere in memory, for some stretch of time, and something has to end that time and give the memory back. In C that's the programmer's job (`malloc` and `free`); in C++ it's mostly destructors. Rust answers with one rule, checked by the compiler: every value has exactly one *owner*, and when the owner goes away, so does the value.

This lecture shows that rule at work, one small demo at a time, all in one file. A tiny type prints a line when it's dropped, so the program's output is a trace of when each value's life ends. We cover:
- scopes and drop order;
- moves, `Clone` and `Copy`;
- borrowing with `&` and `&mut`, and the borrow rules;
- the heap: `Box`, `Option`, `Rc` and `Weak`;
- `Drop` for any resource (RAII);
- memory layout: `size_of`, `align_of`, padding, `#[repr(C)]` and `align(16)`;
- raw pointers, `unsafe`, and `debug_assert!`.

## 2.1 The project

### Why
Each demo is a short function, and `main` calls them in order, so the whole lecture is one file: `lecture_2/src/main.rs`. Each section below explains the next part of it; typed in order, the parts make up the complete file.

### How
- **The crate:** `cargo new lecture_2`, as in lecture 1. The workspace picks it up through its `lecture_*` glob.
- **Running it:** `cargo run -p lecture_2`. Each demo's output is labeled with the demo's name.

### Code
From the repo root:
```bash
cargo new lecture_2
```

`lecture_2/Cargo.toml`:
```toml
[package]
name = "lecture_2"
version = "0.1.0"
edition = "2024"
```

## 2.2 Drop, and scopes

### Why
A value's life ends silently: nothing tells us when. To *see* it, we give a type a `Drop` implementation that prints a line. Every later demo uses it.

### How
- **`impl Drop for Tracer`:** `Drop` is the standard trait with one method, `drop(&mut self)`, which Rust calls automatically when the value's owner goes away. We never call it ourselves; the compiler inserts the call. It's Rust's destructor.
- **`#[derive(Clone)]`** gives `Tracer` a `clone()` method that copies it field by field (2.3).
- **Scopes:** a variable lives until the end of the block it's declared in, the closing `}`. Then its value is dropped. Values in the same block are dropped in the reverse order of their declaration, newest first.
- **`let _a`:** a leading `_` names a variable we don't read again, without a warning. (A bare `let _ = ...` is different: it binds nothing, so a newly created value is dropped at once.)
- **`use std::{...}`:** the imports for the whole file, at its top.

### Code
`lecture_2/src/main.rs`, part 1:
```rust
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
```

## 2.3 Moves and copies

### Why
Assigning a value, or passing it to a function, has to decide what happens to the original. C copies the bytes and leaves both. C++ calls a copy or move constructor. Rust *moves*: the value now belongs to the new owner, and the old name can't be used any more.

### How
- **A move:** `let moved = m;` copies the `Tracer`'s bytes to `moved`, and the compiler forbids any use of `m` from then on: `println!("{}", m.name)` after it would be the compile error "borrow of moved value: `m`". No code runs for a move, and there's no half-empty "moved-from" value left behind, so each value is dropped exactly once.
- **Moving into a function:** `take(moved)` gives the `Tracer` to `take`. It's dropped when `take` returns, so `[m] dropped` prints *before* `back from take`.
- **`clone()`** makes a copy on purpose. `take(c.clone())` gives the function its own `Tracer`, which it drops, and we keep `c`, which is dropped at the end of `demo_moves`. Cloning is always explicit: a type without `Clone` can't be copied at all.
- **`Copy` types** copy instead of moving: `let y = x;` leaves `x` usable. Numbers, `bool`, `char`, and structs and enums made only of `Copy` parts can be `Copy`, as lecture 1's `State` was. A type with a `Drop` impl can't be.

### Code
`lecture_2/src/main.rs`, part 2:
```rust
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
```

## 2.4 Borrowing

### Why
Most of the time a function just needs to look at a value, or change it, and give it back. Moving it in and out again would be clumsy. A *reference* borrows the value instead: it points at it without owning it.

### How
- **`&t`** is a shared reference. A function taking `&Tracer` can read the `Tracer`, but not change it.
- **`&mut t`** is a mutable reference: `rename` can change the caller's `Tracer`. The variable must be `let mut` to be lent this way.
- **The borrow rules:** many shared references at once, *or* one mutable reference, never both. The compiler checks them everywhere, which rules out a whole class of bugs: no one can change a value while someone else is reading it.
- **A borrow ends at the reference's last use,** not at the end of the block. `rename`'s `&mut` is over when `rename` returns, so the shared borrows after it are allowed.
- **Dereferencing:** a reference's value is `*reference`, but field access and method calls do that by themselves: `tracer.name` reads through the reference.
- **No dangling references:** a reference can't outlive the value it points at. Returning a reference to a local variable, the classic C bug, is a compile error: "cannot return reference to local variable", or, for a function with no reference parameter to borrow from, "missing lifetime specifier" first.

### Code
`lecture_2/src/main.rs`, part 3:
```rust
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
```

## 2.5 The heap

### Why
Values in variables live on the *stack*, and end with their scope unless they're moved out (returning a value moves it to the caller). A value whose size is only known at run time, that's too big for the stack, that must stay at one address, or that has several owners, goes on the *heap*, and something must own it.

### How
- **`Box<T>`** owns one value on the heap. `Box::new(value)` allocates memory and moves the value into it; dropping the `Box` drops the value and frees the memory. It's Rust's `unique_ptr`, or a `malloc` that can't be forgotten.
- **Moving a `Box`** moves only the pointer, 8 bytes; the `Tracer` stays where it is on the heap.
- **`drop(owner)`:** `drop` is a standard function that takes ownership and does nothing, so the value is dropped as it returns. It ends a life early: `[h] dropped` prints before `after drop(owner)`.
- **`Option<T>`** is a standard enum, like lecture 1's, with two variants: `Some(value)` and `None`. References and `Box`es are never null, so "a `Box`, or nothing" is `Option<Box<T>>`, and "a reference, or nothing" is `Option<&T>`. Neither costs anything: a `Box` is never null, so `None` is stored as null, and the `Option` is still 8 bytes. `assert_eq!` checks it, and panics if the two sizes ever differ.
- **`Rc<T>`** ("reference counted") is for a value with several owners and no obvious last one.
  - `Rc::new` puts the value on the heap with a count of owners. `Rc::clone(&shared)` adds an owner: it copies a pointer, not the `Tracer`.
  - `Rc::strong_count` reports the owners. The value is dropped when the last one goes: not at `drop(shared)`, but at `drop(another)`.
- **`Weak<T>`** watches an `Rc` value without keeping it alive; `Rc::downgrade(&shared)` makes one. `upgrade()` returns `Some` while an owner is left, and `None` afterwards; `is_some()` asks which.
- **The cost:** `Rc` keeps counters beside the value, and updates them on every clone and drop. It can't be shared between threads either (`Arc` can, lecture 5). Single ownership is the default; `Rc` is the exception.

### Code
`lecture_2/src/main.rs`, part 4:
```rust
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
```

## 2.6 Resources

### Why
Memory isn't the only thing a program must give back: files, locks, network connections and GPU objects all need releasing. `Drop` handles them all the same way.

### How
- **RAII** ("resource acquisition is initialization"): tie each resource to a value whose `Drop` releases it. The release can't be forgotten by accident, or done twice. It's skipped only if a value is leaked: on purpose (`std::mem::forget`, `Box::leak`), or by accident through a cycle of `Rc`s that own each other (which `Weak` exists to break), or if the process exits without unwinding.
- **`TempFile`** owns a file's path, and deletes the file when dropped. `Drop` prints nothing here: the demo checks the file itself, before and after the scope ends.
- **`fs::write`** creates the file. Like most I/O it can fail, so it returns a `Result`; `expect("...")` stops the program with that message if it did. Lecture 5 handles errors properly.
- **`path.clone()`:** the `TempFile` must own its path, but we still need ours to check on the file, so we give it a copy.
- **`let _ = ...`** in `drop` ignores the result of `remove_file` on purpose: there's nothing useful to do about a failed delete during cleanup.
- **`std::env::temp_dir()`** is the system's directory for temporary files, and `join` appends a file name to it.

### Code
`lecture_2/src/main.rs`, part 5:
```rust
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
```

## 2.7 Memory layout

### Why
Values are bytes at addresses, and the hardware has rules about which addresses each type may use: an `f64` should start at a multiple of 8, and SIMD instructions that load four floats at once want a multiple of 16. The compiler follows those rules, and they change how big our structs are. When we share structs with C (lecture 6) or the GPU (lecture 8), we must control the layout ourselves.

### How
- **Alignment:** every type has an alignment, a power of two that each value's address must be a multiple of. `align_of::<T>()` reports it: 1 for a `u8`, 8 for an `f64`.
- **Padding:** each field is placed at an offset that suits its alignment, and the gaps are unused *padding* bytes. A struct's alignment is the largest of its fields' alignments, and its size is rounded up to a multiple of it, so that every element of an array is aligned.
- **`offset_of!(Type, field)`** gives a field's distance from the start of the struct, in bytes.
- **`#[repr(C)]`** lays the fields out in declaration order, exactly as C would:
  - **`Loose`** (a `u8`, an `f64`, a `u8`): 1 byte, 7 padding, 8 bytes, 1 byte, 7 more padding: 24 in total.
  - **`Tight`**, the same fields largest first: the two `u8`s share one padded tail, so it's 16.
- **Rust's own layout** (no `repr`) may reorder fields, and does: `Reordered` declares `Loose`'s order, but is laid out like `Tight`, with `value` first. Any struct shared with C or the GPU needs `#[repr(C)]`, or its order isn't promised.
- **`#[repr(C, align(16))]`** raises a type's alignment: four `f32`s are 16 bytes with alignment 4, but every `Vec4` starts at a multiple of 16.
- **Unread fields:** Rust warns about struct fields that are never read, but not in `#[repr(C)]` structs, since C code might read them, and `offset_of!` counts as a use anyway. So none of these structs gets a warning.
- **`{:>2}`** right-aligns a number in 2 characters, so the columns line up.

### Code
`lecture_2/src/main.rs`, part 6:
```rust
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
```

## 2.8 Raw pointers and `unsafe`

### Why
References are always valid, and the compiler proves it. Sometimes, mostly when talking to C (lectures 6 and 7) or the GPU (lecture 8), we need plain addresses that the compiler can't check. Rust has them, behind a clearly marked door.

### How
- **Raw pointers:** `*const T` and `*mut T` are addresses, like C's pointers. They may be null or dangling, and the compiler checks nothing about them. Making one is safe: `let pointer: *const Vec4 = &on_stack;` converts a reference.
- **`&*on_heap`:** `*on_heap` is the `Vec4` inside the `Box`, and `&` borrows it, so that pointer is the heap address, not the address of the `Box` variable.
- **Addresses are numbers:** `pointer.addr()` is the address as a `usize`, so "16-aligned" is `addr().is_multiple_of(16)`. A local variable and a `Box` both honor `align(16)`.
- **`unsafe { ... }`:** reading through a raw pointer is unsafe, because only we can know that it still points at a live value. An `unsafe` block lets us do it; it doesn't switch any check off, it marks a promise. Every `unsafe` block in this course has a `// SAFETY:` comment saying why the promise holds.
- **`Vec4 { w: 1.0, ..Vec4::default() }`:** struct update syntax: the fields not named come from another value, here `Default`'s all-zero `Vec4`.
- **`debug_assert!(condition, "message")`** panics with the message if the condition is false, but only in debug builds: release builds never evaluate it, and the optimizer removes it. It checks for bugs in our own code, never for bad input. `assert!` is the same check in every build, like the `assert_eq!` of 2.5.

### Code
`lecture_2/src/main.rs`, part 7:
```rust
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
```

## 2.9 The program

### Why
`main` says which kind of build this is, then runs every demo in order.

### How
- **`cfg!(debug_assertions)`** is `true` in debug builds and `false` in release builds, so the first line says whether `debug_assert!` is checked.
- **`if` / `else`** on it picks one `println!`.

### Code
`lecture_2/src/main.rs`, part 8:
```rust
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
```

## 2.10 Build and run

```bash
cargo run -p lecture_2
```

```text
Debug build: debug_assert!() is checked

Scopes:
    end of the inner scope
    [b] dropped
    end of the outer scope
    [a] dropped
Moves and copies:
    took m
    [m] dropped
    back from take
    took c
    [c] dropped
    c is still ours: c
    x = 5, y = 5
    [c] dropped
Borrowing:
    borrowed t
    borrowed u
    two readers: u and u
    [u] dropped
Box:
    owner holds h; a Box is 8 bytes, and so is an Option<Box>
    [h] dropped
    after drop(owner)
Rc and Weak:
    owners: 2
    owners: 1, observer can reach it: true
    [s] dropped
    observer can reach it: false
A file deleted by Drop:
    inside the scope, exists: true
    after the scope, exists: false
Layout:
    Loose      size 24, align  8, offsets tag 0, value 8, flag 16
    Tight      size 16, align  8, offsets tag 8, value 0, flag 9
    Reordered  size 16, align  8, offsets tag 8, value 0, flag 9
    Vec4       size 16, align 16
Raw pointers:
    16-aligned: on the stack true, on the heap true
    w through the pointer: 1, x on the stack: 0
```

Reading the output:
- **Scopes:** `b` is dropped at the inner `}` and `a` at the outer one, newest first.
- **Moves:** `m` is dropped inside `take`, before `back from take`, and only once. The clone of `c` is dropped inside `take`, and our own `c` at the end of `demo_moves`, after the `i32` line.
- **Borrowing:** the `Tracer` was renamed through `&mut`, and dropped once, as `u`, at the end of the demo.
- **Box:** `drop(owner)` ended the `Tracer` early. A `Box` and an `Option<Box>` are both 8 bytes.
- **Rc:** `s` survived the first `drop`, because `another` still owned it, and died with the second. After that, the `Weak` can't reach it.
- **The file** existed inside the scope and was gone after it: `TempFile`'s `Drop` deleted it.
- **Layout:** padding exactly where we predicted, and `Reordered` packed like `Tight`, with `value` moved to the front.
- **Raw pointers:** both `Vec4`s are 16-aligned, and reading through the pointer found the `w` we stored.

A release build prints the same, except for the first line:
```bash
cargo run --release -p lecture_2 | head -1
```

```text
Release build: debug_assert!() is skipped
```

In lecture 3 we move from single values to many: arrays, slices, vectors and strings, all of which are contiguous memory with an owner.
