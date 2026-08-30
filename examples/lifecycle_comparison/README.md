# Stable-placement lifecycle comparisons

These four paired examples model systems that retain an object's final address
as opaque callback userdata or an intrusive identity. The in-process registries
never dereference the address; they stand in for a reactor, native C API,
intrusive scheduler, or plugin host.

| Scenario | Rust++ | Idiomatic Rust |
|---|---|---|
| Reactor connection | `rustpp-reactor` | `rust-reactor` |
| Native window callback | `rustpp-window` | `rust-window` |
| Intrusive scheduler task | `rustpp-scheduler` | `rust-scheduler` |
| Multi-interface plugin | `rustpp-plugin` | `rust-plugin` |

Run a pair with, for example:

```console
cargo run -p lifecycle-comparison --bin rustpp-reactor
cargo run -p lifecycle-comparison --bin rust-reactor
```

Each pair has identical observable behavior and asserts that publication is
removed during destruction.

## What Rust++ expresses

Every Rust++ class first constructs complete structural Data with `new`.
Generated owner construction then establishes the final allocation before
calling `init`. The examples publish the final address during `init` and remove
it during `deinit`, before structural fields drop. Multiple inheritance gives
the window, task, and plugin independently queryable runtime views through the
general `as?` operation.

Normal use remains concise:

```rust
let mut handler: Box<EventHandler> =
    construct Box<Connection>(registry.clone());
handler.on_ready();
drop(handler); // deinit unregisters before Data drops
```

## What idiomatic Rust requires

The Rust versions correctly use `Pin<Box<T>>` and `PhantomPinned` because an
external system retains an address derived from `T`. A private factory boxes
and pins first, publishes second, and uses a small audited `unsafe` operation
to fill the registration token without moving the object. Methods that mutate
the pinned object accept `Pin<&mut Self>`, and `Drop` removes publication.

This is sound, idiomatic low-level Rust, but it imposes visible machinery:

- callers and traits carry `Pin`;
- construction is a manually implemented two-phase factory;
- internal initialization and projection need audited `unsafe`;
- every independently discoverable sibling trait needs an `as_*` query hook;
- the correct publish/unpublish ordering is a library convention rather than a
  language-level class lifecycle.

Rust++ moves those recurring obligations into generated construction,
lifecycle-stage views, RTTI, owner rebinding, rollback guards, and deactivation.
Rust remains preferable when no retained identity exists; these examples are
specifically the cases where pinning and dynamic facets are inherent in the
problem.
