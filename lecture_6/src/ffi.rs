// The raw C interface, exactly as bindgen wrote it. Every function is unsafe to call: the compiler can't check what C does with our pointers.

// The names are C's (MINI_SHAPE_BOX, not MiniShapeBox), and we don't use every binding, so these lints would only be noise here.
#![allow(non_camel_case_types, dead_code)]

// build.rs wrote the bindings into OUT_DIR, and include! pastes that file in here at compile time.
include!(concat!(env!("OUT_DIR"), "/mini.rs"));
