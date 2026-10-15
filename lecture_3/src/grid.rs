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
