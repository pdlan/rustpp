# Rust++ Class / Value-Class Object Model

**Status:** Revised design draft  
**Current implementation target:** Rust source generation as a bootstrap/reference backend  
**Long-term implementation target:** a native Rust++ compiler, native Rust++ object ABI, and native Rust++ ownership/runtime types  
**Interop target:** generated bridge crates that expose value classes naturally and ordinary classes only through opaque class-owner/view façade types  
**Scope:** values, place-bound objects, construction, destruction, inheritance, polymorphic views, ownership, RTTI/downcast, lowering, ABI evolution, and Rust interoperability

> **Core principle:** Rust is the current safety-enforcing bootstrap backend, not the source-language object philosophy and not the permanent Rust++ ABI.

Rust++ deliberately keeps the safety laws that matter for memory safety while rejecting the assumption that Rust's current value-centric object model is itself one of those laws. Ownership, capability provenance, exclusive mutation, lifetime/epoch validity, and linear destruction obligations remain fundamental. Whether every user-defined type must be a movable value does not.

The semantic model must therefore remain independent of Rust trait-object layout, Rust vtable layout, and the concrete representation of `std::boxed::Box`, `Rc`, or `Arc`. The current Rust lowering exists to validate the model, reuse rustc's safety machinery, and provide an implementation path while the native compiler and ABI are developed.

Rust++ deliberately separates Rust-compatible affine values from stable-identity objects. A `value class` is a value and follows the value semantics that the bootstrap backend maps directly to Rust. An ordinary `class` is **not a value**. It is a place/object type whose lifetime is activated over a complete data representation at a stable place.

The design therefore starts from six statements:

```text
A value class is its data.
An ordinary class is not a value; it denotes a place-bound object kind.
An ordinary class object is activated over its data at a stable place.
Inheritance is subtyping between object views, not between values.
Movement never invokes user-defined move code.
Rust-style value generics range over values; class objects participate through place-aware operations or movable view/owner capabilities.
```

A useful summary of the language philosophy is: preserve Rust's rules about who may access an object, for how long, and with what capability; do not treat Rust's preferred surface ontology as a memory-safety requirement.

The backend is allowed to generate a small amount of trusted `unsafe`, but the ordinary source-language model must remain safe without exposing raw construction or lifecycle primitives.

---

# Part I — Semantic Foundations

## 1. Values, Data, and Objects

Rust++ distinguishes two kinds of user-defined aggregate.

### 1.1 Value classes

For every value class `V`:

```text
V ≡ VData
```

There is one lifetime: the lifetime of the value representation.

```text
∅ --new--> V --drop--> ∅
```

Moving `V` means moving its representation according to ordinary Rust affine semantics.

### 1.2 Ordinary classes

An ordinary class is not a non-movable value. It is **not a value at all** in the Rust++ source type model. A class name such as `T` denotes an exact object/place kind: a stable location in which `T` object semantics may be active.

This distinction is semantic, not merely an optimization or an `Unpin`-like restriction. Source constructs that require an ordinary transferable value do not accept a bare ordinary class type. A direct binding such as `let t: T = construct T(...)` names an exact object place; it does not create a movable first-class `T` value.

For every ordinary class `T`, the structural data and the object lifetime are distinct:

```text
∅ --new--> TData
TData --init--> T object
T object --deinit--> TData
TData --drop--> ∅
```

`init` and `deinit` are in-place lifetime transitions. They do not move the data.

An ordinary class object exists only while a complete valid `TData` at a stable place is activated as that class stage.

The bare class type therefore does not participate in ordinary value relocation, value tuples, value enums, or value containers. Instead, movable values such as `&T`, `&mut T`, `Box<T>`, `Rc<T>`, and `Arc<T>` carry borrow or ownership capabilities referring to the stable object. Moving one of those values moves the capability representation, not the object.

---

## 2. Design Goals

The class model aims to provide:

- Rust-like value semantics for value classes;
- stable object identity for ordinary classes;
- inline ordinary-class composition without moving live subobjects;
- concrete inheritance without slicing;
- borrowed and owning polymorphic class views;
- explicit virtual methods;
- multiple concrete inheritance without repeated concrete bases;
- deterministic construction and destruction;
- fallible construction without exceptions;
- lifecycle-aware dispatch during construction and destruction;
- RTTI, downcast, and sibling cross-cast;
- real Rust trait interoperability;
- a bootstrap lowering that can reuse Rust `Box`, `Rc`, `Arc`, references, and trait-object machinery where useful;
- a source ownership model that does not semantically depend on those Rust library representations;
- a migration path to a native Rust++ compiler and native class/view/owner ABI without changing source object semantics;
- a small compiler-controlled unsafe boundary in the Rust bootstrap backend;
- no dependency on the C++ ABI or on an object-embedded C++-style vptr.

Non-goals for the core model include:

- C++ binary compatibility;
- virtual inheritance;
- object slicing;
- user-defined move constructors;
- generic virtual methods;
- by-value virtual receivers;
- a safe general-purpose placement-new API for handwritten Rust;
- permanent dependence on Rust trait-object, `Box`, `Rc`, `Arc`, or vtable ABI details.

### 2.1 Backend Strategy and Long-Term ABI Direction

Rust++ is intentionally specified above any particular backend ABI.

The implementation plan has two major stages.

**Bootstrap/reference backend.** The current compiler may generate Rust and deliberately reuse rustc facilities:

```text
value representation       -> ordinary Rust structs where possible
borrow checking            -> Rust references and rustc borrow checking
virtual class views        -> hidden Rust dyn traits
stable unique ownership    -> Rust Box
shared ownership           -> Rust Rc / Arc
RTTI/cross-cast            -> compiler metadata plus generated thunks
RAII/unwind cleanup        -> Rust Drop and guards
```

This representation is a proving and implementation strategy. It is not the language ABI. In particular, source `Box<C>`, `Rc<C>`, and `Arc<C>` for ordinary classes are semantic owner categories whose current lowering happens to use the corresponding Rust standard-library owners.

**Native backend.** The long-term compiler is expected to own the corresponding mechanisms directly:

```text
class-view ABI             -> Rust++-defined fat view / metadata ABI
virtual dispatch           -> Rust++-defined vtables or equivalent metadata
RTTI                       -> Rust++ TypeDescriptor ABI
stable unique owner        -> Rust++ native Box-like owner
shared owner               -> Rust++ native Rc/Arc-like control blocks
construction/destruction   -> native lifecycle glue
stack/inline placement     -> native compiler placement machinery
```

The source semantics in this document must survive that transition unchanged. A program must not be able to observe whether a class view is currently backed by a Rust trait object or by a native Rust++ view record.

For Rust interoperability in the native phase, ordinary classes are not exported as `Box<dyn Trait>`, `Rc<dyn Trait>`, or `Arc<dyn Trait>`. A generated bridge crate exposes opaque façade types such as:

```rust
ClassBox<C>
ClassRc<C>
ClassArc<C>
ClassRef<'a, C>
ClassMut<'a, C>
```

or class-specific aliases/wrappers such as `WidgetBox`. These types own or borrow a native Rust++ object through stable ABI shims and do not expose the native object layout, native vtable layout, or native control block to handwritten Rust.

Value classes remain deliberately different: when their representation is Rust-compatible, the bridge crate may continue to expose them as ordinary Rust values with `V::new(...)` and ordinary Rust value ownership.

---

## 3. Core Surface Boundary

A `value class` supports:

- fields and methods;
- constructors;
- structural `drop`;
- ordinary Rust moves;
- explicit clone/duplication protocols;
- generics;
- real Rust trait implementations.

A `value class` does not support:

- independent `init`/`deinit` object lifetime;
- concrete class inheritance;
- class virtual methods;
- stable identity as part of the type contract.

An ordinary `class` supports:

- `new` and `init` constructor phases;
- `deinit` and structural `drop` destructor phases;
- stable-place identity;
- inline ordinary-class subobjects;
- concrete inheritance;
- borrowed class views;
- owning class views through stable owner types;
- virtual dispatch;
- access control;
- RTTI and class-view casts.

An ordinary `class` does **not** support being used as an ordinary transferable value merely because its class name is a source type. In particular, the core language does not implicitly make `C` a valid element or payload type for Rust-style value-generic containers and sum/product types.

### 3.1 Value Kinds, Class Kinds, and Rust-Style Generics

The source type model distinguishes at least two semantic roles:

```text
Value kind
    a transferable affine value whose representation may move

Class/object-place kind
    an exact stable object place whose live object may not relocate
```

A `value class V` belongs to the value kind. A bare ordinary `class C` belongs to the class/object-place kind.

Rust-style value generics range over values. Unless a generic construct is explicitly defined as placement-aware or class-aware, substituting a bare ordinary class for its value parameter is ill-formed. Conceptually:

```text
Vec<T>       requires T to be a Value
Option<T>    requires T to be a Value
Result<T,E>  requires T and E to be Values
(T, U)       ordinary tuple components are Values
```

Therefore:

```rust
Vec<Point>               // valid if Point is a value class
Vec<Box<Widget>>         // valid: the Box owner is a movable value
Vec<Rc<Widget>>          // valid
Vec<Arc<Widget>>         // valid
Vec<&Widget>             // valid when the borrow lifetime permits

Vec<Widget>              // invalid: Widget is not a value
Option<Widget>           // invalid as an ordinary value Option
Result<Widget, Error>    // invalid as an ordinary value Result
(Widget, i32)            // invalid as an ordinary movable tuple
```

`Vec<Widget>` is rejected as a type-category error, not merely because some `Vec` methods happen to relocate elements. `Vec` is a value container whose representation contract permits element relocation during growth, insertion, removal, compaction, and related operations. A live `Widget` is a stable-place object, so it is not an admissible element value in the first place.

