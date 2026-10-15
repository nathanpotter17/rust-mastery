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
