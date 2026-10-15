# Lecture 3: Contiguous data

By the end of this lecture we'll run Conway's Game of Life on a small board that wraps around at the edges. Each run steps a starting pattern forward until a generation repeats, then reports the cycle it found. The glider prints its first five generations side by side as it crawls diagonally.

The Game of Life is just a board of cells, stored one after another in memory. That makes it the right vehicle for this lecture's subject, *contiguous* data: elements side by side in memory, and the types that own them, view them and walk them:
- arrays (`[T; N]`): fixed size, value semantics;
- slices (`&[T]`): a borrowed view of any contiguous elements;
- `Vec<T>`: a growable array, its capacity, and why the borrow checker rules out iterator invalidation;
- `String` and `&str`, the same owner/view pair for UTF-8 text;
- iterators, and the methods that replace hand-written loops;
- `static` items, and the `'static` lifetime.

Lecture 2's ideas are everywhere here. `Vec` and `String` are owners of heap memory with a `Drop`, and slices and `&str` are borrows, packaged with a length.

## 3.1 Project layout

### Why
The board and its rules are one concern, the starting patterns another, and the demos and simulation a third, so each gets a file: the first lecture split into several.

### How
- **Modules in files:** lecture 1 wrote `mod machine { ... }` with its contents inline. `mod grid;` with a `;` instead says "the module's contents are in `src/grid.rs`". So `src/main.rs` declares `mod grid;` and `mod patterns;`, and the code lives in `src/grid.rs` and `src/patterns.rs`.
- **Only declared files count:** a file under `src/` is part of the crate only if a `mod` declaration names it.
- **One compilation unit:** unlike C and C++, which compile each source file separately and link the results, Rust compiles a whole crate at once. There are no headers, and nothing needs declaring before use: `main.rs` reaches `grid.rs`'s items by their path, `grid::Grid`, or through a `use`.
- **Visibility:** an item is private to its module (and the modules inside it) unless it's `pub`, so everything `main.rs` calls in `grid.rs` is marked `pub`, and the helpers it doesn't, like `wrap`, aren't.
- **The rules of Life:** each cell is alive or dead, and has 8 neighbors. In each generation:
  - a live cell with 2 or 3 live neighbors survives;
  - a dead cell with exactly 3 comes alive;
  - every other cell is dead in the next generation.

### Code
From the repo root:
```bash
cargo new lecture_3
```

`lecture_3/Cargo.toml`:
```toml
[package]
name = "lecture_3"
version = "0.1.0"
edition = "2024"
```

## 3.2 The board: `src/grid.rs`

### Why
A board is a number of cells fixed at compile time: 8 × 8. We want them in one contiguous block, we want to copy and compare whole boards, and we want to look at one row without copying it. This file also holds the rules: reading and writing cells, counting, placing patterns, stepping a generation, and rendering text.

### How
- **`[bool; WIDTH * HEIGHT]`** is an array: exactly 64 `bool`s, side by side, with no heap allocation. The length is part of the type, and must be known at compile time, so it's built from `const`s. `[false; WIDTH * HEIGHT]` repeats one value to fill it.
- **Two dimensions in one:** the grid is stored *row-major*, row 0's cells first, then row 1's. Cell `(x, y)` is at index `y * WIDTH + x`. Nearly every image format, including the PNGs of lecture 7, stores pixels this way.
- **`#[derive(Clone, Copy, PartialEq)]`:**
  - **`Copy`:** an array of `Copy` values is `Copy`, and so is a struct of them. `let copy = original;` copies all 64 cells, and both stay usable.
  - **`PartialEq`:** `==` compares every field in order, here the array, which compares all 64 cells. The simulation uses it to spot a repeated generation. (The "partial" is for types like `f64`, where `NaN != NaN`.)