Owner and view constructors cross the boundary back into the value world:

```text
C           : Class/ObjectPlace
&C          : Value capability
&mut C      : Value capability
Box<C>      : Value ownership capability
Rc<C>       : Value shared-ownership capability
Arc<C>      : Value shared-ownership capability
```

`Box<C>` does not make the `C` object movable. It creates a movable owner value whose referent remains at one stable object place. The same principle applies to `Rc<C>`, `Arc<C>`, and borrowed views.

The language may also support class-kind generic parameters for class-aware APIs, conceptually:

```rust
fn inspect<C: Class>(x: &C) { ... }
fn own<C: Class>(x: Box<C>) { ... }
```

Such a parameter names a class/object kind. It does not authorize by-value use such as `fn bad<C: Class>(x: C)` unless a future explicitly placement-aware parameter mechanism defines that syntax. Exact surface syntax for kind bounds may evolve; the semantic separation is normative.

---

## 4. Exact Object Type and Class View

A source class name used as a direct object place denotes an exact object type.

```rust
D
```

means an exact `D` object place.

A borrowed class view:

```rust
&D
&mut D
```

is different. It is a semantic view rooted in a complete object identity and may refer to a descendant object through its `D` view.

For example:

```text
class D : A
```

permits:

```text
&D       -> &A
&mut D   -> &mut A
Box<D>   -> Box<A>
Rc<D>    -> Rc<A>
Arc<D>   -> Arc<A>
```

but does not permit:

```text
D -> A
```

No base value is materialized.

An ordinary class name is therefore a place/object type rather than a transferable by-value type. More strongly: a bare ordinary class is outside the ordinary value universe. The core language does not permit ordinary-class function parameters or return values by value, and ordinary Rust-style value-generic positions reject it. APIs use borrowed views, stable owner/view values, or explicitly placement-aware language constructs instead. A future caller-provided return-slot construction feature could be added as placement syntax without changing this rule.

---

# Part II — Construction and Placement

## 5. Value-Class Construction

A value class uses ordinary value-construction syntax and should look Rust-like.

```rust
value class Point {
    x: f64,
    y: f64,

    constructor(x: f64, y: f64) {
        new {
            x,
            y,
        }
    }
}
```

Source use:

```rust
let p = Point::new(1.0, 2.0);
let q = p;
let boxed = Box::new(Point::new(3.0, 4.0));
```

`Point::new(...)` produces an ordinary movable `Point` value.

A value-class constructor has no `init` phase.

---

## 6. Ordinary-Class Constructor Declaration

An ordinary class constructor has two semantic phases:

```rust
class Widget {
    title: String,

    constructor(title: String) {
        new {
            title,
        }

        init {
            self.register();
        }
    }
}
```

`new` constructs structural data. There is no class `self` during `new`.

`init` begins the object lifetime after the complete data has reached its final place.

Constructor parameters follow normal Rust ownership rules. A parameter may be consumed by `new`, retained in an activation frame for `init`, borrowed, or moved into a field. The phase boundary does not implicitly clone arguments.

---

## 7. `construct` Is the Ordinary-Class Construction Operator

Ordinary classes deliberately do not use value-construction syntax.

The source-language construction operator is:

```text
construct <construction-target>(args...)
```

Examples:

```rust
let d = construct D(...);
let d = construct Box<D>(...);
let d = construct Rc<D>(...);
let d = construct Arc<D>(...);
```

The distinction is intentional:

```text
Point::new(...)     produces a movable value
construct D(...)    creates a place-bound object lifetime
```

`construct` is not a function call that first creates a movable `D` value. It is a placement-aware object-construction operation.

---

## 8. Construction Targets

The core construction targets are:

```text
D           direct final object place
Box<D>      Box-owned stable allocation
Rc<D>       Rc-owned stable allocation
Arc<D>      Arc-owned stable allocation
```

`construct D(...)` is called **direct-place construction**, not specifically stack construction. In a local binding its final place is normally stack storage; inside an enclosing ordinary-class `new` it may denote an inline subobject data destination.

`construct Box<D>(...)`, `construct Rc<D>(...)`, and `construct Arc<D>(...)` select the stable owner before activation.

The complete object class is always the class written inside the construction target. The class must be concrete and constructible; an abstract class cannot be used as the complete construction class.

```rust
let a: Box<A> = construct Box<D>(...);
```

means:

```text
complete object class = D
placement owner       = Box
final exposed view    = A
```

The final `D -> A` step is a class-view upcast. It does not affect construction or allocation.

`construct Box<A>(...)` constructs an exact `A`; it does not implicitly choose an unknown descendant.

---

## 9. Direct Places Are Exact

A direct object binding is exact:

```rust
let d: D = construct D(...);      // valid
```

This is invalid:

```rust
let a: A = construct D(...);      // invalid
```

because `A` is an exact direct object place, not an existential polymorphic storage container.

Polymorphism is expressed through views and owner/view types:

```rust
let d = construct D(...);
let a: &A = &d;

let a: Box<A> = construct Box<D>(...);
```

---

## 10. Final Placement Rule

`init` runs only after complete data has reached its final stable address.

Direct local conceptual lowering:

```text
D.new(...)
    ↓
(DData, ActivationFrame)
    ↓
move DData into final direct slot
    ↓
activate bases, fields, and D
```

Box conceptual lowering:

```text
D.new(...)
    ↓
(DData, ActivationFrame)
    ↓
Box<DData>
    ↓ final allocation address established
activate
    ↓
Box<DStorage>
    ↓ class-view unsizing
Box<D-view>
```

No source object observes a temporary live-object address.

---

## 11. Nested Ordinary-Class Fields

An ordinary class may contain an ordinary class inline.

```rust
class Parent {
    child: Child,

    constructor() {
        new {
            child: construct Child(),
        }

        init {}
    }
}
```

The nested `construct Child()` inside `Parent.new` does not create a live temporary `Child`.

It contributes:

```text
ChildData
ChildActivationFrame
```

to the enclosing construction.

Conceptually:

```text
Child.new
    ↓
(ChildData, ChildActivationFrame)

Parent.new
    ↓
ParentData { child: ChildData, ... }
ParentActivationFrame { child: ChildActivationFrame, ... }
    ↓
move ParentData to final place
    ↓
Child.init
    ↓
Parent.init
```

This follows directly from the Data/Object lifetime split.

---

## 12. Activation Frames

A constructor may need values after the `new` phase.

Therefore the backend models ordinary-class construction as producing:

```text
(Data, ActivationFrame)
```

rather than only `Data`.

Example:

```rust
class BufferView {
    buffer: Vec<u8>,

    constructor(buffer: Vec<u8>, initial: usize) {
        new {
            buffer,
        }

        init {
            self.set_position(initial);
        }
    }
}
```

Conceptual backend state:

```rust
struct __BufferViewActivationFrame {
    initial: usize,
}
```

The activation frame:

- is an ordinary movable Rust value;
- is not part of the final object representation;
- may contain constructor parameters retained for `init`;
- may recursively contain activation frames for inline class fields and bases;
- is destroyed after successful activation or rollback.

---

## 13. Activation Order

For an ordinary class, activation after final placement occurs recursively in this fixed order:

1. direct concrete bases, in declaration order;
2. ordinary-class fields, in declaration order;
3. the current class's own `init`.

Value-class fields have no activation phase.

For:

```text
C
├── base A
│   └── field X
└── field B
```

activation is:

```text
X.init
A.init
B.init
C.init
```

The order is structural and is not determined by arbitrary initializer evaluation order.

---

## 14. Activation State Model

A useful abstract state machine is:

```text
InactiveData
    ↓ begin stage C init
Activating<C>
    ↓ init succeeds
Active<C>
```

For a hierarchy:

```text
D : B : A
```

construction proceeds conceptually through:

```text
DData
  ↓
Activating<A>
  ↓ success
Active<A>
  ↓
Activating<B>
  ↓ success
Active<B>
  ↓
Activating<D>
  ↓ success
Live<D>
```

A stage becomes a deinitialization obligation only after that stage's `init` succeeds.

---

## 15. Constructor Failure

Successful complete Data construction creates exactly one structural-drop obligation.

Successful class-stage activation creates exactly one matching `deinit` obligation.

```text
successful new  => one structural-drop obligation
successful init => one deinit obligation
failed init     => no deinit obligation for that failed stage
```

If:

```text
A.init succeeds
B.init succeeds
D.init fails
```

rollback is:

```text
B.deinit
A.deinit
complete DData structural drop
release final storage
```

`D.deinit` does not run because `D.init` did not succeed.

Before complete `TData` exists, ordinary Rust local cleanup is used.

Constructor errors must not contain borrows into Data that will be destroyed by failure cleanup unless ordinary lifetime analysis proves the borrow remains valid independently.

---

# Part III — Destruction

## 16. `deinit` and `drop`

Destruction is the dual of construction:

```text
new    begins Data lifetime
init   begins Object lifetime

deinit ends Object lifetime
drop   ends Data lifetime
```

An ordinary class may declare:

```rust
destructor {
    deinit {
        ...
    }

    drop {
        ...
    }
}
```

A value class may declare only structural `drop`.

---

## 17. `deinit` Ends Object Semantics

At entry to a class stage's `deinit`:

- that class-stage object still exists;
- `self` exists;
- methods may be called;
- lifecycle-limited class views may be formed;
- virtual dispatch follows destruction-stage rules;
- structural data still exists.

When `deinit` completes, that stage's object lifetime ends while the underlying Data remains.

```text
before:
    object stage exists
    Data exists

after:
    object stage does not exist
    Data exists
```

`deinit` cannot return an expected failure that cancels destruction.

---

## 18. `drop` Is Structural

A `drop` body operates on Data, not on a class object.

Inside structural `drop`:

