# Rust++ HTTP server demo

The complete application, including `fn main()`, is written in
[`src/main.rpp`](src/main.rpp). `src/main.rs` is only the one-line Cargo
inclusion shim for generated Rust.

Run the loopback integration demo from the workspace root:

```console
cargo run -p http-server-demo
```

The program binds an ephemeral localhost port, launches a standard-library TCP
client, serves one HTTP/1.1 request, checks the response, and exits. It
demonstrates a movable RAII `Fd` value class, a stable `Connection` class,
inline ownership, `BaseServer`/`TcpServer`/`HttpServer` inheritance, and virtual
request dispatch through a base owner view.
