# Lecture 4: Type aliases and the standard library

By the end of this lecture we'll have a small batch calculator. It reads expressions like `"12 + 12"` from text, evaluates them with 32-bit arithmetic that can't silently overflow, prints every result in decimal, hex and binary, and summarizes the batch. The calculator is only the excuse. The lecture has two threads:

- **Naming types.** We give types names with `type` aliases, including generic aliases and aliases for function pointers. We'll see what an alias *can't* do, and use the two tools that make real new types: the newtype pattern, and an enum with a chosen representation. *Associated types* let a trait choose a type per implementation, which is how generic code computes types.
- **The standard library.** Instead of writing helpers ourselves, we use the library's vocabulary: `Option` and its methods, `?`, `str::parse`, the checked integer methods, `BTreeMap`, closures and `dyn FnMut`, and iterator adapters like `filter` and `flatten`, and consumers like `fold`. Lecture 3's arrays, slices, `Vec` and `&str` are used throughout.

## 4.1 Project layout

### Why
The program is split by concern, one module per idea, so each section of this lecture introduces exactly one file.

### How
- **The layout is lecture 3's:** `src/main.rs` declares the modules, and each lives in its own file.
- **Paths between modules:** a module refers to its siblings through `crate::`, the root of the crate: `use crate::types::Answer;` in `operations.rs`.
- **Nothing to download:** everything we use ships with Rust.

### Code
From the repo root:
```bash
cargo new lecture_4
```

`lecture_4/Cargo.toml`:
```toml
[package]
name = "lecture_4"
version = "0.1.0"
edition = "2024"
```

## 4.2 Naming types: `src/types.rs`

### Why
A program says the same types over and over, and some of them are long: `Option<i32>`, `fn(i32, i32) -> Option<i32>`, `BTreeMap<&str, usize>`. Aliases give them short names that also say what they *mean*, and let a whole program change a type by editing one line. But an alias is only a name, and sometimes we need the compiler to tell two things apart.

### How
- **`type Answer = Option<i32>;`** declares an alias: `Answer` and `Option<i32>` are the same type, everywhere. A calculation either produces an `i32` or fails (overflow, division by zero), and `Option` says exactly that: `Some(value)` or `None`.
- **Function pointer types:** `fn(i32, i32) -> Answer` is the type of a pointer to any function with that signature. Calling one is calling the function. `BinaryOp` names it.
- **Generic aliases:** `type Tally<K> = BTreeMap<K, usize>;` takes a type parameter, so `Tally<&str>` is a map from names to counts. A `BTreeMap` keeps its keys sorted.
- **Sizes:** `i32` uses every one of its bit patterns, so `Option<i32>` needs a separate tag for `None`: 8 bytes, as the const assert checks. Lecture 2's `Option<Box<T>>` cost nothing, because a `Box` can never be null.
- **An alias is not a new type.** `Meters` and `Seconds` are both `f64`, so the compiler happily adds meters to seconds.
- **Newtypes:** `struct Celsius(pub f64);` is a *tuple struct*, a struct whose fields have no names, read as `.0`, `.1` and so on. One field makes a *newtype*: a real new type, wrapped around an existing one, that costs nothing at run time.
  - **No operations to start with:** `Celsius(20.0) + 1.0` doesn't compile, and neither does `Celsius + Celsius`. A newtype only does what we implement for it (lecture 6 implements `Add`).
  - **`#[derive(Debug)]`** lets `{:?}` print it, as `Celsius(21.5)`.
- **Associated types:**
  - **What they are:** a trait can declare a type that each implementation chooses. `Bits` declares `type Unsigned;`, and `i32`'s implementation chooses `u32`. Code generic over `T: Bits` can then name `T::Unsigned`, the unsigned type with the same size as `T`, without knowing what it is.
  - **The method:** `bits(self) -> Self::Unsigned` reinterprets a value as its unsigned counterpart. A negative number keeps its bit pattern, so hex and binary output shows the two's complement bits (`-1` is `0xffffffff`) instead of a minus sign. The standard library's `cast_unsigned` does the work.
  - **Implementing a trait for a built-in type:** we may implement our own traits for any type, including `i32`.