- there is no class `self`;
- class views cannot be formed from already-deactivated Data;
- virtual dispatch on the destroyed object is unavailable;
- `init` invariants must not be assumed;
- structural fields may be read or mutated for cleanup;
- value-class fields are ordinary values;
- ordinary Rust resources use ordinary Rust destruction.

By-value extraction from a structural field is forbidden unless an explicit operation transfers the corresponding drop obligation.

---

## 19. Two-Pass Object-Tree Destruction

For a fully live ordinary object tree, destruction conceptually occurs in two passes.

### Deactivation pass

```text
current class .deinit
ordinary-class fields, reverse declaration order
concrete bases, reverse declaration order
```

After this pass no ordinary-class object in the subtree remains alive.

### Structural-drop pass

```text
current Data.drop body
fields in reverse source declaration order
bases in reverse source declaration order
```

The structural pass destroys Data only; it must not run `deinit` again.

---

## 20. Panic and Unwinding During Destruction

When Rust unwinding is enabled, generated lifecycle glue uses RAII cleanup guards so that remaining deinitialization obligations are executed during unwinding as far as Rust's panic model permits.

Conceptually:

```text
run current deinit
finally run remaining child/base deinits
finally structurally drop Data
```

If another `deinit` panics while the thread is already unwinding from a panic, normal Rust double-panic behavior applies, which may abort the process.

The backend must still never intentionally run a successful stage's `deinit` twice or structurally drop owned Data twice.

---

# Part IV — Movement, Ownership, and Containment

## 21. Move Is Not a Constructor

Rust++ has no user-defined move constructor.

For a value class:

```text
move(V) == move(complete V representation)
```

No hidden user code runs merely because a value changes location.

A moved-from variable no longer contains a source-language value, following ordinary Rust affine semantics.

---

## 22. Ordinary Classes Are Place-Bound

Once an ordinary-class stage is activated, its object identity is tied to the stable Data address.

A live object may not be relocated by value.

Before activation, `TData` remains an ordinary movable structural value.

Therefore:

```text
move TData before activation     valid
move live T object by value      invalid
move an owner handle             valid
```

---

## 23. Stable Owner Types

The source language defines stable owner categories with the familiar spellings:

```text
Box<T>
Rc<T>
Arc<T>
```

when `T` is an ordinary class view. Their semantic contracts are part of Rust++, not permanently part of the Rust standard library ABI.

In the current Rust bootstrap backend they deliberately lower to Rust `Box`, `Rc`, and `Arc` where possible. In the future native backend they may use Rust++-defined allocation headers, reference-count control blocks, metadata pointers, and destruction entry points while preserving the same source behavior.

Moving `Box<T>`, cloning `Rc<T>`, or cloning `Arc<T>` does not move the underlying object.

An owning polymorphic class view means:

> the owner controls one complete stable object allocation while exposing a particular class view into that same object.

For example:

```rust
let d: Box<D> = construct Box<D>(...);
let a: Box<A> = d;
```

preserves:

- the same allocation;
- the same complete `D` object;
- the same destruction obligation;
- ownership of the same allocation.

Only the exposed class view changes from `D` to `A`.

---

## 24. Value-Class Containment

A value class may contain ordinary movable values and stable owner handles.

Valid:

```rust
value class WindowHandle {
    window: Box<Window>,
}
```

Moving `WindowHandle` moves the `Box` handle but does not relocate the live `Window`.

A value class may not contain a live ordinary class inline because moving the outer value would relocate the inner object.

The same rule applies through ordinary value-generic containers. For example:

```rust
value class WindowSet {
    windows: Vec<Window>,       // invalid: Window is not a value
}

value class WindowSet {
    windows: Vec<Box<Window>>,  // valid: Box<Window> is a movable owner value
}
```

---

## 25. Ordinary-Class Containment

An ordinary class may contain:

- value classes inline;
- ordinary Rust values inline;
- ordinary classes inline as Data before activation and live subobjects after placement;
- `Box`, `Rc`, or `Arc` owners of other ordinary-class objects.

The conceptual matrix is:

| Container | Contained | Inline allowed? |
|---|---|---:|
| value class | value | yes |
| value class | ordinary class object | no |
| value class | `Box/Rc/Arc<class>` | yes |
| ordinary class | value | yes |
| ordinary class | ordinary class | yes |
| ordinary class | `Box/Rc/Arc<class>` | yes |

Inline ordinary-class composition is a placement-aware language operation and is therefore different from storing the class in an ordinary value container. These two declarations intentionally have different validity:

```rust
class Parent {
    child: Child,                 // valid inline object subplace
    children: Vec<Child>,         // invalid: Vec elements are values
    owned_children: Vec<Box<Child>>, // valid
}
```

`child: Child` reserves `ChildData` as part of the enclosing structural Data and activates the `Child` only after the complete enclosing Data reaches its final place. `Vec<Child>` would instead require `Child` to satisfy the movable element-value contract of `Vec`, which an ordinary class never does.

---

## 26. Assignment and Explicit Duplication

Assignment follows the value/object distinction.

Value classes use ordinary Rust-style assignment semantics and may support explicit clone protocols.

Ordinary classes are not assignable by whole-object replacement merely because two places have the same class type. In particular, a mutable base view may not replace the embedded base object with another base value.

Any future object-cloning facility must explicitly construct a new object at a new stable destination; it is not a move operation.

---

# Part V — Inheritance and Class Views

## 27. Inheritance Belongs to Objects

Concrete class inheritance exists only for ordinary classes.

```rust
class Shape {
    pub virtual fn area(&self) -> f64;
}

class Rectangle : Shape {
    width: f64,
    height: f64,

    pub override fn area(&self) -> f64 {
        self.width * self.height
    }
}
```

A `value class` does not participate in concrete class inheritance. Value polymorphism should use real Rust traits and generics.

---

## 28. Physical Base Representation

Concrete base Data is physically nested inside derived Data.

```rust
struct AData {
    x: i32,
}

struct BData {
    __base_a: AData,
    y: i64,
}
```

When a `B` object is activated, the embedded `AData` participates in the `A` base-object stage lifetime.

Fields and bases are not required to be flattened, and the source model does not require a C++ object layout.

---

## 29. Multiple Concrete Inheritance

Multiple concrete inheritance is allowed.

```rust
class D : A, B {
    ...
}
```

Concrete repeated-base diamonds are forbidden.

> In the transitive concrete-base closure of one complete class, a concrete base class may occur at most once.

Therefore each complete concrete class `D` has at most one concrete view of any target class `T`.

This **unique-projection invariant** substantially simplifies base conversion, RTTI, downcast, and sibling cross-cast.

Virtual inheritance is not part of the core model.

---

## 30. Base Visibility

Base edges may be:

```text
public
protected
private
```

Visibility affects whether a source context is allowed to form the corresponding class view.

It does not affect physical Data layout.

Runtime RTTI metadata is compiler-private and need not encode source access-control policy as a security mechanism. The frontend performs access checks before generating view conversion code.

---

## 31. Class View Identity

Every class view is rooted in exactly one complete object identity.

Upcast, downcast, and cross-cast preserve that identity.

Conceptually a class view contains:

```text
complete-object identity/root
static class view
borrow/ownership capability
effective dynamic class / dispatch metadata
lifetime
```

The exact backend representation may use a Rust trait object in the current bootstrap backend or a native Rust++ class-view ABI in the future compiler. This representation choice is not source-observable.

---

## 32. Borrowed Class Views

`&A` means a shared borrow of one complete live object through its `A` view.

`&mut A` means an exclusive borrow of one complete live object through its `A` view.

The backend must not interpret `&mut A` as merely an independent `&mut AData` borrow of an embedded base subobject.

This is necessary because a virtual call through `&mut A` may dispatch to the most-derived override and mutate other parts of the complete object.

---

## 33. Complete-Object Mutable Borrow Invariant

For:

```text
class D : A, B
```

two views:

```text
&mut A
&mut B
```

of the same `D` object are semantically conflicting even if `AData` and `BData` occupy disjoint byte ranges.

A mutable class view exclusively borrows the complete most-derived object identity.

Therefore a safe mutable base view must not expose an operation equivalent to:

```rust
mem::replace(&mut derived.__base_a, new_a_data);
```

The source language never exposes a safe `DerefMut<Target = AData>` for a class view.

---

## 34. Lifecycle-Bounded Views

Views formed during `init` or `deinit` are bounded by the current lifecycle stage.

They must not escape that stage unless a later rule explicitly creates a new view after the complete object reaches the corresponding live state.

For example, a reference to `self` formed in `A.init` while constructing a `D` cannot be stored somewhere with the final `D` object lifetime.

This prevents constructor rollback from leaving dangling class views.

---

# Part VI — Methods and Virtual Dispatch

## 35. Methods and Virtual Slots

Virtual methods are explicit:

```rust
virtual
override
final override
abstract class
final class
```

Virtual methods use borrowed receivers only:

```text
&self
&mut self
```

The core model excludes:

- by-value virtual `self`;
- type-generic virtual methods;
- const-generic virtual methods;
- ABI-sensitive covariant `Self` positions;
- implicit async virtual methods.

A virtual slot is owned by the class that first declares it. Unrelated bases declaring the same method name and signature still define distinct slots unless the source language provides an explicit conflict-resolution rule.

---

## 36. Dynamic Dispatch Lives in Views

The preferred object representation does not require a C++-style vptr inside every Data object.

Conceptually:

```text
object/data representation:
    bases and fields

class-view representation:
    complete-object root
    dynamic dispatch metadata
    RTTI identity
    static-view projection capability
```

The current portable Rust bootstrap lowering uses hidden Rust trait objects for live class views.

