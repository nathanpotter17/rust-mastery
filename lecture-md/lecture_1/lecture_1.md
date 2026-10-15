# Lecture 1: Fundamentals

By the end of this lecture we'll have a vending machine: a program that takes coins, sells cola, chips and candy, gives change, refunds on request, and runs out of stock. It's a *state machine*: at any moment it's in one state (idle, collecting money, dispensing), and each event (a coin, a button) moves it to the next. Before the machine, the program takes a short tour of Rust's primitive types.

A state machine is the right first program for Rust, because it shows off what Rust does differently from C and C++ from the very start:
- **Enums that carry data.** A Rust `enum` is not a list of named integers: each variant can hold its own fields. `Event::Coin(25)` carries its value, and `State::Collecting { credit: 125 }` carries the money inserted so far.
- **Pattern matching.** `match` takes values apart, checks every possible case, and refuses to compile if we forget one.
- **Everything is an expression.** `if`, `match`, blocks and even `loop` produce values, so most variables are assigned once, and immutable.
- **Cargo.** One tool creates, builds, runs, formats, lints, documents and tests the project. We write unit tests for the machine on day one.

Along the way we cover the basics every later lecture stands on: primitive types and conversions, `let` and `let mut`, functions, `while`, `for` and `loop`, structs and methods, and modules.

The whole program is one file, `lecture_1/src/main.rs`. Each section below explains the next part of it; typed in order, the parts make up the complete file.

## 1.1 The toolchain and the project

### Why
Rust's tools come as one installation, and Cargo, its build tool and package manager, runs all of them. Every lecture in this course is its own small program, and we want them side by side in one repository, built the same way, without repeating the setup in each one.

