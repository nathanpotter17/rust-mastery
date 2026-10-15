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