This lets the bootstrap compiler reuse Rust's trait-object vtable machinery for virtual calls and dynamic drop glue without making the Rust++ source class itself a Rust trait. The native compiler is expected to replace this representation with a Rust++-defined class-view/vtable ABI while preserving the same source semantics.

---

## 37. Effective Dynamic Class

A class view has an **effective dynamic class** used by virtual dispatch and RTTI.

Normally the effective dynamic class is the complete most-derived live class.

During construction and destruction it is capped by the current lifecycle stage.

For:

```text
class B : A
```

while constructing `B`:

```text
A.init: effective dynamic class = A
B.init: effective dynamic class = B
```

while destroying `B`:

```text
B.deinit: effective dynamic class = B
A.deinit: effective dynamic class = A
```

This rule applies through indirect calls as well as direct virtual calls.

---

## 38. Lifecycle Dispatch Is a View Property

The backend must not implement lifecycle dispatch as a syntactic special case only for calls written directly inside `init` or `deinit`.

Instead, the lifecycle stage creates a stage-specific class view whose dynamic dispatch behavior is already capped appropriately.

Thus:

```text
A.init -> A::nonvirtual_helper -> self.virtual_f()
```

still dispatches as `A`.

No permanent `current_vptr` or `initialized_stage` field is required in the live object.

---

## 39. Member Projection and Method Bodies

The backend distinguishes:

```text
object self     dynamic class-view capability
Data self       static defining-class Data projection
```

A nonvirtual method defined on `A` and called through an arbitrary `&A` may resolve the `AData` projection once for one uninterrupted borrow region and then perform direct field accesses.

Conceptually:

```rust
fn __A_sum(view: &dyn __View_A) -> i32 {
    let data = unsafe { view.__project_A() };
    data.x + data.y
}
```

For a mutable method, projected references must end before an operation that requires a conflicting complete-object mutable borrow, such as a virtual re-entry.

```rust
{
    let data = unsafe { view.__project_A_mut() };
    data.x += 1;
}

view.__slot_virtual();

{
    let data = unsafe { view.__project_A_mut() };
    data.x += 1;
}
```

This lets rustc enforce aliasing for the projected references.

A virtual override implementation is more efficient: once Rust's trait vtable dispatch selects the concrete backing `__DStorage`, the method body receives a typed `&__DStorage` or `&mut __DStorage` and can directly access all statically-known Data projections without another dynamic projection call.

---

# Part VII — RTTI and Class-View Casts

## 40. `as?` Is the Fallible Class-View Rebinding Operator

Dynamic class conversion uses:

```rust
expr as? TargetClass
```

`as?` means:

> fallibly preserve the surrounding borrow/owner capability while rebinding its class view to `TargetClass`.

It covers both traditional downcast and sibling cross-cast.

Examples:

```rust
let d = a as? D;
let b = a as? B;
```

where `a: &A`.

Calling the operation `downcast` alone would be misleading because `A -> B` may be a sibling cross-cast rather than a downward hierarchy edge.

---

## 41. Result Types of `as?`

Borrowed views do not consume ownership, so failure returns `None`:

```text
&A       as? D  -> Option<&D>
&mut A   as? D  -> Option<&mut D>
```

Owning views consume the current owner handle, so failure must return that owner unchanged:

```text
Box<A> as? D  -> Result<Box<D>, Box<A>>
Rc<A>  as? D  -> Result<Rc<D>, Rc<A>>
Arc<A> as? D  -> Result<Arc<D>, Arc<A>>
```

The cast does not clone an `Rc` or `Arc` implicitly.

If the caller wants to retain the original shared owner handle, it may explicitly clone first:

```rust
let maybe_d = a.clone() as? D;
```

---

## 42. Cast Success Condition

The success test is not:

```text
active_dynamic_class == Target
```

It is:

```text
Target occurs in the active dynamic class's concrete base closure or is the active class itself.
```

Therefore, for:

```text
C : D : A
```

an `&A` whose active dynamic object is `C` successfully converts with:

```rust
a as? D
```

because the complete active object contains a unique legal `D` view.

The no-repeated-concrete-base rule guarantees uniqueness.

---

## 43. Cross-Cast

For:

```text
    D
   / \
  A   B
```

an `&A` may dynamically convert to `&B`:

```rust
let b: Option<&B> = a as? B;
```

if the active object is `D` or another descendant containing both views.

Downcast, cross-cast, and dynamic base-view lookup use the same runtime operation: query the currently active object for a target class view.

---

## 44. Static Upcast

A statically known accessible derived-to-base conversion is infallible and does not use `as?`.

```rust
let a: &A = d;
let a: Box<A> = d_box;
```

The Rust bootstrap backend should use native Rust trait-object upcasting/unsizing where available or equivalent generated coercion glue. A native Rust++ backend performs the same semantic view conversion using its own class-view metadata ABI.

No RTTI lookup is required for an ordinary static upcast.

---

## 45. `is` and Exact Dynamic Identity

The language may provide:

```rust
expr is T
```

with the meaning:

```text
expr as? T would succeed
```

An exact-most-derived test is distinct:

```rust
expr is exact T
```

or an equivalent library spelling.

The exact spelling is less fundamental than preserving the semantic distinction between "contains a T view" and "active dynamic class is exactly T".

---

## 46. Lifecycle RTTI

RTTI observes the currently active object semantics, not merely the concrete allocation layout.

While constructing `D : A`, the `A.init` stage exposes an RTTI descriptor whose active class is `A` and whose cast table contains only views valid at that lifecycle stage.

Therefore:

```rust
// inside A.init while complete storage will eventually become D
self as? D
```

fails.

The same rule prevents `A.deinit` from downcasting into already-deactivated `D` state.

---

# Part VIII — Current Rust Bootstrap Lowering Model

## 47. Status of This Lowering

This part specifies the current **Rust bootstrap/reference lowering**. It is intentionally detailed because it provides an executable soundness strategy and makes the object model testable against rustc. It is not a commitment that the final Rust++ compiler will use Rust trait objects, Rust standard-library smart pointers, or Rust's vtable ABI internally.

The future native backend should preserve the lifecycle, ownership, view, dispatch, and RTTI invariants defined earlier while replacing the mechanisms in this part with Rust++-defined ABI structures.

### 47.1 Overview of Generated Types

For an ordinary class `C`, the bootstrap backend generates at least three conceptual Rust-level entities:

```text
__CData
    sized structural representation
    movable before activation
    Drop = structural drop only

__CStorage
    sized concrete backing type for a fully live complete C object
    layout-compatible with __CData
    Drop = complete object deactivation followed by __CData drop

dyn __View_C
    unsized polymorphic C class view
    Rust trait-object representation in the portable backend
```

The source class itself is not identical to any one of these generated Rust types.

The semantic split is:

```text
source exact object C
    = activated C object semantics over stable __CData storage

source class view C
    = dyn __View_C or equivalent view representation
```

Lifecycle stage wrappers are also generated where needed:

```text
__DStage_A
__DStage_B
__DStage_D
```

for a complete `D` object passing through different active class stages.

---

## 48. Value-Class Lowering

A value class should lower as directly as practical to an ordinary Rust struct.

Rust++:

```rust
value class Point {
    x: f64,
    y: f64,

    constructor(x: f64, y: f64) {
        new { x, y }
    }
}
```

Representative Rust:

```rust
pub struct Point {
    x: f64,
    y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}
```

If the value class has structural cleanup, its source `drop` body lowers to an ordinary Rust `Drop` implementation as directly as possible.

Value movement remains ordinary Rust movement.

---

## 49. Data Lowering for Ordinary Classes

For:

```rust
class A {
    x: i32,
}

class B {
    y: i64,
}

class D : A, B {
    z: String,
}
```

representative structural Data is:

```rust
struct __AData {
    x: i32,
}

struct __BData {
    y: i64,
}

struct __DData {
    __base_a: __AData,
    __base_b: __BData,
    z: String,
}
```

The backend may reorder hidden Rust fields where necessary to make Rust's automatic field-drop order implement the Rust++ structural-drop order, provided source-visible layout guarantees are preserved.

If a required drop order cannot be encoded through hidden field declaration order, compiler-generated `ManuallyDrop`/drop glue may be used inside the trusted boundary.

---

## 50. Live Storage Wrappers

A complete live backing type may be represented as a transparent wrapper over Data:

```rust
#[repr(transparent)]
struct __DStorage(__DData);
```

The wrapper exists to give Rust a concrete type whose `Drop` means:

```text
complete D object is live
therefore first run object deactivation
then allow __DData structural destruction
```

Representative shape:

```rust
impl Drop for __DStorage {
    fn drop(&mut self) {
        unsafe {
            __deactivate_complete_D(&mut self.0);
        }
    }
}
```

After `Drop::drop` returns, Rust drops `self.0`, invoking the structural `__DData` drop path.

Thus the concrete Rust backing type itself guarantees most-derived destruction when erased behind a trait object.

---

## 51. Hidden Class-View Traits

The portable backend generates one hidden Rust view trait per ordinary class.

A common root trait provides compiler RTTI:

```rust
unsafe trait __RppObject {
    fn __rpp_type_desc(&self) -> &'static __TypeDesc;
}
```

For class `A`:

```rust
unsafe trait __View_A: __RppObject {
    unsafe fn __project_A(&self) -> &__AData;
    unsafe fn __project_A_mut(&mut self) -> &mut __AData;

    fn __slot_A_kind(&self) -> i32;
}
```

For class `B`:

```rust
unsafe trait __View_B: __RppObject {
    unsafe fn __project_B(&self) -> &__BData;
    unsafe fn __project_B_mut(&mut self) -> &mut __BData;

    fn __slot_B_bump(&mut self);
}
```

For a derived class `D : A, B`:

```rust
unsafe trait __View_D:
    __View_A
    + __View_B
{
    unsafe fn __project_D(&self) -> &__DData;
    unsafe fn __project_D_mut(&mut self) -> &mut __DData;
}
```