### How
- **Installing:** `rustup` installs and updates Rust: the compiler `rustc`, Cargo, the formatter `rustfmt`, the linter Clippy, and the standard library's documentation. On Linux, install it from <https://rustup.rs>, then `rustup update` keeps it current.
- **The Cargo commands we'll use all course:**
  - `cargo new name`: a new *crate* (Rust's unit of compilation, a library or an executable), with a `Cargo.toml` and a `src/main.rs`.
  - `cargo build` and `cargo run`: compile, and compile then run. `--release` turns optimizations on.
  - `cargo check`: type-check without producing an executable. Much faster than a build, and the quickest way to see compiler errors.
  - `cargo fmt`: format every file the same way. `cargo clippy`: Clippy's lints, warnings about code that compiles but is likely wrong or unidiomatic.
  - `cargo test`: build and run the tests (1.5).
  - `cargo doc --open`: build HTML documentation from the code's `///` comments (1.2), and open it in a browser.
  - With several crates in one workspace, `-p name` picks one: `cargo run -p lecture_1`.
- **A workspace:** the root `Cargo.toml` makes the repository one Cargo *workspace*, with one crate per lecture.
  - `members = ["lecture_*"]` makes every directory named `lecture_*` a member, so a new lecture needs no edit here.
  - All members share one `Cargo.lock`, which records the exact version of every dependency, and one `target/` directory for build output.
  - `resolver = "3"` picks Cargo's newest rules for choosing dependency versions and features.
  - `[profile.release]` sets options for release builds: `lto = true` lets the optimizer work across crate boundaries, and `codegen-units = 1` compiles each crate as one unit, which optimizes better and builds slower.
- **`rustfmt.toml`** configures `cargo fmt`. We allow lines up to 140 characters, and `use_small_heuristics = "Max"` keeps a call on one line whenever it fits.
- **`.gitignore`** keeps build output out of git. Lecture 7 downloads third-party source code into `vendor/` directories, which stay out too.
- **The package manifest:** `cargo new lecture_1` writes `lecture_1/Cargo.toml`, which names the crate, gives it a version, and picks the *edition*. An edition is a set of language rules; 2024 is the newest. Crates of different editions work together, so the language can change without breaking old code. An executable crate starts at `src/main.rs`, and builds `target/debug/lecture_1`.
- **The editor:** rust-analyzer (the VS Code / VSCodium extension) understands Cargo workspaces directly, so it needs no configuration.

### Code
At the repo root:
`Cargo.toml`:
```toml
# One workspace, one crate per lecture: lecture_N/ builds an executable named lecture_N. The glob picks up every lecture_* directory, so a new lecture needs no edit here. All lectures share one Cargo.lock and one target/ directory.
[workspace]
resolver = "3"
members = ["lecture_*"]

# Release builds: optimize across crates (link-time optimization) as one unit.
[profile.release]
lto = true
codegen-units = 1
```

`rustfmt.toml`:
```toml
# cargo fmt: one long line reads better than a call wrapped across six.
max_width = 140
use_small_heuristics = "Max"
```

`.gitignore`:
```text
# Build output, for every lecture.
target/

# Third-party source code, fetched by the commands in each lecture.
lecture_*/vendor/
```

From the repo root:
```bash
cargo new lecture_1
```

`lecture_1/Cargo.toml`:
```toml
[package]
name = "lecture_1"
version = "0.1.0"
edition = "2024"
```

## 1.2 Primitive types

### Why
Every value in Rust has a type, and every type has a fixed representation: a size in bytes, and a meaning for each bit. Rust is strict about types: there are no implicit conversions between number types, and integer overflow is treated as a bug. A short tour prints sizes, bits and conversions, and the vending machine relies on all of them.

### How
- **`use`** brings a name into scope, so we can write `size_of` instead of `std::mem::size_of`. `std` is the standard library, which every Rust program has. (A few very common names, the *prelude*, are in scope everywhere; `size_of` joined them in Rust 1.80, so this `use` is optional, and does no harm.)
- **The primitive types:**
  - `bool`: `true` or `false`, one byte.
  - `char`: one Unicode character, such as `'A'` or `'é'`. It's 4 bytes, enough for any of them.
  - Integers: `u8`, `i32`, `u32`, `i64` and so on. `i` types are signed and `u` types unsigned, and the number is the size in bits. `size_of::<T>()` gives the size in bytes, and Rust fixes it on every platform.
  - `usize`: the unsigned integer the size of an address, 8 bytes on a 64-bit machine. Lengths and indexes are `usize`.
  - `f32` and `f64`: floating point numbers. `f64` has about 16 significant digits.
- **Representation:** every value is stored as bits, and a few conversions and methods show them, with no special tools:
  - `true as u8` is `1`, and `false as u8` is `0`.
  - `'A' as u32` is `65`: a `char` is stored as its Unicode *code point*, and `'é'` is 233.
  - `to_le_bytes()` returns an integer's bytes, lowest first ("little-endian", the order x86 and ARM use): 300 is `0x012c`, stored as `[44, 1, 0, 0]`, since `0x2c` is 44.
  - `-1_i32 as u32` keeps the bits and reads them as unsigned. Signed integers are stored in *two's complement*, where -1 is all ones, the largest `u32`.
  - `1.0_f32.to_bits()` is `0x3f800000`: a float is a sign bit, an exponent and a fraction (IEEE 754), nothing like the integer 1.
  - `{:?}` prints the byte array, and `{:#010x}` prints hex with a `0x` prefix (`#`), padded with zeros to 10 characters.
- **Literals have types:** a plain `7` is an `i32` and a plain `7.0` an `f64`, unless the code around them needs another type. A suffix chooses: `300_i32`, `1.0_f32`. `b'A'` is the *byte* 65, a `u8`; `'A'` is the `char`.
- **No implicit conversions:**
  - `7 / 2` is integer division, `3`. `7 / 2.0` doesn't compile at all: Rust never converts an integer to a float by itself.
  - `f64::from(7)` converts without loss. `From` exists only where nothing can be lost: `i32` to `f64` yes, `i64` to `f64` no.
  - `as` converts between any two number types, and may lose information: `3.99_f64 as i32` truncates to `3`.
  - A `char` isn't a number, so `'A' + 1` doesn't compile; `b'A' + 1` is the `u8` `66`, and `char::from` turns it back into `'B'`.
- **Overflow is a bug:** when the compiler can see the values, as in `zero - 1` here, its `arithmetic_overflow` lint, an error by default, rejects the overflow outright. When it can't, a debug build checks at run time, and an integer operation that goes out of range *panics*: the program stops with an error message. Release builds skip that check and wrap around, for speed. To wrap on purpose, we ask: `zero.wrapping_sub(1)` is the largest `u32`.
- **`let`** declares a variable. It can't be changed after it's initialized, unless it's declared `let mut`. Its type is inferred, from the initializer and from how the variable is used later, or written out: `let zero: u32 = 0;`. (Unlike C++'s `auto`, later uses count: in 1.5, a `quarters` that starts as `0` becomes a `u32` because it's returned as one.)
- **Functions:** `fn name() { ... }`. The last expression of a function, written without a `;`, is its return value; these three return nothing.
- **`println!`** prints a line. The `!` marks a *macro*: code that writes code, here so the format string is checked against the arguments while compiling. Each `{}` is replaced by the next argument, and `{zero}` by the variable of that name.

### Code
`lecture_1/src/main.rs`, part 1:
```rust
use std::mem::size_of;
use std::process::ExitCode;

// Primitive types

fn print_sizes() {
    // size_of::<T>() is a type's size in bytes. Rust fixes every one of these sizes on every platform, except usize, which is the size of an address.
    println!(
        "bool {}, char {}, u8 {}, i32 {}, u32 {}, i64 {}, usize {}, f64 {}",
        size_of::<bool>(),
        size_of::<char>(),
        size_of::<u8>(),
        size_of::<i32>(),
        size_of::<u32>(),
        size_of::<i64>(),
        size_of::<usize>(),
        size_of::<f64>()
    );
}

fn print_representation() {
    // Every value is stored as bits, and these conversions and methods show them.
    println!("true as u8 = {}, false as u8 = {}", true as u8, false as u8);
    println!("'A' as u32 = {}, 'é' as u32 = {}: a char is a Unicode code point", 'A' as u32, 'é' as u32);
    println!("300_i32.to_le_bytes() = {:?}: lowest byte first", 300_i32.to_le_bytes());
    println!("-1_i32 as u32 = {}: negative numbers are two's complement, all bits set", -1_i32 as u32);
    println!("1.0_f32.to_bits() = {:#010x}: sign, exponent and fraction, not 1", 1.0_f32.to_bits());
}

fn print_conversions() {
    // Rust never converts between number types by itself: 7 / 2.0 doesn't compile. f64::from converts without losing anything, and `as` converts even when something is lost.
    println!("7 / 2 = {}, f64::from(7) / 2.0 = {}, 3.99 as i32 = {}", 7 / 2, f64::from(7) / 2.0, 3.99_f64 as i32);

    // A char is not a number: 'A' + 1 doesn't compile. b'A' is the byte (u8) for 'A', and char::from turns a byte back into a char.
    println!("b'A' + 1 = {}, as a char: {}", b'A' + 1, char::from(b'A' + 1));

    // Going below zero in a u32 is a bug: the compiler rejects zero - 1 here, since it can see the values, and one it can't see would panic in a debug build. To wrap around, we have to ask.
    let zero: u32 = 0;
    println!("zero.wrapping_sub(1) = {}", zero.wrapping_sub(1));
}
```

## 1.3 Modelling with enums

### Why
A vending machine is described by a few kinds of things: the products, the events that happen to it, the states it can be in, and what each event did. Each is a fixed set of cases, and some cases carry data. In C we'd write an `enum` for the kind and a `struct` or `union` for the data, and keep the two in step by hand. A Rust `enum` is both.

### How
- **Comments:** `//` is a comment for the reader of the code. `///` is a *doc comment*: it documents the item below it, and `cargo doc` turns it into HTML, as the standard library's own documentation is made.
- **`mod machine { ... }`:** a module. Every name declared inside is really `machine::State`, `machine::Event` and so on, so they can't clash with names elsewhere. Everything in a module is private to it (and to the modules inside it) unless marked `pub`. A `pub enum`'s variants and their fields are public along with it, while a struct's fields each need their own `pub`.
- **A plain enum:** `Product` has three variants and no data, like a C enum, except that a `Product` isn't a number and doesn't turn into one by accident.
- **`#[derive(...)]`** asks the compiler to implement *traits* for a type. A trait is a set of methods a type can have; lecture 4 writes our own.
  - `Debug` lets `{:?}` print a value as the code would write it: `Cola`, `Coin(25)`, `Collecting { credit: 125 }`.
  - `Clone` and `Copy` let a value be copied like a number. Lecture 2 explains why types don't copy without this.
  - `PartialEq` makes `==` compare two values field by field. The tests need it.
- **`impl Product { ... }`** holds the type's associated items:
  - **An associated constant:** `Product::ALL` is an array of every product, `[Product; 3]`, for walking the menu with a `for` loop. Lecture 3 covers arrays.
  - **A method:** `price(self)` takes the product it's called on: `Product::Cola.price()`. Prices are whole cents, in a `u32`, so money never rounds.
  - **`const fn`** can also run at compile time.
- **`match`** compares a value against patterns, in order, and evaluates the arm that matches. The compiler checks that the arms cover every possible value: add a fourth product, and `price` stops compiling until we give it a price.
- **A const assert:** `const _: () = assert!(...);` evaluates the assert while compiling, so if a price weren't a multiple of 5 (the smallest coin), the build would fail. `_` is a constant with no name, kept only for the check. `is_multiple_of(5)` reads better than `% 5 == 0`, and Clippy suggests it.
- **Enums with data:**
  - **Tuple variants:** `Event::Coin(u32)` carries a number, and `Event::Select(Product)` a product. They're written `Event::Coin(25)`.
  - **Struct variants:** `State::Collecting { credit: u32 }` carries named fields. `credit` exists only while the machine is collecting, so an idle machine has no credit to get wrong, and no code can read a stale one.
  - **`Outcome`** mixes all three kinds of variant: `Restocked`, `SoldOut(Product)`, and `Vended { product, change }`.
  - **The tag:** an enum value records which variant it holds, usually in a hidden *tag* stored beside the largest variant's data. `match` reads it, so code can only reach a variant's fields after checking that it's that variant.

### Code
Part 2 opens `mod machine`; parts 3 and 4 continue inside it.

`lecture_1/src/main.rs`, part 2:
```rust
// Everything about the machine lives in module machine, so its names (State, Event, ...) can't clash with names from other code.
mod machine {
    // Products

    /// What the machine sells. #[derive] asks the compiler to write traits for us: Debug prints a value with {:?}, Clone and Copy let it copy like a number, and PartialEq compares with ==.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Product {
        Cola,
        Chips,
        Candy,
    }

    impl Product {
        /// Every product, for walking the menu with a for loop.
        pub const ALL: [Product; 3] = [Product::Cola, Product::Chips, Product::Candy];

        /// The price in cents. Money is counted in whole cents, so it never rounds.
        pub const fn price(self) -> u32 {
            match self {
                Product::Cola => 125,
                Product::Chips => 100,
                Product::Candy => 65,
            }
        }
    }

    // Checked while compiling: the machine only takes nickels and up, so every price must be a multiple of 5.
    const _: () = assert!(
        Product::Cola.price().is_multiple_of(5) && Product::Chips.price().is_multiple_of(5) && Product::Candy.price().is_multiple_of(5)
    );

    // Events, states and outcomes

    /// Something that happens to the machine. Variants can carry data: a Coin carries its value in cents, and a Select carries the product chosen.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Event {
        Coin(u32),
        Select(Product),
        Cancel,
        TakeItem,
        Service,
    }

    /// What the machine is doing. Collecting and Dispensing carry named fields, and the data exists only in those states: an idle machine has no credit to get wrong.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum State {
        Idle,
        Collecting { credit: u32 },
        Dispensing { product: Product, change: u32 },
    }

    /// What an event did, for the program to report.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Outcome {
        Accepted { credit: u32 },
        Rejected { coin: u32 },
        NeedMore { short: u32 },
        SoldOut(Product),
        Vended { product: Product, change: u32 },
        Refunded(u32),
        Collected(Product),
        Restocked,
        Ignored,
    }
```

## 1.4 The state machine

### Why
All the machine's behaviour is one function: given the current state and an event, produce the next state and an outcome. With enums and `match`, that function reads like the rules it implements, and the compiler checks that every combination of state and event has a rule.

### How
- **`struct Machine`** holds the state, the stock of each product, and the takings. Its fields have no `pub`, so only code inside `mod machine` can touch them; the rest of the program uses the methods.
- **No constructors:** a struct is created with a struct literal, `Machine { state: State::Idle, ... }`, and nothing else runs. By convention, a type offers an *associated function* named `new` that builds one. Every function in an `impl` is an associated function; one without a `self` parameter isn't a method, so it's called on the type, `Machine::new(2)`, rather than on a value.
- **Methods:** `&self` borrows the machine to read it, and `&mut self` borrows it to change it. `stock` reads; `take_one` and `handle` change. Lecture 2 explains borrowing.
- **`self.cola -= 1`** on a `u8`: if stock were already 0, that would be the overflow bug of 1.2, and a debug build would panic. The `SoldOut` arm makes sure it never gets here.
- **Matching on a pair:** `match (self.state, event)` builds a *tuple*, an unnamed group of values, and matches both at once. Each arm is a pattern for the pair. `self.state` can be copied out because `State` is `Copy`.
- **Patterns:**
  - `_` matches anything, and binds nothing: `(_, Event::Service)` is "any state, a Service event".
  - `State::Collecting { credit }` matches a variant and binds its field to a new variable, `credit`. `State::Dispensing { product, .. }` binds one field and ignores the rest.
  - `5 | 10 | 25 | 100` matches any of the four values, and `cents @ (...)` binds the value that matched to `cents`.
  - A plain name like `state` matches anything, and binds it.
- **Guards:** `if self.stock(product) == 0` after a pattern adds a condition. Arms are tried in order, and the first whose pattern *and* guard fit wins. So `Select` while collecting tries "sold out", then "not enough money", then the general case. The compiler doesn't look inside guards when it checks exhaustiveness, so that last `Select` arm has no guard: it's what covers every `(Collecting, Select)` pair.
- **Exhaustiveness:** the arms must cover every possible pair, or the code doesn't compile. The last arm names the remaining events, `Event::Select(_) | Event::Cancel | Event::TakeItem`, instead of using `_`: if we add a new event later, this `match` stops compiling until we decide what it does. A catch-all `_` would silently ignore it.
- **Everything is an expression:**
  - A `match` has a value: the value of the arm that ran. Here every arm's value is a `(State, Outcome)` pair, and `let (next, outcome) = match ...;` takes the pair apart.
  - A block `{ ... }` is an expression too: its statements run, and its last expression, with no `;`, is its value. The `Service` arm runs three assignments, then produces `(State::Idle, Outcome::Restocked)`.
  - So the next state is computed once, assigned once, and stored once, at the end.
- **Shadowing:** `let credit = credit + cents;` declares a *new* `credit`, computed from the old one, which it hides. The old one never changes, so neither needs to be `mut`.

### Code
`lecture_1/src/main.rs`, part 3:
```rust
    // The machine

    /// The machine: its state, its stock and the money it has taken.
    pub struct Machine {
        state: State,
        cola: u8,
        chips: u8,
        candy: u8,
        takings: u32,
    }

    impl Machine {
        /// A machine holding `stock` of each product. Rust has no constructors: new is a plain function, named by convention.
        pub fn new(stock: u8) -> Machine {
            Machine { state: State::Idle, cola: stock, chips: stock, candy: stock, takings: 0 }
        }

        pub fn state(&self) -> State {
            self.state
        }

        pub fn takings(&self) -> u32 {
            self.takings
        }

        pub fn stock(&self, product: Product) -> u8 {
            match product {
                Product::Cola => self.cola,
                Product::Chips => self.chips,
                Product::Candy => self.candy,
            }
        }

        // &mut self: this method changes the machine.
        fn take_one(&mut self, product: Product) {
            match product {
                Product::Cola => self.cola -= 1,
                Product::Chips => self.chips -= 1,
                Product::Candy => self.candy -= 1,
            }
        }

        /// Handles one event: moves to the next state, and says what happened.
        pub fn handle(&mut self, event: Event) -> Outcome {
            // One match on the (state, event) pair decides everything. Each arm's value is a (next state, outcome) pair, and the whole match is that value.
            let (next, outcome) = match (self.state, event) {
                // A technician can always reset and refill the machine. `_` never looks at the old state, so any credit in it is lost; only Cancel, which reads the credit out of State::Collecting, refunds it.
                (_, Event::Service) => {
                    self.cola = 5;
                    self.chips = 5;
                    self.candy = 5;
                    (State::Idle, Outcome::Restocked)
                }

                // Nickels, dimes, quarters and dollars. `cents @ pattern` names the value that matched.
                (State::Idle, Event::Coin(cents @ (5 | 10 | 25 | 100))) => {
                    (State::Collecting { credit: cents }, Outcome::Accepted { credit: cents })
                }
                (State::Collecting { credit }, Event::Coin(cents @ (5 | 10 | 25 | 100))) => {
                    // Shadowing: a new `credit`, computed from the old one.
                    let credit = credit + cents;
                    (State::Collecting { credit }, Outcome::Accepted { credit })
                }

                // Any other coin, or any coin while dispensing, drops straight into the return tray.
                (state, Event::Coin(coin)) => (state, Outcome::Rejected { coin }),

                // Guards (if ...) add conditions; the first arm whose pattern and guard both fit wins.
                (State::Collecting { credit }, Event::Select(product)) if self.stock(product) == 0 => {
                    (State::Collecting { credit }, Outcome::SoldOut(product))
                }
                (State::Collecting { credit }, Event::Select(product)) if credit < product.price() => {
                    (State::Collecting { credit }, Outcome::NeedMore { short: product.price() - credit })
                }
                (State::Collecting { credit }, Event::Select(product)) => {
                    self.take_one(product);
                    self.takings += product.price();
                    let change = credit - product.price();
                    (State::Dispensing { product, change }, Outcome::Vended { product, change })
                }

                (State::Collecting { credit }, Event::Cancel) => (State::Idle, Outcome::Refunded(credit)),
                (State::Dispensing { product, .. }, Event::TakeItem) => (State::Idle, Outcome::Collected(product)),

                // Everything else (selecting with no money in, taking an item that isn't there, ...) changes nothing.
                (state, Event::Select(_) | Event::Cancel | Event::TakeItem) => (state, Outcome::Ignored),
            };

            self.state = next;
            outcome
        }
    }
```

## 1.5 Change, and tests

### Why
Change is given in the fewest coins, which is easy to get subtly wrong. And a state machine has many paths through it: exactly the kind of code that needs tests. Rust's tests live next to the code they test, and Cargo runs them.

### How
- **`mut cents`:** a parameter is a variable like any other, and immutable unless declared `mut`. It's the function's own copy of the caller's value, so changing it changes nothing for the caller.
- **`while`** runs as long as its condition holds: subtract quarters while at least 25 cents are left. Integer division and remainder do the same for dimes and nickels in one step each. `cents % 10 / 5` is `(cents % 10) / 5`: `%` and `/` have the same precedence, and work left to right.
- **Returning a tuple:** `(u32, u32, u32)` is three values at once, unnamed. The caller takes them apart with `let (quarters, dimes, nickels) = ...`.
- **Tests:**
  - **`#[test]`** marks a function as a test. `cargo test` builds a separate executable that runs every test and reports each one. A test passes if it returns, and fails if it panics.
  - **`assert_eq!(a, b)`** panics if `a != b`, printing both values: that's what `Debug` and `PartialEq` on our enums are for.
  - **`#[cfg(test)] mod tests`:** the module is compiled only for `cargo test`, so tests cost nothing in the program itself. Being inside `mod machine`, the tests may use its private items too.
  - **`use super::*;`** brings in everything from the enclosing module, `machine`.
  - **`for _ in 0..4`:** `0..4` is a *range*, 0 up to but not including 4, and `_` ignores each value: "do this four times".
  - **What the tests check:** exact change, a rejected coin, a refund, a sold-out product, and the change-making. Each test makes its own machine, so they're independent, and the test executable runs them in parallel, on several threads.

### Code
`lecture_1/src/main.rs`, part 4:
```rust
    // Change

    /// Splits an amount into quarters, dimes and nickels, largest first. `mut cents`: the function's own copy of the argument, which it may change.
    pub fn coins_for(mut cents: u32) -> (u32, u32, u32) {
        let mut quarters = 0;
        while cents >= 25 {
            cents -= 25;
            quarters += 1;
        }

        // Division and remainder do the same in one step each.
        let dimes = cents / 10;
        let nickels = cents % 10 / 5;
        (quarters, dimes, nickels)
    }

    // Tests

    // cfg(test): this module is compiled only by `cargo test`, never into the program. Each #[test] function is run, and passes unless it panics.
    #[cfg(test)]
    mod tests {
        // super is the enclosing module: machine.
        use super::*;

        #[test]
        fn exact_change_vends_with_nothing_back() {
            let mut machine = Machine::new(1);
            for _ in 0..4 {
                machine.handle(Event::Coin(25));
            }
            assert_eq!(machine.handle(Event::Select(Product::Chips)), Outcome::Vended { product: Product::Chips, change: 0 });
            assert_eq!(machine.stock(Product::Chips), 0);
        }

        #[test]
        fn bad_coins_are_rejected_and_credit_kept() {
            let mut machine = Machine::new(1);
            machine.handle(Event::Coin(10));
            assert_eq!(machine.handle(Event::Coin(3)), Outcome::Rejected { coin: 3 });
            assert_eq!(machine.state(), State::Collecting { credit: 10 });
        }

        #[test]
        fn cancel_refunds_everything() {
            let mut machine = Machine::new(1);
            machine.handle(Event::Coin(100));
            machine.handle(Event::Coin(5));
            assert_eq!(machine.handle(Event::Cancel), Outcome::Refunded(105));
            assert_eq!(machine.state(), State::Idle);
        }

        #[test]
        fn sold_out_keeps_the_credit() {
            let mut machine = Machine::new(0);
            machine.handle(Event::Coin(100));
            assert_eq!(machine.handle(Event::Select(Product::Candy)), Outcome::SoldOut(Product::Candy));
            assert_eq!(machine.state(), State::Collecting { credit: 100 });
        }

        #[test]
        fn change_uses_the_fewest_coins() {
            assert_eq!(coins_for(0), (0, 0, 0));
            assert_eq!(coins_for(60), (2, 1, 0));
            assert_eq!(coins_for(95), (3, 2, 0));
        }
    }
}
```

## 1.6 The program

### Why
`main` runs the tour, prints the menu, then plays out a few customers' visits, printing every event and what it did.

### How
- **`use machine::{...}`** brings the names we need from the module into scope.
- **`dollars`** turns cents into an `f64` for printing only; all the arithmetic stays in whole cents. `f64::from` converts a `u32` exactly.
- **`press`** sends one event and prints it:
  - **`&mut Machine`:** the function borrows the caller's machine to change it, and the caller writes `&mut machine` to lend it. Lecture 2 explains why that's spelled out.
  - **`match` as an expression:** each arm is the text for one kind of outcome, and `let text = match ...;` keeps whichever arm ran.
  - **Literal patterns:** `Outcome::Vended { product, change: 0 }` matches only when there's no change, so it comes before the general `Vended` arm.
  - **`format!`** builds a `String` (owned text, lecture 3) the way `println!` prints. `{product:?}` prints the product with `Debug`. Derived `Debug` doesn't pad a value as a whole: it hands the width on to each field, so `{:<20?}` would print `Coin(100                 )`. So `format!("{event:?}")` makes it a `String` first, and `{:<20}` pads that.
- **`for product in Product::ALL`** takes each element of the array in turn.
- **Formats:** `{:.2}` prints two digits after the point, and `{:<6}` left-aligns in 6 characters.
- **`let mut machine`:** handling events changes the machine, so the variable must be `mut`. Without it, `&mut machine` wouldn't compile.
- **`loop` with a value:** `loop` runs until a `break`, and `break credit` ends it with a value, so the whole loop is an expression: `let credit = loop { ... };`. A `for` or `while` can't do that, since they may finish without reaching a `break`.
- **`if let ... && ...`:** `if let Outcome::Accepted { credit } = ...` runs its block only if the pattern matches, binding `credit` on the way. `&& credit >= ...` adds a condition that can use it. Chaining `let` patterns with `&&` is new in edition 2024.
- **`matches!(value, pattern)`** is `true` if the value matches the pattern: a one-arm `match` that only asks.
- **A refund, and a loss:** the sold-out scene ends with `Cancel`, which hands the dollar back: the `Cancel` arm reads `credit` out of `State::Collecting` before moving to `Idle`. The next scene inserts a quarter and sends `Service`, whose arm matches any state with `_` and moves to `Idle` without looking: the quarter only existed inside the old state, so it's gone. `let before = machine.state();` copies the state first (it's `Copy`), so the output can show what was lost. To make `Service` refund too, an arm `(State::Collecting { credit }, Event::Service)` above the general one would read the credit out the same way `Cancel` does.
- **The exit code:** `main` returns `ExitCode::SUCCESS`, which tells the shell the program succeeded. `main` could also return nothing; we'll return errors from it in lecture 5.
- **Order doesn't matter:** a function can be called from above its definition, since the compiler reads the whole crate before checking any call.

