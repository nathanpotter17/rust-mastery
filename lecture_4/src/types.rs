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
