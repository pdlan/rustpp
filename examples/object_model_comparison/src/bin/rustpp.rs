// Cargo crate-root shim. The complete program, including `main`, is in
// `../widgets.rpp`.
include!(concat!(env!("OUT_DIR"), "/widgets.rs"));
