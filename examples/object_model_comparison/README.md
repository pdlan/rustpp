# Stateful object-view comparison

Four binaries implement the same observable toggle-button scenario. Each
prints:

```text
[ Dark mode: checked ]
clicks=1, same_address=true
```

Run all designs from the workspace root:

```console
cargo run -p object-model-comparison --bin rustpp-widgets
cargo run -p object-model-comparison --bin rust-query-widgets
cargo run -p object-model-comparison --bin rust-enum-widgets
cargo run -p object-model-comparison --bin rust-unified-widgets
```

## Rust++: independent stateful views

[`src/widgets.rpp`](src/widgets.rpp) gives `ToggleButton` three concrete,
stateful base views: `Widget`, `Clickable`, and `Accessible`. The example uses
language-level owner upcasting, `is`, mutable and immutable sibling
cross-casts, and an owning downcast. Every view reports the same complete
object address.

Tradeoff: this model needs Rust++'s more elaborate stable-object lifecycle,
view metadata, and generated runtime machinery.

## Rust query-interface design: open but boilerplate-heavy

[`src/bin/rust.rs`](src/bin/rust.rs) flattens the three facets into one struct.
Its erased `Widget` trait must know about every dynamically discoverable
interface through `as_clickable_mut` and `as_accessible`. Concrete owning
downcast requires another consuming `into_any` escape hatch.

Tradeoff: adding a runtime facet modifies the central erased trait and every
implementor, and facet/owner casts are handwritten rather than general.

## Rust enum design: simple but closed

[`src/bin/rust_enum.rs`](src/bin/rust_enum.rs) uses exhaustive enum matching.
This is clear, idiomatic Rust when all widget kinds are known centrally. An
inner `Box` is retained so consuming the outer enum and recovering a concrete
variant does not relocate the stable widget.

Tradeoff: adding a widget changes the central enum and every relevant match;
third-party widget kinds cannot extend it. Stable typed owner recovery also
costs an extra allocation/indirection in this representation.

## Rust unified-trait design: open types, closed capabilities

[`src/bin/rust_unified.rs`](src/bin/rust_unified.rs) places painting, clicking,
and accessibility directly on one `Widget` trait. Calls are concise and new
implementing types can be defined independently.

Tradeoff: independently reusable or optional facets disappear. Adding a new
capability changes the central trait and all implementations, and concrete
owner recovery still needs `Any`.

Rust traits and enums remain excellent when those tradeoffs fit the program.
The Rust++ design is strongest under the combined requirement of an open
hierarchy, independently stateful runtime facets, sibling cross-casting,
stable identity, and identity-preserving owner rebinding.