The actual generated traits should be sealed so handwritten safe Rust cannot invent arbitrary class-view implementations.

The `unsafe` contract is implemented only by generated code or by explicitly trusted interop code.

---

## 52. Complete Storage Is the Trait-Object Data Pointer

For the primary lowering, a class-view trait object is backed by the complete concrete storage type, not by the embedded base Data address.

For a live `D : A, B`:

```text
&D as D-view:
    data pointer -> complete __DStorage
    metadata     -> __DStorage as __View_D

&D as A-view:
    data pointer -> same complete __DStorage
    metadata     -> __DStorage as __View_A

&D as B-view:
    data pointer -> same complete __DStorage
    metadata     -> __DStorage as __View_B
```

This has two major benefits:

1. mutable trait-object views borrow the complete backing storage rather than only an embedded base byte range;
2. sibling base views can share the same complete-object identity without C++-style adjusted data pointers.

Base field access is performed through generated projection methods.

---

## 53. Implementing Derived Views

For a fully live `D`:

```rust
unsafe impl __RppObject for __DStorage {
    fn __rpp_type_desc(&self) -> &'static __TypeDesc {
        &__DESC_D
    }
}
```

A-view implementation:

```rust
unsafe impl __View_A for __DStorage {
    unsafe fn __project_A(&self) -> &__AData {
        &self.0.__base_a
    }

    unsafe fn __project_A_mut(&mut self) -> &mut __AData {
        &mut self.0.__base_a
    }

    fn __slot_A_kind(&self) -> i32 {
        __D_kind_body(self)
    }
}
```

B-view implementation:

```rust
unsafe impl __View_B for __DStorage {
    unsafe fn __project_B(&self) -> &__BData {
        &self.0.__base_b
    }

    unsafe fn __project_B_mut(&mut self) -> &mut __BData {
        &mut self.0.__base_b
    }

    fn __slot_B_bump(&mut self) {
        __D_bump_body(self)
    }
}
```

The virtual override body receives typed complete storage and therefore uses direct static projections.

---

## 54. Nonvirtual Method Lowering

A nonvirtual method defined on class `A` is statically selected even when invoked through an `A` view of a descendant.

Representative lowering:

```rust
fn __A_score_body(view: &dyn __View_A) -> i32 {
    let data = unsafe { view.__project_A() };
    data.x + view.__slot_A_kind()
}
```

The backend may keep `data` live across shared re-entry only where normal Rust borrowing accepts it.

For mutable receivers, projected references are scoped so that no conflicting projected borrow survives a complete-object mutable re-entry.

The baseline backend prefers reference-returning projection methods over raw-pointer projection methods so rustc continues to enforce as much lifetime and aliasing validity as possible.

---

## 55. Virtual Method Lowering

A virtual slot call through `&dyn __View_A` is an ordinary Rust trait-object dispatch in the portable backend.

For a `D` object:

```text
&A
  ↓ Rust vtable
<__DStorage as __View_A>::__slot_A_kind
  ↓
__D_kind_body(&__DStorage)
```

The body then directly accesses `__DStorage` and statically known base/field projections.

Therefore the normal virtual-method cost is one Rust dynamic dispatch; ordinary field accesses inside the selected override do not require an additional class-view projection vcall.

---

## 56. Lifecycle Stage Wrappers

The normal `__DStorage` trait implementations represent a fully live `D` dynamic object and therefore dispatch overrides as `D`.

Construction and destruction require different effective dynamic classes.

The portable backend uses transparent stage wrappers over the same final `DData` bytes:

```rust
#[repr(transparent)]
struct __DStage_A(__DData);

#[repr(transparent)]
struct __DStage_B(__DData);

#[repr(transparent)]
struct __DStage_D(__DData);
```

These are not moved object values. Compiler glue temporarily interprets references to the final `DData` storage through the stage wrapper appropriate to the current lifecycle phase.

For `A.init`:

```rust
unsafe impl __View_A for __DStage_A {
    unsafe fn __project_A(&self) -> &__AData {
        &self.0.__base_a
    }

    unsafe fn __project_A_mut(&mut self) -> &mut __AData {
        &mut self.0.__base_a
    }

    fn __slot_A_kind(&self) -> i32 {
        __A_kind_body(self)
    }
}
```

Therefore any direct or indirect virtual call through that stage view dispatches as `A`.

The stage wrapper's RTTI descriptor also reports active class `A`, preventing lifecycle-invalid downcasts.

---

## 57. Bootstrap `construct Box<D>` Lowering

Representative lowering of:

```rust
let d = construct Box<D>(args...);
```

is:

```text
D.new(args...)
    ↓
(DData, DActivationFrame)
    ↓
Box<DData>
    ↓ final address established
ActivationGuard
    ↓
A.init / base stages
    ↓
field init stages
    ↓
D.init
    ↓ success
reinterpret same allocation
Box<DStorage>
    ↓ Rust unsizing
Box<dyn __View_D>
```

Representative commit operation:

```rust
unsafe fn __box_commit_D(
    x: Box<__DData>,
) -> Box<__DStorage> {
    let raw = Box::into_raw(x);
    Box::from_raw(raw.cast::<__DStorage>())
}
```

This operation changes the semantic/type interpretation of the same allocation after all required object stages have successfully activated. It does not relocate the Data.

---

## 58. Bootstrap `construct Rc<D>` Lowering

`Rc` allocation must be established before activation and remain uniquely controlled by construction machinery until activation commits.

Conceptually:

```text
(DData, frame)
    ↓
Rc<DData> with unique construction ownership
    ↓
activate using unique mutable access
    ↓
reinterpret same allocation
Rc<DStorage>
    ↓ unsize
Rc<dyn __View_D>
    ↓ publish to source code
```

No `Rc` clone and no `Weak` publication may occur before activation completes.

Representative commit:

```rust
unsafe fn __rc_commit_D(
    x: Rc<__DData>,
) -> Rc<__DStorage> {
    let raw = Rc::into_raw(x);
    Rc::from_raw(raw.cast::<__DStorage>())
}
```

The backend must ensure the sized Data and Storage wrappers have compatible layout and destruction contracts.

---

## 59. Bootstrap `construct Arc<D>` Lowering

`Arc` follows the same publication rule:

```text
allocate final Arc<Data>
retain unique construction control
activate completely
reinterpret to Arc<Storage>
unsize to Arc<class view>
publish/share only after activation
```

No other thread can observe a partially activated object through safe generated APIs.

---

## 60. Direct-Place Construction Lowering

Rust++ may support:

```rust
let d = construct D(...);
```

by creating compiler-managed final storage whose address is fixed before activation.

Conceptually:

```text
(DData, frame)
    ↓
final direct storage slot
    ↓
place complete DData once
    ↓
activate stages in place
    ↓
live-object guard owns destruction obligation
```

The source binding is a place-bound object, not an ordinary movable Rust value. It cannot be passed, returned, or assigned by ordinary by-value movement.

Direct-place construction is a Rust++ compiler capability. It is not exposed as a general safe placement API to handwritten Rust.

---

## 61. Inline-Field Construction Lowering

For:

```rust
class Parent {
    child: Child,
}
```

physical Data contains:

```rust
struct __ParentData {
    child: __ChildData,
    ...
}
```

After the complete parent Data reaches its final place, compiler glue temporarily views `&mut parent_data.child` as the appropriate `Child` lifecycle stage, runs `Child.init`, and records the deinit obligation.

Once the parent tree is live, a source `&Child` view of the inline field is formed by interpreting that stable child Data address through the generated `__ChildStorage`/view machinery.

The parent owns the child's Data structurally; there is no independent `Box` allocation.

---

## 62. Activation Guards

Fallible activation uses generated guards.

For a `D` whose activation schedule is:

```text
A
B
Child
D
```

one guard may track:

```text
final Data owner/place
completed stage count
activation frame
```

A stage increments the count only after its `init` returns successfully.

On failure or unwinding, the guard:

1. runs `deinit` for successful stages in reverse order;
2. structurally drops the complete Data;
3. releases the final storage owner where applicable.

Activation state is construction machinery, not a permanent field of every live object.

---

## 63. Structural Destruction Lowering

Value-class structural `drop` generally maps directly to Rust `Drop`.

For ordinary classes:

```text
__DStorage::drop
    -> object deactivation pass
    -> return
Rust drops __DStorage.0: __DData
    -> __DData structural drop
    -> structural child/base/value drops
```

This separation ensures an inline `ChildData` is structurally dropped after its object stage has already been deinitialized and is not accidentally deinitialized a second time.

---

# Part IX — Runtime Type Metadata and Cast Lowering

## 64. Class IDs

Rust trait-object vtable pointer identity is not used as source-language RTTI identity.

The compiler assigns each ordinary class a stable class identifier within the Rust++ ABI/versioning model.

Conceptually:

```rust
struct __ClassId(u128);
```

The exact encoding is a compiler ABI detail and may include crate identity, fully qualified class identity, generic arguments, and compiler metadata versioning.

---

## 65. Type Descriptors

Each effective dynamic class/stage exposes a descriptor:

```rust
struct __TypeDesc {
    active_class: __ClassId,
    complete_storage_class: __ClassId,
    casts: &'static [__CastEntry],
}
```

For a normal live `D`:

```text
active_class           = D
complete_storage_class = D
casts                  = D, A, B, ... accessible-by-semantics entries
```

For `D` during `A.init`:

```text
active_class           = A
complete_storage_class = D
casts                  = A and currently valid ancestors only
```

`complete_storage_class` is compiler-private information and does not authorize source access to inactive derived stages.

---

## 66. Cast Entries Use Rust-Generated Coercion Thunks

The RTTI table does not store or parse Rust vtable internals.