- **`Grid::new()`** is a `const fn`. `Default` would be the usual choice, but the standard library only implements `Default` for arrays of up to 32 elements, so `#[derive(Default)]` on a `[bool; 64]` field doesn't compile.
- **Wrapping around:** the board is a torus, so cell `(-1, 0)` is the last cell of row 0.
  - `%` keeps the sign of its left operand: `-1 % 8` is `-1`. `rem_euclid` always returns a value from `0` to `size - 1`: `(-1).rem_euclid(8)` is `7`.
  - `wrap` takes an `i32` (which may be negative) and returns a `usize` (an index), so it converts with `as` both ways. The values are small, so nothing is lost.
- **Indexing:** `cells[index]` checks the index, and *panics* if it's out of range: the program stops with an error, instead of reading whatever memory lies past the end. Our indexes come from `index_of`, which can only produce valid ones.
- **Slices:**
  - **What a slice is:** `&[bool]` borrows a run of contiguous `bool`s. It's a pointer to the first element and a count. It owns nothing, so making one copies no elements.
  - **`&self.cells[start..start + WIDTH]`** slices the array with a *range*: elements `start` up to, not including, `start + WIDTH`. `row(y)` returns that view of one row.
  - **The lifetime:** the returned slice borrows from `&self`, so it can't outlive the grid, and the grid can't change while the slice is in use.
- **`count_alive(cells: &[bool])`** takes a slice, so it accepts anything contiguous: an array, a `Vec`, or a piece of either, as `&array`, `&vec` or `&array[..n]`. A `&[bool; 64]` or a `&Vec<bool>` converts to a `&[bool]` automatically.
- **Iterators:**
  - **`iter()`** walks a slice's elements as references; `filter` keeps the ones the closure accepts; `count` counts them. Each step returns a new iterator, and nothing runs until `count` asks for elements. Iterators are *lazy*.
  - **`**cell`:** `filter` hands its closure a reference to each element, and the elements are already `&bool`, so the closure sees a `&&bool`: two `*`s get to the `bool`.
  - **Prefer named methods to hand-written loops:** the name says what the loop would only show, and there's no index to get wrong.
- **What a `for` loop does:** `place` spells it out once, so we can see it.
  - **`next()`:** an iterator's `next()` returns `Some(item)` until the items run out, then `None`. `while let Some(...) = rows.next()` runs until the first `None`. `for row in rows` is shorthand for exactly this.
  - **`enumerate()`** pairs each element with its index, and `row.bytes()` walks a string's bytes.
  - **The clippy allow:** Clippy would suggest the `for` loop; spelling it out is the point here.
- **`step`** builds a new grid from the old one, because every cell must see the *previous* generation's neighbors. It returns the `Grid` by value: a 64-byte copy, which is cheap. `-1..=1` is a range that includes both ends.
- **`render`** builds a `String`:
  - **The owner:** a `String` owns its text on the heap, and frees it when dropped: lecture 2's `Drop` again.
  - **`String::with_capacity`:** allocates room for all 72 characters up front.
  - **`push`:** appends a `char`, and the string grows as needed.
  - **`as_chunks::<WIDTH>()`** splits a slice into a slice of `[bool; WIDTH]` arrays, one per row, plus a slice of whatever's left over (nothing here, since 64 divides by 8). `let (rows, _) = ...` takes the first part of the pair and ignores the second.
  - **`for &cell in row`** iterates over references, and the `&` in the pattern copies each `bool` out of its reference.

