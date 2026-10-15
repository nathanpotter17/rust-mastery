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
