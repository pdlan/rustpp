use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(()) => ExitCode::FAILURE,
    }
}

fn run() -> Result<(), ()> {
    let mut args = env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new("emit-rust")) {
        eprintln!(
            "usage: rustpp emit-rust <input.rpp> [--output <output.rs>] [--metadata-output <output.rppmeta>] [--abi-identity <crate/module>]"
        );
        return Err(());
    }
    let Some(input) = args.next().map(PathBuf::from) else {
        eprintln!("error: missing input file");
        return Err(());
    };
    let mut output = None;
    let mut metadata_output = None;
    let mut abi_identity = None;
    while let Some(argument) = args.next() {
        if argument == "--output" || argument == "-o" {
            let Some(path) = args.next() else {
                eprintln!("error: missing path after --output");
                return Err(());
            };
            output = Some(PathBuf::from(path));
        } else if argument == "--metadata-output" {
            let Some(path) = args.next() else {
                eprintln!("error: missing path after --metadata-output");
                return Err(());
            };
            metadata_output = Some(PathBuf::from(path));
        } else if argument == "--abi-identity" {
            let Some(identity) = args.next() else {
                eprintln!("error: missing identity after --abi-identity");
                return Err(());
            };
            abi_identity = Some(identity.to_string_lossy().into_owned());
        } else {
            eprintln!(
                "error: unexpected argument `{}`",
                argument.to_string_lossy()
            );
            return Err(());
        }
    }

    let source = fs::read_to_string(&input).map_err(|error| {
        eprintln!("error: failed to read {}: {error}", input.display());
    })?;
    let abi_identity = abi_identity.unwrap_or_else(|| {
        input
            .file_name()
            .unwrap_or(input.as_os_str())
            .to_string_lossy()
            .into_owned()
    });
    let compilation = rustpp_compiler::compile_source_with_identity(
        &input.display().to_string(),
        &abi_identity,
        &source,
    )
    .map_err(|diagnostics| {
        for diagnostic in diagnostics {
            eprintln!(
                "{}",
                diagnostic.render(&input.display().to_string(), &source)
            );
        }
    })?;
    if let Some(output) = output {
        fs::write(&output, &compilation.rust_source).map_err(|error| {
            eprintln!("error: failed to write {}: {error}", output.display());
        })?;
        let metadata_output = metadata_output.unwrap_or_else(|| output.with_extension("rppmeta"));
        fs::write(&metadata_output, &compilation.metadata).map_err(|error| {
            eprintln!(
                "error: failed to write {}: {error}",
                metadata_output.display()
            );
        })?;
    } else {
        print!("{}", compilation.rust_source);
        if let Some(metadata_output) = metadata_output {
            fs::write(&metadata_output, &compilation.metadata).map_err(|error| {
                eprintln!(
                    "error: failed to write {}: {error}",
                    metadata_output.display()
                );
            })?;
        }
    }
    Ok(())
}