### Code
`lecture_1/src/main.rs`, part 5:
```rust
// Reporting

use machine::{Event, Machine, Outcome, Product, State};

/// Cents as dollars, for printing only: f64::from converts the u32 exactly.
fn dollars(cents: u32) -> f64 {
    f64::from(cents) / 100.0
}

/// Sends one event to the machine, and prints the event and what happened.
fn press(machine: &mut Machine, event: Event) -> Outcome {
    let outcome = machine.handle(event);

    // match is an expression too: each arm is the text for one kind of outcome. format! builds a String the way println! prints.
    let text = match outcome {
        Outcome::Accepted { credit } => format!("credit {:.2}", dollars(credit)),
        Outcome::Rejected { coin } => format!("{coin}-cent coin returned"),
        Outcome::NeedMore { short } => format!("insert {:.2} more", dollars(short)),
        Outcome::SoldOut(product) => format!("{product:?} is sold out"),
        Outcome::Vended { product, change: 0 } => format!("{product:?} dispensed"),
        Outcome::Vended { product, change } => {
            let (quarters, dimes, nickels) = machine::coins_for(change);
            format!("{product:?} dispensed, change {:.2}: {quarters}x25 {dimes}x10 {nickels}x5", dollars(change))
        }
        Outcome::Refunded(cents) => format!("{:.2} refunded", dollars(cents)),
        Outcome::Collected(product) => format!("{product:?} taken"),
        Outcome::Restocked => String::from("restocked"),
        Outcome::Ignored => String::from("ignored"),
    };

    println!("  {:<20} {text}", format!("{event:?}"));
    outcome
}

// The program

// Functions can be defined in any order: the compiler reads the whole crate before checking any call.
fn main() -> ExitCode {
    print_sizes();
    print_representation();
    print_conversions();
    println!();

    println!("Menu:");
    for product in Product::ALL {
        println!("  {:<6} {:.2}", format!("{product:?}"), dollars(product.price()));
    }

    // A machine with 2 of each product. It's `mut` because handling events changes it.
    let mut machine = Machine::new(2);

    println!("\nA cola, paid in two goes:");
    press(&mut machine, Event::Coin(100));
    press(&mut machine, Event::Select(Product::Cola));
    press(&mut machine, Event::Coin(3));
    press(&mut machine, Event::Coin(25));
    press(&mut machine, Event::Select(Product::Cola));
    press(&mut machine, Event::Coin(25));
    press(&mut machine, Event::TakeItem);

    println!("\nCandy, with change:");
    press(&mut machine, Event::Coin(100));
    press(&mut machine, Event::Coin(25));
    press(&mut machine, Event::Select(Product::Candy));
    press(&mut machine, Event::TakeItem);

    println!("\nChanging our mind:");
    press(&mut machine, Event::Select(Product::Chips));
    press(&mut machine, Event::Coin(10));
    press(&mut machine, Event::Coin(5));
    press(&mut machine, Event::Cancel);

    println!("\nThe last candy, then none, and a refund:");
    press(&mut machine, Event::Coin(100));
    press(&mut machine, Event::Select(Product::Candy));
    press(&mut machine, Event::TakeItem);
    press(&mut machine, Event::Coin(100));
    press(&mut machine, Event::Select(Product::Candy));
    press(&mut machine, Event::Cancel);

    // Service moves to Idle without reading the old state, so the credit that only existed inside State::Collecting is lost.
    println!("\nService with money inside:");
    press(&mut machine, Event::Coin(25));
    let before = machine.state();
    press(&mut machine, Event::Service);
    println!("  {before:?} became {:?}: the quarter is gone", machine.state());

    // loop runs until a break, and `break value` makes the loop itself an expression with that value.
    println!("\nQuarters until a cola:");
    let mut quarters = 0;
    let credit = loop {
        quarters += 1;
        if let Outcome::Accepted { credit } = machine.handle(Event::Coin(25))
            && credit >= Product::Cola.price()
        {
            break credit;
        }
    };
    println!("  {quarters} quarters make {:.2}", dollars(credit));
    press(&mut machine, Event::Select(Product::Cola));

    // A match can test the shape of a value: is the machine waiting for the cola to be taken?
    let waiting = matches!(machine.state(), State::Dispensing { .. });
    println!("\nWaiting to be emptied: {waiting}; takings {:.2}", dollars(machine.takings()));

    ExitCode::SUCCESS
}
```

