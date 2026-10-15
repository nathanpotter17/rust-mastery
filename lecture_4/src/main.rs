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
