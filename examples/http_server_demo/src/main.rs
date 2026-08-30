// Cargo requires a Rust crate-root path. The application and its `main`
// function live entirely in `main.rpp`; this file only includes its output.
include!(concat!(env!("OUT_DIR"), "/main.rs"));