## 1.7 Build, run and test

```bash
cargo run -p lecture_1
```

Or run `./build.bash`, choose `lecture_1`, and press Enter (option 4: build and run, Debug).

```text
bool 1, char 4, u8 1, i32 4, u32 4, i64 8, usize 8, f64 8
true as u8 = 1, false as u8 = 0
'A' as u32 = 65, 'é' as u32 = 233: a char is a Unicode code point
300_i32.to_le_bytes() = [44, 1, 0, 0]: lowest byte first
-1_i32 as u32 = 4294967295: negative numbers are two's complement, all bits set
1.0_f32.to_bits() = 0x3f800000: sign, exponent and fraction, not 1
7 / 2 = 3, f64::from(7) / 2.0 = 3.5, 3.99 as i32 = 3
b'A' + 1 = 66, as a char: B
zero.wrapping_sub(1) = 4294967295

Menu:
  Cola   1.25
  Chips  1.00
  Candy  0.65

A cola, paid in two goes:
  Coin(100)            credit 1.00
  Select(Cola)         insert 0.25 more
  Coin(3)              3-cent coin returned
  Coin(25)             credit 1.25
  Select(Cola)         Cola dispensed
  Coin(25)             25-cent coin returned
  TakeItem             Cola taken

Candy, with change:
  Coin(100)            credit 1.00
  Coin(25)             credit 1.25
  Select(Candy)        Candy dispensed, change 0.60: 2x25 1x10 0x5
  TakeItem             Candy taken

Changing our mind:
  Select(Chips)        ignored
  Coin(10)             credit 0.10
  Coin(5)              credit 0.15
  Cancel               0.15 refunded

The last candy, then none, and a refund:
  Coin(100)            credit 1.00
  Select(Candy)        Candy dispensed, change 0.35: 1x25 1x10 0x5
  TakeItem             Candy taken
  Coin(100)            credit 1.00
  Select(Candy)        Candy is sold out
  Cancel               1.00 refunded

Service with money inside:
  Coin(25)             credit 0.25
  Service              restocked
  Collecting { credit: 25 } became Idle: the quarter is gone

Quarters until a cola:
  5 quarters make 1.25
  Select(Cola)         Cola dispensed

Waiting to be emptied: true; takings 3.80
```

