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