### Code
`lecture_3/src/grid.rs`:
```rust
// The board

pub const WIDTH: usize = 8;
pub const HEIGHT: usize = 8;

// A Game of Life board that wraps around: the cells on the right edge are neighbors of those on the left edge, and the bottom row of the top row. Copy: a Grid is plain bytes, so it copies like a number. PartialEq: == compares every field, here all 64 cells.
#[derive(Clone, Copy, PartialEq)]
pub struct Grid {
    // Row-major order: all of row 0, then all of row 1, and so on. Cell (x, y) is at index y * WIDTH + x.
    pub cells: [bool; WIDTH * HEIGHT],
}

// % keeps the sign of its left side (-1 % 8 is -1). rem_euclid always lands in 0 .. size - 1, so -1 wraps to 7.
fn wrap(value: i32, size: usize) -> usize {
    value.rem_euclid(size as i32) as usize
}

fn index_of(x: i32, y: i32) -> usize {
    wrap(y, HEIGHT) * WIDTH + wrap(x, WIDTH)
}

impl Grid {
    // [false; N] repeats one value N times. const fn: Grid::new() can make a constant.
    pub const fn new() -> Grid {
        Grid { cells: [false; WIDTH * HEIGHT] }
    }

    // x and y wrap around, so (-1, 0) is the last cell of row 0.
    pub fn alive(&self, x: i32, y: i32) -> bool {
        self.cells[index_of(x, y)]
    }

    pub fn set(&mut self, x: i32, y: i32, value: bool) {
        self.cells[index_of(x, y)] = value;
    }

    // A view of one row: no copy, just a pointer to its first cell and a length. The slice borrows the grid, so it can't outlive it.
    pub fn row(&self, y: i32) -> &[bool] {
        let start = index_of(0, y);
        &self.cells[start..start + WIDTH]
    }
}

// Functions on boards

// Counts the true values in any contiguous run of bools: an array, a Vec, or a piece of either.
pub fn count_alive(cells: &[bool]) -> usize {
    // An iterator chain instead of a loop: keep the true cells, then count them.
    cells.iter().filter(|cell| **cell).count()
}

// Copies a pattern onto the grid with its top-left corner at (x, y). Each row of the pattern is text: '#' is alive, anything else is dead.
pub fn place(grid: &mut Grid, pattern: &[&str], x: i32, y: i32) {
    // Spelled out, this is what a for loop does: next() returns Some(item) until the items run out, then None.
    let mut rows = pattern.iter().enumerate();
    #[allow(clippy::while_let_on_iterator)] // spelled out on purpose
    while let Some((dy, row)) = rows.next() {
        for (dx, byte) in row.bytes().enumerate() {
            grid.set(x + dx as i32, y + dy as i32, byte == b'#');
        }
    }
}

// The next generation: a live cell with 2 or 3 live neighbors survives, a dead cell with exactly 3 comes alive, and every other cell is dead.
pub fn step(grid: &Grid) -> Grid {
    let mut next = Grid::new();

    for y in 0..HEIGHT as i32 {
        for x in 0..WIDTH as i32 {
            let mut neighbors = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx != 0 || dy != 0) && grid.alive(x + dx, y + dy) {
                        neighbors += 1;
                    }
                }
            }

            let alive = grid.alive(x, y);
            next.set(x, y, neighbors == 3 || (alive && neighbors == 2));
        }
    }

    // Returning moves the Grid out to the caller.
    next
}

// The grid as text, one line per row.
pub fn render(grid: &Grid) -> String {
    // One allocation up front: each row is WIDTH characters plus a '\n'.
    let mut text = String::with_capacity((WIDTH + 1) * HEIGHT);

    // as_chunks splits the cells into whole [bool; WIDTH] arrays, one per row, and a slice of whatever is left over (nothing, here).
    let (rows, _) = grid.cells.as_chunks::<WIDTH>();
    for row in rows {
        for &cell in row {
            text.push(if cell { '#' } else { '.' });
        }
        text.push('\n');
    }

    text
}
```

## 3.3 Starting patterns: `src/patterns.rs`

### Why
Patterns are small, fixed tables of text, known at compile time. Arrays are the natural fit, and they should exist once, for the whole program.