- **`macro_rules!`** writes code for us, as `println!` does. `impl_bits!` matches a comma-separated list of `$signed:ty => $unsigned:ty` pairs, and for each pair, writes one `impl Bits` block with the types filled in. In the pattern, `$( ... ),*` means "repeated, separated by commas"; in the output, `$( ... )*` writes the block once per match, with no separator. Four impls for the price of one.
- **An alias through a trait:** `type UnsignedOf<T> = <T as Bits>::Unsigned;` names the associated type. `<T as Bits>` says which trait's `Unsigned` we mean.
- **Checking a type at compile time:** `const _: fn(UnsignedOf<i32>) -> u32 = |value| value;` is a constant holding a function pointer, made from a closure that returns its argument. It compiles only if `UnsignedOf<i32>` is `u32`, so the build checks it for us.

### Code
`lecture_4/src/types.rs`:
```rust
use std::collections::BTreeMap;
use std::mem::size_of;

// Aliases

// A type alias is another name for an existing type: the compiler can't tell them apart. A calculation either produces an i32 or fails (overflow, division by zero).
pub type Answer = Option<i32>;

// A function pointer type. Every plain function with this signature fits, and so does every closure that captures nothing.
pub type BinaryOp = fn(i32, i32) -> Answer;

// A generic alias: Tally<&str> is BTreeMap<&str, usize>, a count per key.
pub type Tally<K> = BTreeMap<K, usize>;

// i32 has no spare bit pattern to mean None, so Option<i32> needs a tag beside the value.
const _: () = assert!(size_of::<Answer>() == 8);

// An alias is not a new type

// Meters and Seconds are both f64, so the compiler happily adds one to the other.
pub type Meters = f64;
pub type Seconds = f64;

// A newtype: a struct with one unnamed field, read as .0. It's a real new type, and starts with no operations at all: Celsius(20.0) + 1.0 doesn't compile.
#[derive(Clone, Copy, Debug)]
pub struct Celsius(pub f64);

// Associated types

// A trait whose implementations each name a type. Bits::Unsigned is the unsigned type with the same size as Self.
pub trait Bits {
    type Unsigned;

    // The same bits, read as unsigned. -1 becomes 0xffffffff, so hex and binary output shows the two's complement pattern instead of a minus sign.
    fn bits(self) -> Self::Unsigned;
}

// macro_rules! writes code for us: here, one impl per (signed, unsigned) pair in the list.
macro_rules! impl_bits {
    ($($signed:ty => $unsigned:ty),*) => {
        $(
            impl Bits for $signed {
                type Unsigned = $unsigned;

                fn bits(self) -> $unsigned {
                    self.cast_unsigned()
                }
            }
        )*
    };
}

impl_bits!(i8 => u8, i16 => u16, i32 => u32, i64 => u64);

// A generic alias that reaches into a trait: UnsignedOf<i32> is <i32 as Bits>::Unsigned.
pub type UnsignedOf<T> = <T as Bits>::Unsigned;

// This compiles only if UnsignedOf<i32> and u32 are the same type.
const _: fn(UnsignedOf<i32>) -> u32 = |value| value;
```

## 4.3 Checked arithmetic: `src/operations.rs`