Then the tests. `--test-threads=1` runs them one at a time, so they print in the same order every time; after the `--`, arguments go to the test executable rather than to Cargo.

```bash
cargo test -p lecture_1 -- --test-threads=1
```

```text

running 5 tests
test machine::tests::bad_coins_are_rejected_and_credit_kept ... ok
test machine::tests::cancel_refunds_everything ... ok
test machine::tests::change_uses_the_fewest_coins ... ok
test machine::tests::exact_change_vends_with_nothing_back ... ok
test machine::tests::sold_out_keeps_the_credit ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Reading the output:
- **The types:** `char` is 4 bytes, enough for any Unicode character, and `usize` is 8 on a 64-bit machine. `-1` as a `u32` and `0 - 1` wrapped around both give 4294967295, the largest `u32`: all 32 bits set.
- **The first cola:** a dollar isn't enough, so the machine asks for 25 cents more and keeps the credit. The 3-cent coin is returned, and the credit is unchanged. The quarter while the cola is waiting to be taken is returned too: coins are only accepted while idle or collecting.
- **Change:** $1.25 for a 65-cent candy leaves 60 cents, two quarters and a dime.
- **Changing our mind:** selecting with no money in is ignored, and `Cancel` refunds everything inserted.
- **Sold out:** the machine started with 2 candies. After the second sells, the next `Select(Candy)` is refused, the dollar stays as credit, and `Cancel` refunds it.
- **Service with money inside:** the state was `Collecting { credit: 25 }`, and `Service` made it `Idle`. The quarter isn't refunded, and isn't in the takings either: it lived only in the old state.
- **The loop:** five quarters reach the cola's price, and the loop's value is the credit, $1.25. The machine then waits for the cola to be taken, and the takings are the four sales: 1.25 + 0.65 + 0.65 + 1.25 = 3.80.
- **The tests:** all five pass. Change a price, or break a rule in `handle`, and run `cargo test` again to see a failure: `assert_eq!` prints what it got and what it expected.

In lecture 2 we look at where values actually live: memory, ownership, and borrowing, the rules behind the `&` and `&mut` we've been writing.
