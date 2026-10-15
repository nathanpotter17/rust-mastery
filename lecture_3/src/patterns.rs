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