Instead it stores generated conversion thunks that ask Rust itself to create the correct target trait-object pointer.

Conceptually:

```rust
struct __CastEntry {
    target: __ClassId,

    make_const: unsafe fn(
        root: *const (),
        output: *mut (),
    ),

    make_mut: unsafe fn(
        root: *mut (),
        output: *mut (),
    ),
}
```

For live `D -> B`:

```rust
unsafe fn __D_make_B_const(
    root: *const (),
    out: *mut (),
) {
    let storage = root.cast::<__DStorage>();

    let view: *const dyn __View_B = storage;

    out.cast::<*const dyn __View_B>()
        .write(view);
}
```

The backend therefore depends on Rust's normal concrete-to-trait-object coercion rules, not on a hard-coded trait-object vtable layout.

---

## 67. Borrowed `as?` Lowering

For:

```rust
let d: Option<&D> = a as? D;
```

where `a: &A`, representative lowering is:

```rust
fn __cast_ref_A_to_D<'a>(
    src: &'a dyn __View_A,
) -> Option<&'a dyn __View_D> {
    let desc = src.__rpp_type_desc();
    let entry = desc.find(__CLASS_D)?;

    let root =
        src as *const dyn __View_A as *const ();

    let mut out =
        MaybeUninit::<*const dyn __View_D>::uninit();

    unsafe {
        (entry.make_const)(
            root,
            out.as_mut_ptr().cast(),
        );

        Some(&*out.assume_init())
    }
}
```

The raw pointer exists only inside compiler-generated cast glue. The returned lifetime is tied to the input borrow.

---

## 68. Mutable `as?` Lowering

For:

```rust
let d: Option<&mut D> = a as? D;
```

representative lowering preserves the same complete-object exclusive loan:

```rust
fn __cast_mut_A_to_D<'a>(
    src: &'a mut dyn __View_A,
) -> Option<&'a mut dyn __View_D> {
    let desc = src.__rpp_type_desc();
    let entry = desc.find(__CLASS_D)?;

    let root =
        src as *mut dyn __View_A as *mut ();

    let mut out =
        MaybeUninit::<*mut dyn __View_D>::uninit();

    unsafe {
        (entry.make_mut)(
            root,
            out.as_mut_ptr().cast(),
        );

        Some(&mut *out.assume_init())
    }
}
```

This operation is a reborrow/view conversion, not creation of a new independent mutable capability.

---

## 69. Bootstrap `Box` Owning Cast Lowering

For:

```rust
let result: Result<Box<D>, Box<A>> = owner as? D;
```

representative lowering is:

```text
borrow descriptor from owner
    ↓
if target absent: return Err(original owner)
    ↓
Box::into_raw(owner)
    ↓
extract complete root pointer
    ↓
cast thunk creates *mut dyn __View_D
    ↓
Box::from_raw(target trait-object pointer)
    ↓
Ok(target owner)
```

No allocation, object move, or clone occurs on success.

The underlying complete storage and dynamic drop glue remain unchanged.

---

## 70. Bootstrap `Rc` and `Arc` Owning Cast Lowering

`Rc` and `Arc` follow the same metadata-rebinding principle through their raw-owner round trip.

```text
Rc<dyn ViewA>
    -> Rc::into_raw
    -> same allocation root
    -> target view thunk
    -> Rc::from_raw
    -> Rc<dyn ViewD>
```

and similarly for `Arc`.

Strong and weak ownership counts are preserved by the cast. The operation does not implicitly clone or decrement the owner count.

---

## 71. Owner-Rebind Runtime Abstraction

The generated runtime may internally factor Box/Rc/Arc cast code through a trusted owner-rebind abstraction.

Conceptually:

```rust
unsafe trait __RppOwnerRebind<T: ?Sized>: Sized {
    type Rebind<U: ?Sized>;

    unsafe fn __rpp_rebind<U: ?Sized>(
        self,
        entry: &__CastEntry,
    ) -> Self::Rebind<U>;
}
```

This is compiler/runtime infrastructure, not necessarily a safe public Rust trait.

A future custom stable owner may participate only through an explicitly designed unsafe protocol that proves stable placement, lifecycle ownership, destruction, and metadata rebinding.

### 71.1 Native ABI Equivalent

The native Rust++ compiler need not perform any Rust `into_raw`/`from_raw` round trip. The semantic operation is simply owner-preserving class-view rebinding:

```text
native owner header / control block
    + complete object root
    + current class-view metadata
        ↓ as? Target
same owner header / control block
    + same complete object root
    + target class-view metadata
```

The native RTTI descriptor provides the target projection/dispatch metadata directly. `Box`, `Rc`, and `Arc` source semantics therefore do not depend on Rust DST metadata or Rust smart-pointer layout.

---

# Part X — Rust Traits and Rust Interoperability

## 72. Rust Traits Remain an Interop Concept, Not Class Inheritance

Rust++ class inheritance and Rust trait implementation remain separate concepts.

A value class or ordinary class may declare Rust-trait interoperability where Rust coherence permits the generated bridge implementation.

A Rust trait:

- contributes no concrete class Data;
- does not create a Rust++ class base subobject;
- does not participate in the concrete repeated-base rule;
- uses Rust's own trait semantics at the Rust interoperability boundary.

In the bootstrap Rust backend, the compiler may generate direct Rust trait implementations on the generated value/storage/view types. In the native backend, the generated Rust bridge crate instead implements the Rust trait on the exposed Rust façade type and forwards the operation through native Rust++ ABI thunks. This keeps Rust trait coherence and object safety on the Rust side without making the native Rust++ class representation itself a Rust type.

The compiler may generate forwarding implementations for inherited Rust-trait conformance where the result is semantically unambiguous and legal under Rust coherence.

---

## 73. Rust-Facing Value-Class API

A value class is intentionally easy to consume from handwritten Rust.

Rust++:

```rust
value class Point {
    constructor(x: f64, y: f64) {
        new { x, y }
    }
}
```

Rust-facing API:

```rust
impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        ...
    }
}
```

Handwritten Rust may:

```rust
let p = Point::new(1.0, 2.0);
let q = p;
let b = Box::new(Point::new(3.0, 4.0));
```

This is ordinary Rust value behavior.

---

## 74. Safe Rust Must Not Directly Construct Ordinary-Class Places

Safe handwritten Rust does not receive a movable ordinary-class value API and does not receive a general direct-placement API.

In particular, generated safe Rust API does not provide:

```text
C::new(...) -> C
C::construct(...) -> C
public CData constructors
public CStorage constructors
safe activation functions
safe stack-placement constructors
safe inline-field-placement constructors
```

Handwritten safe Rust therefore cannot directly create:

```text
a stack-resident ordinary-class object
an inline ordinary-class subobject
an activated class object over arbitrary user-provided storage
```

These operations remain compiler-controlled Rust++ language capabilities.

---

## 75. Rust-Facing Stable Owner Factories

The Rust-facing ordinary-class ABI is intentionally different in the bootstrap and native implementation phases.

### 75.1 Bootstrap bridge

While Rust itself is the implementation backend, generated safe Rust may internally/publicly use aliases around Rust owners and hidden view traits, for example:

```rust
pub type CBox = Box<dyn CView>;
pub type CRc  = Rc<dyn CView>;
pub type CArc = Arc<dyn CView>;
```

with generated factories:

```text
C::construct_box(...)
C::construct_rc(...)
C::construct_arc(...)
```

This form is useful while validating the design, but it is explicitly **not** the permanent Rust++ ABI.

### 75.2 Native Rust++ bridge

Once Rust++ has a native compiler/runtime ABI, handwritten Rust must not observe native ordinary classes as `Box<dyn Trait>`, `Rc<dyn Trait>`, `Arc<dyn Trait>`, or as the native Rust++ storage type.

Instead the generated bridge crate exposes opaque owner types, conceptually:

```rust
pub struct ClassBox<C: ClassMarker> { /* opaque */ }
pub struct ClassRc<C: ClassMarker>  { /* opaque */ }
pub struct ClassArc<C: ClassMarker> { /* opaque */ }
```

with class-specific aliases if desired:

```rust
pub type WidgetBox = ClassBox<Widget>;
pub type WidgetRc  = ClassRc<Widget>;
pub type WidgetArc = ClassArc<Widget>;
```

Factories keep the same ergonomic entry points:

```rust
let w: WidgetBox = Widget::construct_box(...);
let w: WidgetRc  = Widget::construct_rc(...);
let w: WidgetArc = Widget::construct_arc(...);
```

These opaque Rust types may contain only an ABI handle, pointer pair, or runtime-owned token. Their Rust representation is a bridge ABI choice. Safe Rust cannot inspect or reconstruct the native Rust++ storage, vtable, RTTI record, or ownership control block.

Dropping/cloning the façade calls generated/native Rust++ ABI entry points. Upcast, `as?`, method dispatch, and destruction likewise cross through generated thunks or a stable native C-compatible/Rust-bridge ABI rather than relying on Rust trait-object internals.

The public naming strategy remains an ergonomics choice; the semantic requirement is that safe Rust receives only fully activated stable owners/views and cannot forge native object state.

---

## 76. Rust++ Source vs Rust Interop Construction

Rust++ source may write:

```rust
let x = construct C(...);
let x = construct Box<C>(...);
let x = construct Rc<C>(...);
let x = construct Arc<C>(...);
```

Handwritten Rust instead writes generated owner factories:

```rust
let x = C::construct_box(...);
let x = C::construct_rc(...);
let x = C::construct_arc(...);
```

In the bootstrap backend these may return wrappers/aliases around Rust `Box/Rc/Arc`; in the native backend they return opaque `ClassBox/ClassRc/ClassArc`-style façade types. The call-site construction model does not need to change.

