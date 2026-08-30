use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/widgets.rpp");
    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR")).join("widgets.rs");
    if let Err(diagnostics) = rustpp_compiler::compile_file("src/widgets.rpp".as_ref(), &output) {
        for diagnostic in diagnostics {
            println!("cargo:warning={diagnostic}");
        }
        panic!("Rust++ compilation failed");
    }
}