### Why
Integer overflow is a bug in Rust: an `i32` addition that overflows at run time panics in a debug build, and silently wraps around in a release build. (Written with constants, as `i32::MAX + 1`, it doesn't even compile: the compiler sees the overflow coming.) A calculator has to detect it instead, every time, and say "no result". We also want the operators in a table, so finding and adding them is data, not a chain of `if`s.

### How
- **The checked methods:** `i32::checked_add(a, b)` returns `Some(sum)`, or `None` if the sum doesn't fit. `checked_sub`, `checked_mul`, `checked_div`, `checked_rem` and `checked_pow` work the same way. `checked_div` and `checked_rem` also return `None` for the two divisions that would panic: by zero, and `i32::MIN / -1`, whose result is one more than `i32::MAX`.
- **Methods are functions:** `i32::checked_add` is a plain function of two `i32`s returning `Option<i32>`, so it fits a `BinaryOp` field as it is.
- **Closures as function pointers:** `|base, exponent| ...` is a closure. One that *captures* nothing (uses no variables from around it) converts to a plain `fn` pointer, so it fits `BinaryOp` too.
  - **The exponent:** `checked_pow` takes a `u32`, and `u32::try_from(exponent)` converts an `i32`, failing for negative ones. `TryFrom` is the checked counterpart of lecture 1's `From`: it returns a `Result`, and `.ok()` turns that into an `Option`, so `?` can return `None` from the closure.
- **`#[repr(u8)] enum Symbol`:** lecture 1's enums let the compiler pick how to store them, and some carried data. `#[repr(u8)]` stores every `Symbol` as one byte, and `Add = b'+'` chooses that byte: the character that writes the symbol.
  - **To a byte:** `Symbol::Add as u8` is `b'+'`, a plain cast.
  - **From a byte:** there's no cast the other way. Most bytes name no `Symbol`, and an enum value must always be one of its variants: the compiler relies on that, for example to know a `match` is complete. So `from_byte` is a `match`, and returns an `Option`.
  - **A real new type:** a `Symbol` won't mix with a `u8` by accident: `let s: Symbol = b'+';` doesn't compile.
- **The table:**
  - **The element type:** `Operation` holds a `Symbol`, a name and a `BinaryOp`.
  - **`static OPERATIONS: [Operation; 6]`:** one table, at one address, for the whole program.
  - **`find_operation`:** `iter().find(...)` hands each element to the closure, and returns the first one it accepts, or `None`. The result is an `Option<&'static Operation>`: a reference into the static table, valid forever.
  - **`#[derive(PartialEq)]`** on `Symbol` is what makes `operation.symbol == symbol` compile.

### Code
`lecture_4/src/operations.rs`:
```rust
use crate::types::BinaryOp;

// Operator symbols

// #[repr(u8)]: every Symbol is stored as one byte, and we choose each one: the character that writes it.
#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Symbol {
    Add = b'+',
    Subtract = b'-',
    Multiply = b'*',
    Divide = b'/',
    Remainder = b'%',
    Power = b'^',
}

impl Symbol {
    // A byte can't just be cast to a Symbol: most bytes name no Symbol, and an enum must always hold one of its values. So this is a match, and it returns an Option.
    pub fn from_byte(byte: u8) -> Option<Symbol> {
        match byte {
            b'+' => Some(Symbol::Add),
            b'-' => Some(Symbol::Subtract),
            b'*' => Some(Symbol::Multiply),
            b'/' => Some(Symbol::Divide),
            b'%' => Some(Symbol::Remainder),
            b'^' => Some(Symbol::Power),
            _ => None,
        }
    }

    // The other way is a plain cast: Symbol::Add as u8 is b'+'.
    pub fn as_char(self) -> char {
        char::from(self as u8)
    }
}

// The operation table

pub struct Operation {
    pub symbol: Symbol,
    pub name: &'static str,
    pub apply: BinaryOp,
}

// i32's checked_* methods return None instead of overflowing, so each fits BinaryOp as it is. checked_div and checked_rem also refuse to divide by zero, or i32::MIN by -1. The power is a closure that captures nothing, which converts to a fn pointer too: a negative exponent fails u32::try_from.
pub static OPERATIONS: [Operation; 6] = [
    Operation { symbol: Symbol::Add, name: "add", apply: i32::checked_add },
    Operation { symbol: Symbol::Subtract, name: "subtract", apply: i32::checked_sub },
    Operation { symbol: Symbol::Multiply, name: "multiply", apply: i32::checked_mul },
    Operation { symbol: Symbol::Divide, name: "divide", apply: i32::checked_div },
    Operation { symbol: Symbol::Remainder, name: "remainder", apply: i32::checked_rem },
    Operation { symbol: Symbol::Power, name: "power", apply: |base, exponent| base.checked_pow(u32::try_from(exponent).ok()?) },
];

// find() hands each element to the closure, and returns the first one it accepts.
pub fn find_operation(symbol: Symbol) -> Option<&'static Operation> {
    OPERATIONS.iter().find(|operation| operation.symbol == symbol)
}
```

## 4.4 Parsing text: `src/expression.rs`

### Why
Expressions arrive as text, so we need to turn `"-7 * 6"` into two numbers and an `Operation`. Parsing is a chain of steps that can each fail, and we want the failure path to cost one character per step.

### How
- **`&str` is the parser's natural input,** as in lecture 3. Taking one is as cheap as passing two integers, and every smaller `&str` we cut from it borrows the same text, with no copies.
- **Calling a fn pointer field:** `(self.operation.apply)(self.lhs, self.rhs)`. The parentheses say "call the field", where `self.operation.apply(...)` would look for a *method* named `apply`.
- **`trim_start_matches(' ')`** drops leading spaces.
- **`take_number`** returns `Option<(i32, &str)>`: the number, and the rest of the text.
  - **Tuples:** `(i32, &str)` is a *tuple*, an unnamed group of values, and `let (lhs, rest) = ...` takes it apart.
  - **The lifetime:** the returned `&str` borrows from the `text` argument. With one reference in and one out, the compiler connects them by itself.
  - **Finding the end:** the number is an optional `-`, then digits. `usize::from(true)` is `1`, so `sign` is how many bytes the `-` takes. `find` with a closure returns the position of the first non-digit after it, and `map_or` turns "no such position" into "the end of the text".
  - **`split_at(end)`** cuts the text in two at a byte position.
  - **`str::parse`** reads a number from text. The type to parse into comes from where the result goes: here the `i32` in the return type. It fails on an empty string, a lone `-`, or a number too big for an `i32`, and returns a `Result`, which `.ok()?` turns into an early `None`.
- **`parse`:**
  - **`?` on an `Option`:** `take_number(text)?` either unwraps the `Some`, or returns `None` from `parse` at once. A chain of steps that can fail reads like a chain of steps that can't.
  - **The operator:** `rest.as_bytes().first()?` gets the first byte, and `Symbol::from_byte(...)?` and `find_operation(...)?` look it up.
  - **Slicing after it:** the symbol is one ASCII byte, so `&rest[1..]` starts at a character boundary.
  - **Leftovers:** anything but spaces after the second number makes the whole input bad.

### Code
`lecture_4/src/expression.rs`:
```rust
use crate::operations::{Operation, Symbol, find_operation};
use crate::types::Answer;

// Expressions

// "lhs op rhs", with the operator already looked up in the table.
pub struct Expression {
    pub lhs: i32,
    pub operation: &'static Operation,
    pub rhs: i32,
}

impl Expression {
    // apply is a field holding a fn pointer, so it's called as (self.operation.apply)(...).
    pub fn evaluate(&self) -> Answer {
        (self.operation.apply)(self.lhs, self.rhs)
    }
}

// Parsing helpers

fn skip_spaces(text: &str) -> &str {
    text.trim_start_matches(' ')
}

// Reads an i32 from the front of text, and returns it with the rest of the text. Both &strs borrow the caller's text.
fn take_number(text: &str) -> Option<(i32, &str)> {
    let text = skip_spaces(text);

    // The number is an optional '-', then digits. usize::from(true) is 1.
    let sign = usize::from(text.starts_with('-'));
    let end = text[sign..].find(|c: char| !c.is_ascii_digit()).map_or(text.len(), |digits| sign + digits);

    // parse() fails on no digits at all, or on a number too big for an i32. It returns a Result, and ok() turns that into an Option.
    let (number, rest) = text.split_at(end);
    Some((number.parse().ok()?, rest))
}

// The parser

// Accepts "12 + 12", "-7*6" and so on: number, operator, number. Each ? returns None at the first thing that doesn't fit.
pub fn parse(text: &str) -> Option<Expression> {
    let (lhs, rest) = take_number(text)?;

    let rest = skip_spaces(rest);
    let symbol = Symbol::from_byte(*rest.as_bytes().first()?)?;
    let operation = find_operation(symbol)?;

    // The symbol is one ASCII byte, so the text after it starts at byte 1.
    let (rhs, rest) = take_number(&rest[1..])?;
    if !skip_spaces(rest).is_empty() {
        return None;
    }

    Some(Expression { lhs, operation, rhs })
}
```

## 4.5 Summaries: `src/stats.rs`

### Why
After the batch, we want the minimum, median, maximum and total of the results. We'd like to write that once, for any integer type and anything we can iterate over. Generic code is where associated types earn their keep: inside `summarize`, we don't know the types by name, so we have them chosen for us.

### How
- **Choosing a type at compile time:** a total of many `i32`s can pass `i32::MAX` even when every value fits, so it needs a wider type of the same signedness. The trait `Wide` maps each value type to its total type: `i32` to `i64`, `u32` to `u64`.
- **Bounds on the associated type:** `type Total: Copy + Default + From<Self> + Add<Output = Self::Total>;` lists what generic code may do with a total: copy it, start from its default (`0`), make one from a value (`i64::from(i32)`), and add two together. `Add<Output = ...>` is the trait behind `+`.
- **Supertraits:** `trait Wide: Copy + Ord` means every `Wide` type is also `Copy` and `Ord` (totally ordered, so it can be sorted).
- **`TotalOf<T>`** is the alias, as `UnsignedOf<T>` was. `Summary<T: Wide>` uses it for its `total` field.
- **`summarize<I>(values: I)`:**
  - **`where`** lists the bounds: `I: IntoIterator` accepts anything a `for` loop accepts, such as a `Vec`, an array, or an iterator chain. `I::Item: Wide` says the elements must be integers we know how to total. `I::Item` is itself an associated type: the element type of `I`.
  - **`collect()`** builds whatever collection the result's type names: here, a `Vec<I::Item>`. Sorting our own copy leaves the caller's data alone.
  - **`sort_unstable()`** sorts in place. "Unstable" means equal elements may be reordered, which for numbers can't be told apart, and it's faster.
  - **Empty input:** `first()?` returns `None` from `summarize` when there are no values, so the result is an `Option<Summary>`.
  - **`fold`** combines the values left to right, starting from the total type's default, zero. Each value is widened with `value.into()` (the other side of `From`) before it's added, so the sum is computed in `i64`.
  - **The median:** with an even count we take the upper of the two middle values.

### Code
`lecture_4/src/stats.rs`:
```rust
use std::ops::Add;

// Choosing a type at compile time

// The type a total of many Selfs is computed in. A total of many i32s can pass i32::MAX even when every value fits, so it needs a wider type of the same signedness.
pub trait Wide: Copy + Ord {
    type Total: Copy + Default + From<Self> + Add<Output = Self::Total>;
}

impl Wide for i32 {
    type Total = i64;
}

impl Wide for u32 {
    type Total = u64;
}

pub type TotalOf<T> = <T as Wide>::Total;

// The summary

pub struct Summary<T: Wide> {
    pub count: usize,
    pub min: T,
    pub median: T,
    pub max: T,
    pub total: TotalOf<T>,
}

// Accepts anything that can be iterated over and yields numbers: a Vec, an array, or a chain of iterator adapters. None if there are no values.
pub fn summarize<I>(values: I) -> Option<Summary<I::Item>>
where
    I: IntoIterator,
    I::Item: Wide,
{
    // Our own sorted copy, whatever we were given. collect() builds the collection the variable's type names.
    let mut sorted: Vec<I::Item> = values.into_iter().collect();
    sorted.sort_unstable();

    // first() is None for an empty Vec, and ? passes that on.
    let min = *sorted.first()?;
    let max = *sorted.last()?;

    // fold combines the values left to right, starting from the total type's zero. Each value is widened with From before it's added.
    let total = sorted.iter().fold(TotalOf::<I::Item>::default(), |sum, &value| sum + value.into());

    // With an even count we take the upper of the two middle values.
    Some(Summary { count: sorted.len(), min, median: sorted[sorted.len() / 2], max, total })
}
```

## 4.6 Callbacks: `src/batch.rs`

### Why
Running a batch is always the same loop: parse each input, then evaluate it. What to *do* with each result varies: print it, count it, collect it. We want to write the loop once, and let the caller supply the varying part as a function. The caller's function will need to update the caller's own variables, and that's more than a function pointer can do.

### How
- **Why not a fn pointer:** a closure that captures variables carries them with it. A `fn` pointer is just an address with nowhere to keep them, so only closures that capture nothing convert to one (4.3).
- **The closure traits:** every closure implements one or more of `Fn` (only reads its captures), `FnMut` (may change them) and `FnOnce` (may consume them, so can be called once). A closure that pushes to the caller's `Vec` is `FnMut`.
- **`dyn FnMut(...)`** is a *trait object*: any type that implements `FnMut` with that signature. Its size isn't known at compile time, so it's always used behind a pointer: `&mut dyn FnMut(...)`, or `Box<dyn FnMut(...)>`.
  - **The pointer is "fat":** it's two pointers, one to the closure's data and one to a table of its functions (a *vtable*), through which each call goes.
  - **The alternative:** a generic parameter, `handle: impl FnMut(...)`, would compile a separate `run_batch` for each closure type and call it directly. Trait objects compile once and pay an indirect call; we use one here to show them. Lecture 6 uses the generic kind.
- **`RowHandler<'a>`** names the type. A reference stored in a type needs a lifetime, so the alias has a lifetime parameter, and `RowHandler<'_>` in `run_batch` lets the compiler pick it.
- **`run_batch`** takes the inputs as a slice, so any contiguous sequence of `&str`s works.
  - **`for &input in inputs`:** iterating over a `&[&str]` yields `&&str`s, and the `&` pattern copies out the `&str`.
  - **`expression.as_ref()`** turns the `Option<Expression>` into an `Option<&Expression>`, so we can lend it without giving it away.
  - **`and_then`** calls the function only for `Some`, and passes `None` through. `Expression::evaluate` is the method, used as a function.

### Code
`lecture_4/src/batch.rs`:
```rust
use crate::expression::{Expression, parse};
use crate::types::Answer;

// Callbacks

// Called once per input with what parse and evaluate made of it. A &mut dyn FnMut is a reference to any closure with this signature, including one that captures and changes the caller's variables, which a fn pointer can't hold.
pub type RowHandler<'a> = &'a mut dyn FnMut(&str, Option<&Expression>, Answer);

// Running a batch

// Parses and evaluates every input, and leaves what to do with the results to the caller's handler.
pub fn run_batch(inputs: &[&str], handle: RowHandler<'_>) {
    for &input in inputs {
        let expression = parse(input);

        // and_then runs the closure only for Some, and passes None through.
        let value = expression.as_ref().and_then(Expression::evaluate);
        handle(input, expression.as_ref(), value);
    }
}
```

## 4.7 The program: `src/main.rs`

### Why
`main` puts the types on display, then runs the batch and reports on it. Almost every line is a call into the standard library, and the format strings do all of the layout.

### How
- **Measuring the integer types:**
  - **No common trait:** every integer type has `MIN` and `MAX` constants, but no standard trait collects them, so a generic function can't ask for `T::MIN`.
  - **A macro instead:** `print_limits!` takes a type (`$type:ty`), and writes the `println!` for it. `<$type>::MIN` is the constant on that type, and `stringify!($type)` turns the type's tokens back into text for the label.
- **A `u8` is always a number.** It prints as `65`; `char::from(65_u8)` is the character `'A'`. Neither type turns into the other by accident.
- **Aliases mix; newtypes don't:** `distance + time` adds `Meters` to `Seconds` without complaint, and the `Celsius` needs its `.0` to reach the number.
- **The price of `dyn`:** `size_of::<BinaryOp>()` is one pointer, 8 bytes. `size_of::<RowHandler>()` is two, 16: the data pointer and the vtable pointer.
- **`quoted`:** `{:?}` prints a string quoted and escaped, so the spaces in `"  8-3 "` show. Debug formatting of a string ignores widths, so we format it into a new `String` with `format!` first, and pad that.
- **The inputs** are an array of string literals. They live for the whole program, so slices of them never dangle. The list covers normal results, negative results, the edges of the `i32` range, overflow in addition, multiplication, division and powers, a division by zero, and two inputs `parse` rejects.
- **The handler closure:**
  - **Captures:** it uses `results` and `uses` from `main`, and changes them, so it borrows both mutably for as long as the closure is still used. The closure itself must be `let mut` to be called, since calling an `FnMut` changes it.
  - **Parameter types:** a closure's parameter types are usually inferred from where the closure is passed. This one is stored in a variable first, and a parameter like `&str` must be written out for the closure to accept text with any lifetime, as `RowHandler` requires.
  - **`let ... else`:** `let Some(expression) = expression else { ...; return; };` binds the value inside `Some`, or runs the `else` block, which must leave the closure. `return` in a closure returns from the closure, moving on to the next input, much like `continue` in a loop.
  - **`entry().or_insert(0)`:** `uses.entry(name)` finds the map's slot for the key, and `or_insert(0)` fills it with `0` the first time. `*... += 1` then counts.
  - **`count_ones()`** counts the 1 bits of the `u32` from `bits()`.
- **Format specifications** go after the `:` in a `{}`:
  - `{:<20}` left-aligns in 20 columns, and `{:>12}` right-aligns.
  - `{:#010x}` is hex with a `0x` prefix (`#`), padded with zeros (`0`) to 10 characters including the prefix.
  - `{:032b}` is binary padded to 32 digits.
  - `{:.2}` is a floating point number with two decimals.
- **The closure's size** is the size of what it captures: two references, 16 bytes. Every closure is its own type, sized for its captures.
- **The summary:**
  - **Borrows end:** after `run_batch`, the closure is never used again, so its borrows of `results` and `uses` are over, and we can read them.
  - **Counting failures:** `filter(...).count()` counts the empty results.
  - **A lazy chain:** an `Option` iterates over 0 or 1 values, so `results.iter().flatten()` skips the `None`s and unwraps the rest, and `copied()` turns each `&i32` into an `i32`. Nothing runs until something reads the chain.
  - **Reading twice:** an iterator is used up as it's read. The chain is cheap to clone before it starts, so `values.clone()` gives `println!` its own copy, and `summarize` gets the original.
  - **Printing collections:** `{:?}` prints a `Vec` as `[a, b, c]`, and a `BTreeMap` as `{key: value, ...}`.
  - **The total's type:** `let total: i64 = summary.total;` compiles only because the total of `i32`s is an `i64`.
  - **The mean:** `total as f64 / summary.count as f64` converts both sides with `as`, which may round very large values. Here they're far from that.

### Code
`lecture_4/src/main.rs`:
```rust
mod batch;
mod expression;
mod operations;
mod stats;
mod types;

use batch::{RowHandler, run_batch};
use expression::Expression;
use operations::OPERATIONS;
use stats::summarize;
use types::{Answer, BinaryOp, Bits, Celsius, Meters, Seconds, Tally};

use std::mem::{size_of, size_of_val};
use std::process::ExitCode;

// The types, measured

// A macro, because no trait in std has MIN and MAX: each integer type has its own constants. stringify! turns the type back into text.
macro_rules! print_limits {
    ($type:ty) => {
        println!("{:<6} {:>5}  {:>20}  {}", stringify!($type), size_of::<$type>(), <$type>::MIN, <$type>::MAX)
    };
}

fn print_types() {
    println!("{:<6} {:>5}  {:>20}  max", "type", "bytes", "min");
    print_limits!(i8);
    print_limits!(u8);
    print_limits!(i32);
    print_limits!(u32);
    print_limits!(i64);
    print_limits!(usize);

    // A u8 is always a number. Turning it into a character is a separate, explicit step.
    println!("\n65_u8 prints as {}, char::from(65_u8) as {}", 65_u8, char::from(65_u8));

    // Aliases mix freely; a newtype only does what we give it.
    let distance: Meters = 100.0;
    let time: Seconds = 9.58;
    let room = Celsius(21.5);
    println!("meters + seconds = {} (it compiles!), {room:?} holds {}", distance + time, room.0);

    // A fn pointer is one address. A &mut dyn FnMut is two: the closure's data, and a table of its functions.
    println!("\nsize_of: BinaryOp {}, RowHandler {}", size_of::<BinaryOp>(), size_of::<RowHandler>());

    print!("operations:");
    for operation in &OPERATIONS {
        print!(" {} {}", operation.symbol.as_char(), operation.name);
    }
    println!();
}

// Debug formatting ({:?}) quotes and escapes text, so the spaces in "  8-3 " show. It ignores widths, so we format first and pad the result.
fn quoted(input: &str) -> String {
    format!("{input:?}")
}

// The program

fn main() -> ExitCode {
    print_types();

    let inputs = [
        "12 + 12",
        "7 * 6",
        "100 / 7",
        "-100 % 7",
        "  8-3 ",
        "-1 + 0",
        "2147483647 + 0",
        "46340 * 46340",
        "3 ^ 4",
        "46341 * 46341",
        "2147483647 + 1",
        "-2147483648 / -1",
        "10 / 0",
        "2 ^ 31",
        "3 & 4",
        "99999999999 + 1",
    ];

    let mut results: Vec<Answer> = Vec::with_capacity(inputs.len());

    // Operation name -> times used. A BTreeMap keeps its keys sorted.
    let mut uses: Tally<&str> = Tally::new();

    println!("\n{:<20} {:>12}  {:<10}  {:<32}  ones", "expression", "decimal", "hex", "binary");

    // The closure borrows results and uses mutably, so it can fill them in. That capture is why it can't be a BinaryOp-style fn pointer.
    let mut handler = |input: &str, expression: Option<&Expression>, value: Answer| {
        // let-else: bind the value inside Some, or run the else block, which must leave the closure.
        let Some(expression) = expression else {
            println!("{:<20} {:>12}", quoted(input), "bad input");
            return;
        };

        // entry() finds the key's slot, inserting 0 the first time.
        *uses.entry(expression.operation.name).or_insert(0) += 1;
        results.push(value);

        let Some(value) = value else {
            println!("{:<20} {:>12}", quoted(input), "no result");
            return;
        };

        // count_ones counts the 1 bits. bits() gives the u32 with the same bits, so a negative value shows its two's complement pattern.
        let pattern = value.bits();
        println!("{:<20} {:>12}  {:#010x}  {:032b}  {:>4}", quoted(input), value, pattern, pattern, pattern.count_ones());
    };

    // A closure's size is the size of what it captures: here, two references.
    let handler_size = size_of_val(&handler);
    run_batch(&inputs, &mut handler);

    // The summary

    // The closure is done, so its borrows have ended and results and uses can be read again.
    let failed = results.iter().filter(|result| result.is_none()).count();

    // A lazy iterator: an Option iterates over 0 or 1 values, so flatten() skips the Nones and unwraps the rest. Nothing runs until something reads it.
    let values = results.iter().flatten().copied();

    println!("\n{} parsed, {failed} without a result; the handler closure is {handler_size} bytes", results.len());

    // An iterator is used up by reading it, so each reader gets its own clone of the (cheap, unstarted) chain.
    println!("values: {:?}", values.clone().collect::<Vec<i32>>());

    if let Some(summary) = summarize(values) {
        // This line compiles only if the total of i32s is an i64.
        let total: i64 = summary.total;
        println!("min {}, median {}, max {}", summary.min, summary.median, summary.max);
        println!("total {total}, mean {:.2}", total as f64 / summary.count as f64);
    }

    println!("uses:   {uses:?}");

    ExitCode::SUCCESS
}
```

## 4.8 Build and run

```bash
cargo run -p lecture_4
```

```text
type   bytes                   min  max
i8         1                  -128  127
u8         1                     0  255
i32        4           -2147483648  2147483647
u32        4                     0  4294967295
i64        8  -9223372036854775808  9223372036854775807
usize      8                     0  18446744073709551615

65_u8 prints as 65, char::from(65_u8) as A
meters + seconds = 109.58 (it compiles!), Celsius(21.5) holds 21.5

size_of: BinaryOp 8, RowHandler 16
operations: + add - subtract * multiply / divide % remainder ^ power

expression                decimal  hex         binary                            ones
"12 + 12"                      24  0x00000018  00000000000000000000000000011000     2
"7 * 6"                        42  0x0000002a  00000000000000000000000000101010     3
"100 / 7"                      14  0x0000000e  00000000000000000000000000001110     3
"-100 % 7"                     -2  0xfffffffe  11111111111111111111111111111110    31
"  8-3 "                        5  0x00000005  00000000000000000000000000000101     2
"-1 + 0"                       -1  0xffffffff  11111111111111111111111111111111    32
"2147483647 + 0"       2147483647  0x7fffffff  01111111111111111111111111111111    31
"46340 * 46340"        2147395600  0x7ffea810  01111111111111101010100000010000    18
"3 ^ 4"                        81  0x00000051  00000000000000000000000001010001     3
"46341 * 46341"         no result
"2147483647 + 1"        no result
"-2147483648 / -1"      no result
"10 / 0"                no result
"2 ^ 31"                no result
"3 & 4"                 bad input
"99999999999 + 1"       bad input

14 parsed, 5 without a result; the handler closure is 16 bytes
values: [24, 42, 14, -2, 5, -1, 2147483647, 2147395600, 81]
min -2, median 24, max 2147483647
total 4294879410, mean 477208823.33
uses:   {"add": 4, "divide": 3, "multiply": 3, "power": 2, "remainder": 1, "subtract": 1}
```

Reading the output:
- **The type table** gives every size and range. `usize` is 8 bytes on a 64-bit platform and 4 on a 32-bit one, which is why code that means "a size" or "an index" says `usize`.
- **`size_of`:** the `fn` pointer is 8 bytes, and the `dyn FnMut` reference 16: data and vtable.
- **Negative numbers** show their two's complement bits in the hex and binary columns: `-1` is all ones, and `-2` is all ones but the last. The `ones` column agrees: 32 and 31.
- **`-100 % 7`** is `-2`, not `5`. Integer division truncates toward zero, and `%` takes the sign of the left operand.
- **`46340 * 46340`** is the largest square that fits in an `i32`. `46341 * 46341` overflows, as do `2147483647 + 1`, `-2147483648 / -1` and `2 ^ 31`, and `10 / 0` is refused.
- **Bad input:** `&` isn't a `Symbol`, so `Symbol::from_byte` rejects `"3 & 4"`. `99999999999` doesn't fit in an `i32`, so `parse` fails. Neither expression is counted.
- **The total, `4294879410`,** is about twice the largest `i32`. It's correct because `TotalOf<i32>` is `i64`.
- **`uses`** lists the operations in alphabetical order, because a `BTreeMap` keeps its keys sorted.

In lecture 5 we go further into the standard library: errors that carry a reason (`Result` and our own error types), the traits that hook our types into the language, more collections, files, time, and threads.