The asymmetry is deliberate. Rust++ source has a compiler-managed placement language. Rust interop exposes only already-sound stable ownership mechanisms and never exposes direct ordinary-class placement.

---

## 77. Rust Interop Method Calls

Safe Rust consumers receive borrowed class views through the generated bridge API.

In the bootstrap backend, public virtual methods may map directly to public or sealed trait-object methods, and nonvirtual methods may use forwarding/extension methods.

In the native backend, opaque `ClassRef/ClassMut/ClassBox/ClassRc/ClassArc`-style façade types call generated ABI thunks. The Rust-facing API does not expose compiler-private projection methods, lifecycle stage representations, native vtables, RTTI internals, or concrete native Storage.

---

## 78. `Send`, `Sync`, and Auto-Trait Capabilities

The backend must not accidentally promise `Send` or `Sync` merely because the hidden concrete storage happens to satisfy them today.

A polymorphic class view can contain future/legal descendants with different auto-trait properties.

Therefore `Send`/`Sync` are class-view capabilities/contracts, not facts inferred only from one current concrete backing type.

A possible source form is:

```text
Box<A + Send>
Arc<A + Send + Sync>
```

The bootstrap backend may lower this to Rust trait objects carrying corresponding Rust auto-trait bounds. The native backend records the same capability in the Rust++ class/view ABI and exposes only Rust bridge types whose own `Send`/`Sync` implementations are generated when the native contract permits them.

The exact source syntax may evolve, but the soundness requirement is fixed: a polymorphic owner/view may claim an auto trait only if every dynamic object permitted by that view contract satisfies it.

Ordinary class exact backing storage is conceptually non-relocatable while live; exposing `Unpin` for a live ordinary-class view must not create a safe relocation path.

---

## 79. Cross-Crate Metadata and Native Artifacts

During the Rust bootstrap phase, a Rust++ library may emit compiler metadata alongside a generated Rust crate, for example:

```text
libfoo.rlib
libfoo.rppmeta
```

A native Rust++ compiler may instead emit a native code/library artifact plus the same logical Rust++ metadata, and optionally a generated Rust bridge crate:

```text
libfoo.<native-artifact>
libfoo.rppmeta
foo_rust_bridge.rlib        // optional interoperability artifact
```

Metadata may include:

- value-class vs ordinary-class classification;
- concrete class hierarchy;
- Data/base structure information required for downstream generated code;
- base visibility;
- protected member metadata;
- virtual slot identities and override relations;
- abstract/final information;
- constructor signatures;
- activation/deactivation schedules;
- class IDs and RTTI relationships;
- Rust trait conformance;
- generic information;
- compiler ABI/version information.

Handwritten Rust needs only the generated public Rust crate unless it participates in explicitly unsafe/compiler-supported subclassing or low-level interop.

---

# Part XI — Unsafe Boundary and Proof Obligations

## 80. No Required Object-Embedded vptr

The source semantics do not require an object-embedded C++-style vptr.

The current Rust bootstrap backend reuses Rust trait-object metadata for class views:

```text
complete concrete Storage
    +
Rust dyn-trait metadata
```

The long-term native backend is expected to use a Rust++-defined class-view metadata/vtable ABI instead. That ABI may keep dispatch metadata in views/owners, may use compact per-view tables, and may evolve independently of rustc.

Neither source semantics nor the long-term public ABI may depend on the internal byte layout of Rust's vtable.

---

## 81. ABI Independence and Native Compiler Target

Rust++ does not target the C++ object ABI, and it also does not permanently target the Rust trait-object/smart-pointer ABI.

The current generated-Rust backend is an implementation stage. The long-term target is a Rust++-controlled compiler/runtime ABI with stable definitions for class views, vtables/dispatch metadata, RTTI, owners, lifecycle entry points, and cross-language thunks.

Ordinary class inheritance does not imply:

- C++ vtable layout;
- C++ `this` adjustment rules;
- C++ RTTI representation;
- C-compatible base layout;
- virtual destructor declarations.

`#[repr(C)]` may be used only for explicit FFI/bridge records where separately required. Native Rust++ class storage itself need not be C-compatible or Rust-layout-compatible.

For Rust consumers, the compatibility boundary is the generated opaque bridge crate, not the native class layout. This permits the native compiler to change object/view/owner internals without requiring handwritten Rust to recompile against undocumented layout details, subject to the declared bridge ABI/version policy.

---

## 82. Compiler-Generated `unsafe`

Trusted generated `unsafe` may be required for:

- interpreting final `TData` storage through a lifecycle stage wrapper;
- committing `TData` ownership into a live `TStorage` owner;
- stack/direct final placement;
- inline class-field lifecycle projection;
- implementing compiler-private Data projections;
- RTTI erased cast thunks;
- `Box/Rc/Arc` raw-owner rebinding during dynamic cast;
- layout-compatible casts between Data and Storage wrappers.

Ordinary source code should not need an unsafe primitive merely to create or use a normal class object.

---

## 83. Prefer References over Raw Pointers for Member Projection

The portable backend should prefer projection methods that return ordinary Rust references tied to the class-view borrow:

```rust
unsafe fn __project_A(&self) -> &__AData;
unsafe fn __project_A_mut(&mut self) -> &mut __AData;
```

rather than returning raw pointers for ordinary member access.

This lets rustc enforce:

- projection lifetime;
- mutable exclusivity;
- reborrow rules;
- invalid overlapping use across mutable re-entry.

Raw pointers remain appropriate at narrow representation-changing boundaries such as erased RTTI cast glue and owner `into_raw/from_raw` conversion.

---

## 84. Three Relevant Lifetimes

For a method/view access, the proof model distinguishes:

```text
'object   complete object-lifetime epoch
'borrow   current class-view borrow
'access   one projected Data-reference region
```

with:

```text
'access ⊆ 'borrow ⊆ 'object
```

A projection reference may never outlive the class-view capability from which it was derived.

A mutable projection reference must not remain live while the same complete object is mutably re-entered through another capability, such as a virtual call requiring `&mut self`.

---

## 85. Lifecycle Epochs

Address stability is not sufficient to prove object validity.

The same Data address may exist:

```text
before activation
during A stage
during D live state
after D deinit
```

These are different object-lifetime epochs.

A class view is valid only for the lifecycle epoch in which it was created.

`deinit` invalidates all class views belonging to the ended stage/object epoch even though the Data bytes may remain alive for structural destruction.

A Data pointer after `deinit` does not authorize reconstruction of a class view.

---

## 86. View Provenance Invariant

Every class-view conversion preserves:

- complete object identity;
- borrow/ownership provenance;
- lifecycle epoch;
- owner kind, unless the source explicitly performs another ownership operation.

For example:

```text
&mut A as? D
```

is a reborrow of the same complete-object exclusive capability. It does not manufacture an independent `&mut D` from an address.

Similarly:

```text
Rc<A> as? D
```

rebinds the same Rc ownership token to another class view; it does not clone the Rc.

---

## 87. Backend Soundness Obligations

The compiler-controlled boundary must preserve at least the following properties:

1. A live ordinary-class object is never relocated.
2. Complete Data is valid before activation begins.
3. A class view never outlives its corresponding object-lifetime epoch.
4. Lifecycle-stage views cannot escape into later stages or the final live lifetime.
5. A mutable class view exclusively borrows the complete object identity.
6. A safe mutable base view cannot replace its embedded base Data.
7. Projected Data references remain within the originating view borrow and obey Rust aliasing.
8. A successful `init` stage creates exactly one `deinit` obligation.
9. A failed `init` stage creates no `deinit` obligation for that failed stage.
10. Structural Data is dropped exactly once for each established ownership obligation.
11. Lifecycle virtual dispatch never reaches a more-derived inactive or already-deactivated class stage.
12. RTTI and `as?` expose only currently valid active class views.
13. Upcast/downcast/cross-cast preserve complete object identity and capability provenance.
14. Owning polymorphic destruction reaches the most-derived concrete Storage.
15. `Rc`/`Arc` construction does not publish the owner before full activation.
16. Dynamic owner rebinding preserves allocation and reference counts.
17. Value-class movement is ordinary representation movement and invokes no hidden user move code.
18. Generated code does not rely on Rust vtable pointer equality as class identity.
19. Safe handwritten Rust cannot directly forge Data-to-object activation.
20. Safe handwritten Rust cannot obtain the hidden concrete live Storage as a movable value.

These are frontend/backend proof obligations; not all are automatically guaranteed by rustc.

---

# Part XII — Compact Surface Model

## 88. Construction Syntax Summary

### Value class

```rust
let v = V::new(...);
let b = Box::new(V::new(...));
```

### Ordinary class in Rust++ source

```rust
let c = construct C(...);
let b = construct Box<C>(...);
let r = construct Rc<C>(...);
let a = construct Arc<C>(...);
```

### Ordinary class in handwritten safe Rust

```rust
let b = C::construct_box(...);
let r = C::construct_rc(...);
let a = C::construct_arc(...);
```

No safe handwritten Rust direct-place ordinary-class construction is exposed.

---

## 89. View and Cast Summary

For `D : A`:

```text
&D        -> &A          infallible upcast
&mut D    -> &mut A      infallible upcast
Box<D>    -> Box<A>      infallible owner/view upcast
Rc<D>     -> Rc<A>
Arc<D>    -> Arc<A>
```

Fallible dynamic view rebinding:

```text
&A       as? D  -> Option<&D>
&mut A   as? D  -> Option<&mut D>
Box<A>   as? D  -> Result<Box<D>, Box<A>>
Rc<A>    as? D  -> Result<Rc<D>, Rc<A>>
Arc<A>   as? D  -> Result<Arc<D>, Arc<A>>
```

Sibling cross-cast uses the same `as?` operator.

---

## 90. Value/Object Mental Model

The most important category distinction is:

```text
value class V
    -> V is a Value
    -> V may be passed, returned, stored in Vec<V>, Option<V>, tuples, and other value generics

ordinary class C
    -> C is not a Value
    -> C denotes an exact stable object place
    -> &C / &mut C / Box<C> / Rc<C> / Arc<C> are Values carrying capabilities to C
```

```text
VALUE CLASS

    ∅
    |
    | V::new
    v
+-----------+
|     V     | == VData
+-----------+
    |
    | ordinary Rust moves
    |
    | drop
    v
    ∅
```

```text
ORDINARY CLASS

    ∅
    |
    | constructor.new
    v
+-----------+
|   TData   | ordinary structural value
+-----------+
    |
    | place into final destination
    v
+-----------+
| final Data|
+-----------+
    |
    | lifecycle-stage init
    v
+-----------+
| object T  | stable object identity
+-----------+
    |
    | lifecycle-stage deinit
    v
+-----------+
|   TData   | object semantics ended
+-----------+
    |
    | structural drop
    v
    ∅
```

---

# Part XIII — Core Semantic Invariants

## 91. Value Identity Invariant

```text
For every value class V:
V ≡ VData
```

There is no independent object activation state.

---

## 92. Representation-Move Invariant

```text
For every value class V:
move(V) has exactly the semantics of moving V's complete representation.
```

No user-defined move operation is invoked.

---

## 93. Ordinary-Class Lifetime Invariant

```text
For every ordinary class T:
a T object exists only while complete T-related Data at a stable place
is in a lifecycle state where the T stage is active.
```

### 93.1 Ordinary-Class Non-Value Invariant

```text
For every ordinary class C:
C is a class/object-place kind, not a transferable Value.
```

Therefore ordinary Rust-style value-generic positions cannot be instantiated directly with `C`. A class participates in the value world only through a value capability such as a borrow/view or stable owner, or through a separately specified placement-aware construct. In particular:

```text
Vec<C>      invalid
Option<C>   invalid
Box<C>      valid movable owner value; C itself does not move
&C          valid borrowed view value
```

This invariant is stronger than saying that `C` is merely non-movable or `!Unpin`: there is no source-language `C` value whose movement must be prohibited.

---

## 94. Construction Invariant

```text
constructor.new:
    creates complete valid structural Data and activation-frame state

final placement:
    establishes the place in which object identity will exist

constructor.init:
    begins class-stage object lifetime in that place
```

---

## 95. Destruction Invariant

```text
deinit:
    ends object-stage lifetime
    Data remains

drop:
    destroys structural Data
```

`drop` is not a method on the dead object.

---

## 96. View Invariant

```text
Inheritance creates relationships between object views,
not value conversions.
```

No ordinary class conversion materializes a base value from a derived object.

---

## 97. Complete-Identity Invariant

Every borrowed or owning class view is rooted in one complete object identity.

All class-view conversions preserve that identity.

---

## 98. Mutable-View Invariant

A mutable class view exclusively borrows the complete object identity, regardless of which physical base Data is projected for field access.

---

## 99. Unique-Projection Invariant

For any complete concrete class `D` and concrete target class `T`, there is at most one concrete `T` view in `D`.

This follows from the prohibition on repeated concrete bases.

---

## 100. Lifecycle-Dispatch Invariant

```text
normal live view:
    effective dynamic class = most-derived live class

init/deinit stage view:
    effective dynamic class = current lifecycle class
```

Virtual dispatch and RTTI use the effective dynamic class.

---

## 101. Ownership Invariant

Moving an owner handle moves the owner representation, not the live object.

An owner-view upcast or dynamic cast changes the exposed class view without relocating or reconstructing the complete object.

---

# Part XIV — Open Questions and Deferred Features

## 102. Custom Stable Owners

The core language recognizes direct placement plus the core `Box`, `Rc`, and `Arc` owner categories. In the bootstrap backend these map to Rust standard-library owners; the native backend is free to implement them with Rust++ runtime ownership structures.

A future extension may allow:

```text
ArenaBox<T>
GC<T>
intrusive owners
other stable allocators
```

through an explicit unsafe compiler/runtime placement-owner protocol.

The protocol must prove:

- stable final storage before activation;
- unique construction-time access;
- correct deactivation and structural destruction;
- correct ownership transfer/drop;
- class-view metadata support;
- safe dynamic owner rebinding where advertised.

This should not be inferred from an arbitrary generic container merely because it stores a pointer.

---

## 103. Self-Reference

Stable address alone does not make stored self-borrows safe.

The core model does not automatically permit an `init` method to store an ordinary safe Rust reference into the object pointing back into the same object with the final object lifetime.

Safe general self-reference requires a separately designed pinned/self-referential abstraction or compiler analysis.

Until then, low-level self-reference belongs behind explicit unsafe/pinned mechanisms.

---

## 104. Explicit Object-to-Data Operations

Normal safe source code cannot manually execute:

```text
live object -> deinit -> Data -> reactivate
```

while retaining arbitrary aliases.

The lifecycle states exist to define compiler semantics, not to imply a public placement-reconstruction API.

Any future low-level API must account for:

- outstanding views;
- pinning/stability;
- lifecycle epochs;
- deinit obligations;
- structural-drop ownership;
- reactivation safety.

---

## 105. Fallible Constructor Surface Details

The semantic failure model is fixed, but source spelling for constructor error types and implicit/explicit `Result` propagation may still be refined.

Regardless of syntax:

```text
failed new:
    ordinary local/Data cleanup

failed init:
    reverse successful-stage deinit
    then complete structural Data drop
```

must remain unchanged.

---

## 106. Explicit Object Cloning

Move and clone remain distinct:

```text
move  = representation/owner movement; no user move code
clone = explicit potentially user-defined duplication
```

A future ordinary-class clone facility must construct a new complete object at a distinct final stable destination.

---

## 107. Value-Class Polymorphism

Value classes continue to use Rust traits and generics for polymorphism.

Any future value-class polymorphic feature must preserve:

```text
V ≡ VData
move(V) ≡ move(VData)
```

A mechanism that requires stable object identity, independent activation, or a place-bound value belongs to ordinary `class` instead.

---

## 108. Optimized View ABI

The portable backend may use a projection trait method for nonvirtual access through an arbitrary base view.

A future optimized backend may replace this with compiler-known projection metadata, constant offsets, or backend intrinsics so long as:

- complete-object mutable borrowing remains sound;
- source view semantics are unchanged;
- virtual dispatch behavior is unchanged;
- the language does not become dependent on an undocumented Rust vtable layout.

The baseline reference-returning projection ABI is intentionally chosen for simplicity and proofability, not as a mandatory performance ABI.

---

# 109. Final Design Summary

Rust++ deliberately separates values from stable-identity objects and treats that separation as a type-category boundary, not merely as a movement restriction.

A value class is a Rust-like value:

```text
V::new
V ≡ VData
ordinary affine movement
structural drop
```

An ordinary class is **not a value**. A bare class name denotes an exact stable object/place kind. Ordinary Rust-style value generics therefore do not accept it directly:

```text
Vec<C>        invalid
Option<C>     invalid
Result<C, E>  invalid as an ordinary value Result

&C            movable borrowed-view value
Box<C>        movable owner value
Rc<C>         movable shared-owner value
Arc<C>        movable shared-owner value
```

Moving one of the capability values above never moves the live `C` object. Fixed inline class fields remain legal because inline composition is a compiler-managed placement operation, not a value-container operation. Thus `class Parent { child: Child }` is valid while `Vec<Child>` is not.

An ordinary class is an activated object over structural Data:

```text
constructor.new
    -> Data + activation frame

construct target
    -> choose final place/owner
    -> place complete Data
    -> lifecycle activation

live class views
    -> complete-object-rooted capabilities
    -> virtual dispatch through view metadata
    -> inheritance without slicing

as? T
    -> fallible class-view rebinding
    -> downcast and sibling cross-cast

deinit
    -> end object semantics

drop
    -> destroy structural Data
```

The current Rust bootstrap backend uses:

```text
ordinary Rust structs
    for value classes and class Data

hidden concrete Storage wrappers
    for fully live complete ordinary-class objects

hidden Rust dyn traits
    for borrowed and owning polymorphic class views

Rust Box/Rc/Arc
    as bootstrap implementations of Rust++ stable owner categories

compiler TypeDescriptor metadata
    for class RTTI and dynamic class-view lookup

lifecycle stage wrappers
    for constructor/destructor dispatch ceilings
```

The long-term native compiler is expected to replace those implementation mechanisms with:

```text
Rust++ native Data/storage lowering
Rust++ native class-view/vtable ABI
Rust++ native TypeDescriptor/RTTI ABI
Rust++ native Box/Rc/Arc-like owner/control-block ABI
Rust++ native lifecycle and placement glue
opaque ClassBox/ClassRc/ClassArc/ClassRef/ClassMut Rust bridge types
```

The crucial distinction is that Rust trait objects and Rust standard-library owners are bootstrap backend mechanisms, not the source definition or permanent ABI of a class.

This lets the current implementation reuse Rust's ownership containers, borrowing, trait-object dispatch, drop glue, and RAII while preserving a source object model in which:

```text
Data existence != object existence
ordinary class != value
non-value object != merely non-movable Rust value
inheritance != value subtyping
owner movement != object movement
Vec<C> is a category error, while Vec<Box<C>> is a value container
virtual dispatch != embedded C++ vptr
RTTI != Rust vtable identity
```

The trusted backend remains responsible for final placement, activation/deactivation, class-view provenance, lifecycle RTTI, and representation-changing operations. Today those obligations connect Rust++ semantics to Rust; in the native compiler they become direct compiler/runtime ABI obligations. Rust interoperability then occurs only at explicit opaque bridge types and generated ABI thunks.
