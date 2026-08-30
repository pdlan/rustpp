# Rust++ bootstrap compiler

This workspace contains the first Rust++ bootstrap pipeline. It parses a
lossless `.rpp` syntax tree, lowers value classes to HIR, and emits ordinary
Rust.

Run the Cargo-integrated demo:

```sh
cargo run -p value-class-demo
```

Inspect generated Rust:

```sh
cargo run -p rustpp -- emit-rust examples/value_class_demo/point.rpp
```

Write generated Rust to a file:

```sh
cargo run -p rustpp -- emit-rust input.rpp --output output.rs
```

