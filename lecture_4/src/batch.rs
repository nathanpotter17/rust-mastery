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
