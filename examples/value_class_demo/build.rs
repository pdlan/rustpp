use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=point.rpp");
    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR")).join("generated.rs");
    if let Err(diagnostics) = rustpp_compiler::compile_file("point.rpp".as_ref(), &output) {
        for diagnostic in diagnostics {
            println!("cargo:warning={diagnostic}");
        }
        panic!("Rust++ compilation failed");
    }
}
