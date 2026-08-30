use std::env;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    for name in ["reactor", "window", "scheduler", "plugin"] {
        let source = format!("src/{name}.rpp");
        println!("cargo:rerun-if-changed={source}");
        if let Err(diagnostics) =
            rustpp_compiler::compile_file(source.as_ref(), &out.join(format!("{name}.rs")))
        {
            for diagnostic in diagnostics {
                println!("cargo:warning={source}: {diagnostic}");
            }
            panic!("Rust++ compilation failed for {source}");
        }
    }
}