### How
- **`[&str; 3]`** is an array of three string slices. `&str` borrows text; a string literal like `".#."` is text built into the program, so its type is `&'static str`.
- **`'static`** is a *lifetime*: the reference is valid for the whole program. In a `static` item's type, the compiler assumes `'static` for every reference, so `[&str; 3]` here means `[&'static str; 3]`.
- **`static` versus `const`:** a `static` is one value at one address for the whole program. A `const` is a value that the compiler copies into each place it's used. For a table we look at from several places, `static` keeps one copy.
- **`#[rustfmt::skip]`** stops `cargo fmt` from joining the rows into one line, so each pattern stays drawn row by row. Each row is text, with `#` for a live cell and `.` for a dead one.

### Code
`lecture_3/src/patterns.rs`:
```rust
// Starting patterns

// Arrays of string slices: the length is part of the type, [&str; 3]. A static has one address for the whole program, so every use shares this one array. String literals live that long too, so each &str here is a &'static str. rustfmt::skip keeps each pattern drawn row by row.
#[rustfmt::skip]
pub static BLOCK: [&str; 2] = [
    "##",
    "##",
];

#[rustfmt::skip]
pub static BLINKER: [&str; 1] = [
    "###",
];

#[rustfmt::skip]
pub static GLIDER: [&str; 3] = [
    ".#.",
    "..#",
    "###",
];
```

## 3.4 The program: `src/main.rs`

### Why
`main` first demonstrates each contiguous type on its own, then runs three patterns to their cycles. Each demo is a small function, so its output is labeled and its variables don't leak into the next one.

### How
- **Text and its length:**
  - **No terminator:** a `str` is just its UTF-8 bytes. `size_of_val("abc")` is 3, because the length travels beside the pointer in every `&str` instead of being marked by a `'\0'` at the end.
  - **C strings:** C marks the end of text with a `'\0'` byte. `c"abc"` is a `&CStr`, a C string, whose bytes include it: 4 of them. Lecture 6 passes these to C.
- **Arrays, slices and pointers:**
  - **Sizes:** `GLIDER.len()` is the element count, 3, and `size_of_val(&GLIDER)` the size in bytes, 48: three `&str`s of 16 bytes each.
  - **`&[&str; 3]` versus `&[&str]`:** a reference to an array has the length in its type, so it's a bare 8-byte pointer. A slice reference carries the length beside the pointer, 16 bytes. `let slice: &[&str] = &GLIDER;` converts one into the other.
  - **No decay:** unlike C, an array never silently turns into a pointer. `as_ptr()` asks for one explicitly, and the raw pointer knows nothing about how many elements follow. Reading through it needs `unsafe`.
  - **`last()`** returns an `Option`, because an empty slice has no last element. `{:?}` prints it as `Some("###")`.
- **Arrays are values:**
  - **Copies are independent:** `let mut copy = original;` copies the 64 cells, and changing the copy, or the original, leaves the other alone.
  - **`get(index)`** checks the index and returns `Some(&cell)` or `None`, where `[]` would panic. Use `get` when an index might be out of range for a reason that isn't a bug.
- **Slices:**
  - **One function, four kinds of storage:** `count_alive` counts an array, a `Vec`, one row, and the first two rows, all through the same `&[bool]` parameter.
  - **`vec![...]`** builds a `Vec` from a list of elements. A `Vec<bool>` is a plain array of `bool`s on the heap, so it converts to a `&[bool]` like any other.
  - **`..2 * WIDTH`** is a range from the start: `&grid.cells[..16]` is the first 16 cells.
- **`String` and `&str`:**
  - **Owner and view:** `text` owns its characters, and `view` and `row` only look at them. Slicing a `&str` makes another `&str`, with no copy.
  - **Not found:** `find` returns `Some(byte_position)` or `None`. There's no special "not found" number to check for.
  - **No change while borrowed:** while `row` borrows `text`, `text` can't be changed: `replace_range` needs `&mut text`, and the borrow rules allow no `&mut` while a `&` is in use. Here `row` is never used after the `replace_range`, so its borrow has already ended. Printing `row` after the change would not compile, which is exactly the "view of changed data" bug the borrow checker exists to prevent.
  - **UTF-8:** text is stored as UTF-8, where a character takes 1 to 4 bytes. `len()` counts bytes and `chars().count()` counts characters, so `"héllo"` is 6 and 5. String positions are byte positions, and slicing in the middle of a character panics.
- **`Vec<Grid>`** holds every generation of a run:
  - **Length and capacity:** `len()` is how many elements it holds, and `capacity()` how many fit in its current heap block. When `push` finds the block full, it asks the allocator for a bigger one (twice the size, here). The allocator may be able to extend the block where it is; if not, it allocates a new block, moves every element across, and frees the old one. The output shows each of these reallocations.
  - **The first allocation** holds 4 grids: for small elements, `Vec` skips the tiny sizes. `Vec::with_capacity(n)` allocates room for `n` up front, so pushing up to `n` never reallocates. `print_side_by_side` uses it.
  - **Iterator invalidation can't happen:** a reallocation may move every element, so any reference into the old block could dangle. In Rust, holding a reference into `history` borrows it, and `push` needs `&mut history`, so the borrow checker rejects the combination. `find_cycle` gets an *index* from `position`, which borrows nothing, before it calls `push`.
  - **`push(render(frame))`** moves the new `String` into the vector. Only the string's 24-byte header moves; its characters stay where they are on the heap.
- **Finding a cycle:**
  - **The search:** `history.iter().position(|earlier| *earlier == grid)` compares the new generation with every earlier one, using `Grid`'s derived `==`, and returns the index of the first match, or `None`.
  - **The period:** the cycle's length is the current generation minus that index.
- **The table of starts:**
  - **The type:** `Start` holds a name and a slice of one pattern array, so patterns of different sizes fit in one table. Both fields are `&'static`: a struct that holds references must say how long they live, and these point at statics, which live for the whole program.
  - **The loop:** `for start in &starts` borrows each element instead of moving it out.
  - **The glider's frames:** `&history[..5]` views its first five generations without copying them.

### Code
`lecture_3/src/main.rs`:
```rust
mod grid;
mod patterns;

use grid::{Grid, HEIGHT, WIDTH, count_alive, place, render, step};

use std::mem::{size_of, size_of_val};
use std::process::ExitCode;

// Arrays and slices

fn demo_arrays() {
    println!("Arrays and slices:");

    // A str holds just its bytes, with no '\0' at the end: a &str carries the length beside the pointer. C strings, written c"...", keep the '\0'.
    println!("    \"abc\": {} bytes, c\"abc\": {} bytes with its '\\0'", size_of_val("abc"), c"abc".to_bytes_with_nul().len());
    println!("    GLIDER: {} rows, {} bytes", patterns::GLIDER.len(), size_of_val(&patterns::GLIDER));

    // A reference to an array keeps the length in its type, so it's a bare pointer. A slice reference carries the length beside the pointer.
    println!("    &[&str; 3]: {} bytes, &[&str]: {} bytes", size_of::<&[&str; 3]>(), size_of::<&[&str]>());

    // The array converts to a slice, and the slice knows its length. last() returns an Option: an empty slice has no last element.
    let slice: &[&str] = &patterns::GLIDER;
    println!("    as a slice: {} rows, last row {:?}", slice.len(), slice.last());

    // A raw pointer is just the address of the first row: the row count is lost.
    let first: *const &str = patterns::GLIDER.as_ptr();
    // SAFETY: first points at GLIDER's first row, and GLIDER lives for the whole program.
    println!("    as a raw pointer: {} bytes, first row {:?}, row count lost", size_of::<*const &str>(), unsafe { *first });
}

// Arrays are values

fn demo_array_values() {
    println!("Arrays are values:");
    let mut original = Grid::new();
    place(&mut original, &patterns::BLINKER, 2, 3);

    // Grid is Copy, so = copies all 64 cells, and the copy is independent.
    let mut copy = original;
    copy.set(0, 0, true);
    println!("    copy == original: {}, alive {} vs {}", copy == original, count_alive(&copy.cells), count_alive(&original.cells));

    // [] checks the index and panics if it's out of range. get() checks it and returns None instead.
    println!("    cells.get(64): {:?}, cells.get(3): {:?}", original.cells.get(64), original.cells.get(3));

    // The other kind of mutation: change the original, and the copy stays as it was.
    original = Grid::new();
    println!("    after clearing the original: alive {} vs {}", count_alive(&copy.cells), count_alive(&original.cells));
}

// Slices

fn demo_slices(grid: &Grid) {
    println!("Slices:");

    // A slice reference is a pointer and a length. A reference to an array needs only the pointer.
    println!("    size_of: &[bool] {}, &[bool; 64] {}", size_of::<&[bool]>(), size_of::<&[bool; 64]>());

    // One function, four kinds of contiguous storage.
    let flags = [true, false, true];
    let list = vec![true, true, true, false];
    println!("    alive in an array: {}", count_alive(&flags));
    println!("    alive in a Vec: {}", count_alive(&list));
    println!("    alive in the grid: {}", count_alive(&grid.cells));
    println!("    alive in row 2: {}", count_alive(grid.row(2)));

    // ..n is a range from the start: the first two rows.
    println!("    alive in the first two rows: {}", count_alive(&grid.cells[..2 * WIDTH]));
}

// String and &str

fn demo_strings(grid: &Grid) {
    println!("String and &str:");

    // A String owns its bytes, and frees them when it's dropped.
    let mut text = render(grid);
    println!("    rendered {} characters", text.len());

    // A &str only looks at bytes someone else owns. Slicing a &str makes another &str, with no copy.
    let view: &str = &text;
    let row = &view[WIDTH + 1..2 * WIDTH + 1];

    // find returns an Option: Some(position) or None. There's no special "not found" number.
    println!("    row 1 is {row:?}, first '#' at {:?}, an '@' at {:?}", row.find('#'), row.find('@'));

    // While row borrows text, text can't change. Once row is no longer used, it can.
    text.replace_range(WIDTH + 1..WIDTH + 2, "@");
    println!("    after changing text, row 1 is {:?}", &text[WIDTH + 1..2 * WIDTH + 1]);

    // Text is UTF-8: a char may take more than one byte, and len() counts bytes.
    let word = "héllo";
    println!("    {word:?} has {} bytes and {} chars", word.len(), word.chars().count());
}

// Running until a generation repeats

struct Cycle {
    start: usize,  // the first generation of the cycle
    period: usize, // generations per cycle; 0 if none was found
}

fn find_cycle(start: Grid, history: &mut Vec<Grid>, limit: usize) -> Cycle {
    let mut grid = start;

    for generation in 0..limit {
        // position() searches with a closure and returns the index of the first match.
        if let Some(seen) = history.iter().position(|earlier| *earlier == grid) {
            return Cycle { start: seen, period: generation - seen };
        }

        let capacity = history.capacity();
        history.push(grid);
        if history.capacity() != capacity {
            println!("    len {:>2}: capacity {:>2} -> {:>2}", history.len(), capacity, history.capacity());
        }

        grid = step(&grid);
    }

    Cycle { start: 0, period: 0 }
}

fn print_side_by_side(frames: &[Grid]) {
    let mut texts = Vec::with_capacity(frames.len());
    for frame in frames {
        // render returns a new String, and push moves it into the Vec.
        texts.push(render(frame));
    }

    for y in 0..HEIGHT {
        let mut line = String::from("   ");
        for text in &texts {
            let offset = y * (WIDTH + 1);
            line.push(' ');
            line.push_str(&text[offset..offset + WIDTH]);
        }
        println!("{line}");
    }
}

// The program

// 'static: the references live for the whole program, like the patterns they point at.
struct Start {
    name: &'static str,
    pattern: &'static [&'static str],
}

fn main() -> ExitCode {
    demo_arrays();
    println!();
    demo_array_values();
    println!();

    let mut glider = Grid::new();
    place(&mut glider, &patterns::GLIDER, 0, 0);
    demo_slices(&glider);
    println!();
    demo_strings(&glider);
    println!();

    // An array of structs; each slice views one of the pattern arrays.
    let starts = [
        Start { name: "block", pattern: &patterns::BLOCK },
        Start { name: "blinker", pattern: &patterns::BLINKER },
        Start { name: "glider", pattern: &patterns::GLIDER },
    ];

    for start in &starts {
        println!("{}:", start.name);

        let mut grid = Grid::new();
        place(&mut grid, start.pattern, 2, 2);

        let mut history = Vec::new();
        let cycle = find_cycle(grid, &mut history, 100);
        println!("    repeats from generation {} every {} generations", cycle.start, cycle.period);

        if start.name == "glider" {
            // The first five generations: the glider moves one cell diagonally.
            print_side_by_side(&history[..5]);
        }
    }

    ExitCode::SUCCESS
}
```

## 3.5 Build and run

```bash
cargo run -p lecture_3
```

```text
Arrays and slices:
    "abc": 3 bytes, c"abc": 4 bytes with its '\0'
    GLIDER: 3 rows, 48 bytes
    &[&str; 3]: 8 bytes, &[&str]: 16 bytes
    as a slice: 3 rows, last row Some("###")
    as a raw pointer: 8 bytes, first row ".#.", row count lost

Arrays are values:
    copy == original: false, alive 4 vs 3
    cells.get(64): None, cells.get(3): Some(false)
    after clearing the original: alive 4 vs 0

Slices:
    size_of: &[bool] 16, &[bool; 64] 8
    alive in an array: 2
    alive in a Vec: 3
    alive in the grid: 5
    alive in row 2: 3
    alive in the first two rows: 2

String and &str:
    rendered 72 characters
    row 1 is "..#.....", first '#' at Some(2), an '@' at None
    after changing text, row 1 is "@.#....."
    "héllo" has 6 bytes and 5 chars

block:
    len  1: capacity  0 ->  4
    repeats from generation 0 every 1 generations
blinker:
    len  1: capacity  0 ->  4
    repeats from generation 0 every 2 generations
glider:
    len  1: capacity  0 ->  4
    len  5: capacity  4 ->  8
    len  9: capacity  8 -> 16
    len 17: capacity 16 -> 32
    repeats from generation 0 every 32 generations
    ........ ........ ........ ........ ........
    ........ ........ ........ ........ ........
    ...#.... ........ ........ ........ ........
    ....#... ..#.#... ....#... ...#.... ....#...
    ..###... ...##... ..#.#... ....##.. .....#..
    ........ ...#.... ...##... ...##... ...###..
    ........ ........ ........ ........ ........
    ........ ........ ........ ........ ........
```

Reading the output:
- **Lengths:** `"abc"` takes 3 bytes and its C string 4. The array of three `&str`s is 48 bytes.
- **The raw pointer** knows its first row, but not that there are three.
- **The copy** has 4 live cells and the original 3: copies of an array are independent.
- **The slices:** a reference to an array is half the size of a slice reference. `count_alive` counted an array, a `Vec`, one row, and the first two rows of the board, all through the same parameter type.
- **The vector's capacity** starts at 4 and doubles each time it fills: 4, 8, 16, 32. Each of those lines was a reallocation that *could* have moved every grid stored so far, and that we can't rely on not moving them; whether a block is extended in place is up to the allocator. Had we known the count, `with_capacity` would have made it one allocation.
- **The block** never changes, so it repeats with a period of 1; the blinker flips between two shapes, a period of 2. The glider returns to its starting shape every 4 generations, one cell further down and to the right. On an 8 × 8 board that wraps around, it's back in exactly the same place after 8 × 4 = 32 generations.

In lecture 4 we name our types precisely with aliases and newtypes, and lean much harder on the standard library: `Option`, parsing, maps, closures and iterator chains.
