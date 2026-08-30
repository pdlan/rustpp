# Rust++ Bootstrap Compiler Roadmap

This is the implementation ledger for the generated-Rust bootstrap compiler.
The semantic source of truth is `rustpp_class_value_object_model.md`, Parts
I–XIII. Part XIV is intentionally deferred. A feature is complete only when
its syntax, validation, lowering, positive and negative tests, runtime
behavior, and applicable unsafe checks pass.

## Status vocabulary

- `planned`: requirement identified but not implemented
- `partial`: some layers or cases implemented
- `implemented`: implementation and ordinary tests pass
- `verified`: all required runtime, compile-fail, and soundness gates pass
- `deferred`: explicitly belongs to Part XIV

## Language profile decisions

- Source files contain Rust-compatible items plus Rust++ `value class` and
  ordinary `class` items.
- Rust++-specific constructs receive syntax and semantic nodes; they are never
  rewritten by textual search-and-replace.
- Rust-compatible type and expression fragments are validated with `syn`, but
  Rust++ type categories and lifecycle operations are validated by this
  compiler before Rust emission.
- Value classes are movable values. Bare ordinary classes are exact object
  places and are not values. References and `Box`/`Rc`/`Arc` class owners are
  movable capabilities.
- The bootstrap public bridge may use generated Rust traits and owner aliases;
  Data, Storage, lifecycle stages, projections, and activation operations stay
  compiler-private.
- Generated Rust targets stable Rust. A pinned nightly toolchain is used only
  for Miri validation.

## Backend safety contracts

Every generated `unsafe` block must cite one of these contracts in a nearby
comment and have a focused test.

- `SC-DATA-STORAGE-LAYOUT`: `#[repr(transparent)]` Data/Storage/stage
  reinterpretation preserves address, size, alignment, and field validity.
- `SC-ACTIVATION-COMMIT`: Data is valid, uniquely owned, at its final address,
  and fully activated before ownership is retyped as live Storage.
- `SC-STAGE-VIEW`: a lifecycle-stage view is scoped to the activation or
  deactivation call and cannot escape that epoch.
- `SC-PROJECTION`: a generated projection returns the unique concrete base
  subobject belonging to the complete storage and ties its lifetime to the
  originating view borrow.
- `SC-CAST-THUNK`: an RTTI cast thunk receives the matching complete-storage
  root and asks Rust to construct target trait-object metadata.
- `SC-OWNER-REBIND`: raw owner conversion preserves allocation, counts,
  provenance, owner kind, and dynamic drop glue exactly once.

## Milestones

| ID | Deliverable | Status | Required evidence |
|---|---|---|---|
| M0 | Executable specification and validation harness | implemented | conformance ledger, stable phase diagnostic IDs, compile/run/Miri gates, exhaustive small-graph lifecycle model, and generated ABI-ID corpus exist |
| M1 | Layered IR and complete value classes | implemented | constructors, fields/methods, structural Drop, moves, explicit Clone, type/lifetime/const/default/where generics, recursive kinds, and Rust traits pass |
| M2 | Exact ordinary classes and lifecycle | implemented | final placement, stable identity, activation, rollback, two-pass destruction, unwind, epoch, and opacity tests pass |
| M3 | Inline composition and direct placement | implemented | inline/direct final-slot lifecycle and scope-aware nonmovement/pass/return/reassignment diagnostics pass |
| M4 | Concrete inheritance and views | implemented | graph, physical-base lifecycle, public/nonpublic access, field/method privacy and ambiguity, typed receivers, mutable-root, and upcast tests pass |
| M5 | Virtual dispatch and lifecycle stages | implemented | complete-rooted live/stage views, public/private dispatch, capped indirect dispatch, abstract/final slot rules, receiver checks, and unwind paths pass |
| M6 | RTTI and casts | implemented | normal-live/lifecycle identities, cast matrix, stage-bounded sets, statically typed operands, compound conditionals, borrow/deref forms, and opaque generic call results pass |
| M7 | Box/Rc/Arc owner families | implemented | construction, upcast, dynamic rebind, counts, failure recovery, conservative auto-trait behavior, and Miri pass |
| M8 | Generics, interop, metadata | implemented | recursive value kinds, value generics, value/class Rust trait bridges, conservative auto-trait safety, and deterministic ABI-versioned metadata emission pass |
| M9 | Audit and conformance closure | implemented | all Parts I–XIII rows audited; 49 tests, warning-free Clippy, AST freshness, and pinned-nightly Miri are green |

## Normative conformance map

The section ranges below are expanded into individual cases as their milestone
starts. Test names and commands are recorded in the Evidence column.

| Spec | Requirement group | Milestone | Status | Evidence / notes |
|---|---|---:|---|---|
| §§1–4, 91–93 | value/class kinds, exact places, movement | M1/M2 | implemented | recursive value/class kind checking across fields, constructors, methods, functions, locals, containers, owners, borrows, and direct places passes; optional class-kind generic syntax is not required |
| §§5–15, 94 | construction, placement, activation, failure | M2/M3 | implemented | source Box/Rc/Arc and direct final-slot construction, activation frames, base/inline ordering, and unwind rollback verified; detailed expected-error surface spelling belongs to deferred §105 |
| §§16–20, 95 | deinit, structural drop, unwind | M2/M3 | implemented | complete-rooted two-pass parent/child/base destruction, activation rollback, and panic-resilient remaining-stage deinit pass runtime and Miri gates |
| §§21–26, 101 | movement, owners, containment, assignment | M2/M3/M7 | implemented | owner movement, shared clones, inline containment, direct-place nonassignment, and polymorphic rebind verified; explicit object duplication belongs to deferred §106 |
| §§27–34, 96–99 | inheritance, views, complete identity | M4 | implemented | physical bases, graph rejection, complete-rooted views, public/nonpublic conversion paths, field/method access and ambiguity, epoch bounds, and complete-root mutable borrowing verified |
| §§35–39, 100 | methods, slots, lifecycle dispatch | M5 | implemented | nonvirtual and public/private virtual methods, typed receivers, slots/overrides/final/abstract rules, live base-view dispatch, and complete-rooted stage-capped dispatch verified |
| §§40–46 | RTTI, `as?`, `is`, cross-cast | M6 | verified | live/lifecycle identity, borrow and Box/Rc/Arc down/cross-casts, stage-bounded tables, typed identifiers/members/locals, cloned shared owners, compound conditionals/borrows, and opaque generic borrow/owner-producing calls verified |
| §§47–63 | generated-Rust representation and lifecycle | M2–M5 | verified | Data/Storage/stages, lifetime-tied stage views, owner/direct/inline placement, guards, structural destruction, layout equality, opacity, and Miri pass |
| §§64–71 | metadata and borrowed/owning casts | M6/M7 | implemented | explicit logical ABI identities produce relocatable deterministic IDs/artifacts; descriptors, Rust coercion thunks, and Box/Rc/Arc rebind with failure recovery/count preservation are verified under Miri |
| §§72–79 | Rust interop, capabilities, cross-crate metadata | M8 | implemented | value and polymorphic ordinary-view Rust trait impls pass; views conservatively claim no Send/Sync capability; deterministic ABI-versioned `.rppmeta` artifacts are emitted; §79 does not require a bootstrap import syntax |
| §§80–87 | unsafe boundary and soundness obligations | all | verified | every generated unsafe block carries an `SC-*` contract; the §87 matrix below covers all 20 obligations, transparent layouts and bridge opacity are tested directly, and trusted runtime paths pass Miri |
| §§88–101 | compact model and core invariants | all | verified | §88 construction surfaces; §89 full view/cast matrix; §§91–92 value representation/move; §§93–95 stable lifetime and two-phase lifecycle; §§96–99 identity, access, mutable-root and unique projection; §100 stage dispatch/RTTI; §101 owner rebind are each covered by the named tests and §87 evidence matrix |
| §§102–108 | open questions | — | deferred | extension seams only; no invented semantics |

## Backend soundness evidence (§87)

| # | Obligation | Evidence |
|---:|---|---|
| 1 | live objects never relocate | `ordinary_class_runs_lifecycle_at_one_stable_address`, direct-place and owner identity tests |
| 2 | complete Data valid before activation | constructor completeness diagnostics; `panicking_init_drops_data_without_deinit` |
| 3 | views do not outlive object epoch | `class_views_cannot_escape_their_borrow_or_lifecycle_epoch` |
| 4 | lifecycle-stage views cannot escape | lifetime-tying stage helpers plus the stage-specific half of the same compile-fail test |
| 5 | mutable views borrow complete identity | `nonpublic_bases_are_internal_and_mutable_views_borrow_the_complete_root` |
| 6 | mutable base cannot replace embedded Data | hidden Data/Storage and complete-root trait views; external opacity compile-fail test |
| 7 | projections stay within originating borrow | reference-returning projections and rustc overlapping-mutable-borrow compile failure |
| 8 | successful init creates one deinit obligation | lifecycle model and normal/inline destruction runtime tests |
| 9 | failed init creates no failed-stage deinit | `panicking_init_drops_data_without_deinit`, inline rollback test |
| 10 | structural Data drops exactly once | owner, rollback, deinit-panic, inline, and value-drop counters |
| 11 | lifecycle dispatch is stage-capped | `lifecycle_stage_views_cap_indirect_dispatch_and_active_rtti`, private-slot stage test |
| 12 | lifecycle RTTI exposes only active views | stage descriptor/downcast assertions in the same test and Miri fixture |
| 13 | casts preserve identity/provenance | full borrowed/mutable/owner cast matrix and address assertions |
| 14 | owning destruction reaches most-derived Storage | polymorphic owner drop counters for Box/Rc/Arc |
| 15 | Rc/Arc publish only after activation | generated owner factories activate uniquely before `Rc::from`/`Arc::from`; panic tests and Miri |
| 16 | owner rebinding preserves allocation/counts | strong/weak count, upgrade, address, and failure-recovery tests under Miri |
| 17 | value movement is representation movement | value move and exact-once structural Drop tests |
| 18 | RTTI does not use vtable equality | stable class IDs/descriptors and Rust-generated coercion thunks; generated-source audit |
| 19 | safe Rust cannot forge activation | compiler-private Data/lifecycle entry points; external bridge compile failure |
| 20 | safe Rust cannot obtain movable Storage | `transparent_lifecycle_layouts_match_and_safe_bridge_hides_storage` |

## Autonomous revision protocol

For each conformance unit: add a failing test, implement in the owning compiler
layer, run the narrow suite, inspect generated Rust, run the milestone suite,
run Miri when trusted code changed, and update this ledger. Failures are
classified as syntax, resolution, semantic typing, class graph, lifecycle
plan, view plan, backend lowering, unsafe runtime, or specification/test
mismatch. Fixes belong in that layer rather than in output-string patches.

## Progress journal

### 2026-08-30 — Baseline

- Existing pipeline: lexer → lossless Rowan tree → AST facade →
  value-class-only HIR → generated Rust.
- `cargo test --workspace`: 8 compiler tests pass.
- Stable toolchain: Rust/Cargo 1.98.0; Clippy available.
- Miri is not installed on the active stable toolchain; M0 will add a pinned
  nightly setup before Miri becomes a hard gate.
- Repository has no Git metadata in or above the workspace, so progress is
  tracked by this ledger and test evidence rather than commits.

### 2026-08-30 — First ordinary-class lifecycle slice

- Added lossless syntax and typed AST nodes for ordinary `class`, `init`,
  `destructor`, `deinit`, and structural `drop` blocks.
- Generalized the module HIR to carry value and ordinary classes and added
  ordinary-class initializer/destructor validation.
- Added generated Data, transparent live Storage, sealed public view trait,
  `CBox`, and `C::construct_box`. The only representation cast cites
  `SC-ACTIVATION-COMMIT` and `SC-DATA-STORAGE-LAYOUT`.
- Runtime test verifies `init -> deinit -> structural drop`, stable address
  across owner movement, generated-code compilation under `-Dwarnings`, and
  exact-once owner destruction.
- Evidence: `cargo test --workspace` passes 10 tests.
- Known limitation: this is deliberately recorded as partial. Activation
  frames, methods, direct/inline placement, inheritance, stage views, casts,
  and shared owners remain unimplemented.

### 2026-08-30 — Stable owners, semantic plans, and inline composition

- Added `ClassId`, `FieldId`, explicit value/class owner/borrow type kinds, and
  activation/deactivation plans. Backend validation rejects inconsistent plans
  instead of reconstructing lifecycle semantics while printing Rust.
- Added `Box`, `Rc`, and `Arc` construction with unique pre-publication
  activation and in-place transparent commit. Runtime tests verify allocation
  identity, cloned-owner counts, one final destruction, and Data-only cleanup
  when `init` panics.
- Added constructor activation frames, including Rust format-string captures,
  and backend type lowering from class capabilities to generated owner/view
  types. Bare ordinary classes nested in value containers are diagnosed at
  their type spans.
- Added inline `construct Child(...)` lowering. Child Data and frame are folded
  into the parent before placement; child activation precedes parent init;
  rollback deinitializes completed children; normal destruction deinitializes
  the tree before structural Data dropping.
- Added `ordinary-class-demo`, exercising Box/Rc/Arc and inline placement. Both
  `cargo run -p ordinary-class-demo` and
  `cargo +nightly-2026-08-30 miri run -p ordinary-class-demo` pass.
- Evidence: 15 compiler tests pass, generated fixtures compile with
  `-Dwarnings`, and Miri reports no undefined behavior for the current trusted
  Data/Storage casts and owner commits.

### 2026-08-30 — Concrete inheritance and live dispatch

- Added inheritance lists with public/protected/private edges and explicit
  `base A(args...)` structural initialization.
- Added class-graph resolution, cycle rejection, repeated-concrete-base
  rejection, physical nested base Data, base activation frames, ordered
  activation/rollback, reverse deactivation, and reverse structural dropping.
- Generated view traits now root every base/derived view in the same complete
  Storage. Public inheritance edges support borrowed and Box/Rc/Arc Rust trait
  upcasts without allocation, movement, cloning, or identity change.
- Added class methods, explicit virtual slots, override/final validation,
  abstract-slot tracking, and concrete dispatch through derived and base views.
  Nonvirtual methods remain statically selected by the target view.
- Evidence: 20 compiler tests pass, including multiple inheritance lifecycle,
  graph rejection, owner upcast identity, abstract completion, final rejection,
  and virtual dispatch through an upcast owner.
- Known gap: normal live dispatch is implemented, but lifecycle-stage wrappers
  and indirect stage-capped virtual calls are not yet implemented; M5 remains
  partial.

### 2026-08-30 — RTTI and class-view rebinding

- Added deterministic 128-bit bootstrap class IDs, normal-live type
  descriptors, active/complete identities, and per-concrete cast tables.
- Cast entries use Rust-generated raw concrete-pointer coercion thunks; no
  trait-object metadata is inspected or compared.
- Added exact dynamic identity checks, borrowed and mutable downcasts, sibling
  cross-casts, and Box/Rc/Arc owning rebind with original-owner recovery on
  failure. Runtime tests verify complete address, strong/weak counts, and Weak
  upgrade behavior.
- Miri initially found an invalid Stacked Borrows retag caused by constructing a
  temporary reference inside the Rc cast thunk. The thunk was corrected to
  coerce the provenance-carrying raw concrete pointer directly to the target
  trait-object pointer, matching §66. The expanded cast demo now passes Miri.
- Evidence: 21 compiler tests, full workspace tests, Clippy with warnings
  denied, and `cargo +nightly-2026-08-30 miri run -p ordinary-class-demo` pass.
- Lifecycle stage descriptors and stage-bounded cast sets remain required
  before M6 is complete.

### 2026-08-30 — Local lifecycle-stage views

- Added transparent current-class stage wrappers and rewrote lifecycle bodies
  so both direct virtual calls and calls reached through nonvirtual helpers use
  the active stage view. Rewriting covers parsed expressions and Rust macro
  token trees without textual source replacement.
- Runtime coverage proves `A` dispatch during `A.init`/`A.deinit` and `D`
  dispatch during `D.init`/`D.deinit`, including exact active identity checks.
- Class declarations now accept and semantically validate `abstract` and
  `final`; tests reject implicit abstract classes, contradictory
  `abstract final` declarations, and derivation from a final class.
- Evidence: 23 compiler tests pass; the workspace test suite and Clippy with
  warnings denied pass.
- This is intentionally not marked complete: an embedded base stage currently
  roots its view in the base `Data`, rather than in the most-derived complete
  `Data`. M5/M6 require per-complete-class stage wrappers, complete-root
  projection, and stage-specific cast descriptors before verification.
- Added deactivation obligation guards. If a class or subobject `deinit`
  unwinds, the guard consumes each still-live child/base stage at most once;
  Rust then performs structural Data destruction once. A runtime panic test
  proves the remaining child deinit and both structural drops occur.
- Evidence after this addition: 24 compiler tests, full workspace tests,
  Clippy with warnings denied, and the pinned-nightly ordinary-class Miri gate
  pass.

### 2026-08-30 — Complete-rooted lifecycle stages

- Replaced the local-base lifecycle shortcut with per-complete-class stage
  wrappers. For complete `D`, `A.init`/`A.deinit` now use a transparent
  `DData`-rooted `D-as-A` stage, while inline ordinary-class fields retain
  their own complete identities.
- Generated rooted activation/deactivation entry points recursively execute
  physical base stages without treating embedded base Data as a complete
  object. Rooted obligation guards preserve rollback and panic cleanup.
- Each lifecycle stage has its own descriptor: active identity is the current
  stage, complete-storage identity is the most-derived class, and its cast
  table contains only the active class and active ancestors. Abstract stage
  slots receive a non-returning trap rather than incorrectly dispatching to an
  inactive derived override.
- Runtime tests prove capped dispatch through nonvirtual helpers, distinct
  active/complete IDs in base stages, equal IDs in the complete stage, and
  rejection of lifecycle-invalid downcasts. Existing multiple-inheritance,
  inline, rollback, and unwind tests all pass through the new entry points.
- The ordinary-class Miri fixture now exercises rooted stage casts, dispatch,
  descriptors, and inactive-derived cast rejection for `Box` and `Rc` paths.
- Evidence: 24 compiler tests, workspace tests, Clippy with warnings denied,
  and `cargo +nightly-2026-08-30 miri run -p ordinary-class-demo` pass.

### 2026-08-30 — Model validation and typed source casts

- Added an independent exhaustive lifecycle-plan model over all 120
  five-class single-inheritance forests. For every class it checks the HIR
  activation order against recursive base-first construction and deactivation
  against its exact reverse.
- Method signature lowering now maps `&A`, `&mut A`, `Box<A>`, `Rc<A>`, and
  `Arc<A>` to generated view/owner capabilities, including nested signature
  traversal. Semantic validation rejects bare ordinary classes in method
  parameters and returns as movable values.
- Added HIR lowering for `expr as? Target` and `expr is Target` when `expr` is
  typed `self` or a method/constructor parameter. It preserves immutable and
  mutable borrows, consumes Box/Rc/Arc owners with failure recovery, validates
  target and operand kinds, and works in lifecycle-stage bodies.
- Runtime tests cover the full borrow/owner source-operator matrix, exact
  identity, sibling cross-cast, failure, and lifecycle-stage rejection of an
  inactive derived class. Compile-fail tests cover unknown targets, non-view
  operands, and movable method class values.
- General expression operands and local-variable type inference remain open;
  M6 therefore remains partial rather than overstating source coverage.
- Evidence: 28 compiler tests, workspace tests, Clippy with warnings denied,
  and the expanded pinned-nightly Miri gate pass.

### 2026-08-30 — Access invariants and Rust++ construction functions

- Added HIR member-access validation. A derived body may access its own private
  fields and public inherited fields, but private base fields are rejected and
  same-named fields inherited from sibling bases are diagnosed as ambiguous.
  Obsolete raw inherent method copies on `Data` were removed; all ordinary
  method bodies now use the single view/projection model.
- Public base edges support safe borrowed/owner upcasts; private/protected
  edges remain available inside legal class contexts but are not public view
  supertraits. Source `as?` checks statically known base paths using public,
  protected-descendant, and private-owner access rules.
- Compile-fail Rust evidence proves two mutable sibling views cannot coexist:
  both trait-object references borrow the complete storage root, not disjoint
  base Data. Additional tests prove nonpublic owner upcasts fail while internal
  inherited calls remain usable.
- Added lossless top-level Rust++ function items. Source functions now lower
  `construct Box<D>`, `Rc<D>`, and `Arc<D>` to stable owner factories, lower
  source capability annotations such as `Box<A>`, perform derived-to-base
  return coercions, and reject abstract construction targets.
- Added compiler-controlled direct-place lowering for
  `let d: D = construct D(args)`. Data is moved once into a final local
  `MaybeUninit` slot before activation. A live guard performs deactivation and
  a nested structural guard drops Data even if deinit unwinds. Exact-place
  annotation mismatches are rejected.
- Runtime coverage proves stable direct address, lifecycle order, constructor
  rollback, deinit-panic cleanup, and exact identity. The ordinary-class Miri
  fixture now executes a direct inline-containing `Parent` place as well as
  Box/Rc casts and lifecycle stages.
- Fixed the generated-artifact audit path: syntax is still parsed with `syn`,
  but emission no longer round-trips through a comment-dropping formatter.
  `SC-*` safety contracts are now present next to unsafe blocks in the actual
  generated files; the now-unused formatter dependency was removed.
- Evidence: 35 compiler tests, workspace tests, Clippy with warnings denied,
  AST-generation freshness, generated safety-comment inspection, and the
  pinned-nightly Miri gate pass.
- Direct-place bindings still need explicit source nonmovement/assignment
  diagnostics beyond the backend's hidden-slot representation, so M3 remains
  partial.

### 2026-08-30 — Direct-place nonmovement semantics

- Added source-use analysis for exact direct-place bindings. Moving a direct
  binding into another local or passing it to a value parameter is rejected
  before Rust emission with a place-bound diagnostic. Explicit shared/mutable
  borrows remain legal and are lowered as reborrows of the hidden live view,
  avoiding an accidental `&&dyn View` representation.
- Direct construction is accepted only as a local binding statement, retains
  exact target annotations, and cannot use a base annotation as polymorphic
  storage. The hidden `MaybeUninit` slot is never source-nameable or movable.
- Removed the unused Prettyplease dependency after changing emission to retain
  audited safety comments.
- Evidence: 36 compiler tests, full workspace tests, Clippy with warnings
  denied, AST-generation freshness, and the prior direct-place Miri gate pass.
- Direct-place assignment is rejected and lexical shadowing is now tracked, so
  an unrelated inner binding with the same spelling is not misclassified.
  Explicit-duplication rules and broader function-local type inference remain
  before M3 verification.

### 2026-08-30 — Recursive kinds, complete value classes, and interop audit

- Replaced substring-based type classification with recursive `syn::Type`
  semantics. Ordinary classes are rejected anywhere a movable value is
  required, including nested `Vec`/`Option`/`Result`/tuple payloads, method and
  function parameters/returns, and local annotations. `Box/Rc/Arc<class>` and
  borrowed class views remain movable capabilities at arbitrary nesting depth
  and lower recursively to generated owner/view types.
- Generalized balanced type parsing to tuples, arrays, parentheses, brackets,
  and generic angles. Added contextual `value` identifiers and correct Rust
  lifetime tokenization without confusing `'a` with a character literal.
- Value classes now support structural Rust Drop, type/lifetime/const generic
  parameters, bounds, defaults, and where clauses. Runtime tests prove
  ordinary moves and exact-once Drop; illegal value-class `deinit` and
  duplicate destructors are rejected.
- Rust-compatible source items now include traits and impls. Generic value
  classes implement ordinary Rust traits and opt-in Clone under normal Rust
  coherence. `impl Trait for C` on an ordinary class lowers to the polymorphic
  `dyn CView`, so derived virtual dispatch is preserved without exposing
  Storage or treating Rust traits as concrete bases.
- Generated internal names now separate runtime seals from per-class seals;
  the regression fixture uses a source class named `Object`.
- Compile-fail tests prove default polymorphic Box/Arc views do not
  accidentally claim Send or Sync from their current concrete backing.
- Added a generated-artifact invariant test: every compiler-emitted
  `unsafe { ... }` block must have a nearby `SAFETY [SC-*]` contract. Filled
  previously missing contracts on rollback guards, cast thunks, borrowed
  casts, and Box/Rc/Arc owner rebinding.
- Evidence: 44 compiler tests, workspace tests, Clippy with warnings denied,
  AST freshness, and the pinned-nightly Miri gate pass. M1 is implemented;
  final verification remains part of M9's exhaustive audit.

### 2026-08-30 — Versioned metadata and retained-owner cast expressions

- Added deterministic, versioned `.rppmeta` emission from the same HIR used by
  code generation. The artifact records source identity, value-class generic
  and field shape, ordinary-class IDs, flags, base visibility, fields,
  constructor parameters, method kinds/slots, and lifecycle presence.
- Class IDs now come from one shared ABI-versioned implementation used by both
  runtime descriptors and metadata. File compilation writes metadata beside
  generated Rust by default; the CLI also accepts an explicit metadata output
  path. A regression test verifies deterministic bytes and sibling emission.
- Extended typed view-operator lowering to the normative retained-owner form
  `a.clone() as? D`. Simple locals initialized by an immediately unwrapped cast
  retain the resulting borrow/owner capability, allowing subsequent `is` or
  `as?` operations without treating the class view as a movable class value.
  Runtime coverage verifies `Rc`/`Arc` strong counts and dynamic identity.
- Evidence: 45 compiler tests and all workspace tests pass; Clippy with
  warnings denied, AST-generation freshness, and
  `cargo +nightly-2026-08-30 miri run -p ordinary-class-demo` pass.
- Metadata import/compatibility checking, arbitrary expression typing,
  advertised auto-trait capability families, and the remaining M9 audit are
  still open; this is not yet complete-spec conformance.
- Tightened direct-place use analysis with lexical-scope paths. Reassignment
  remains a compile-time error, while an inner ordinary value may shadow the
  source name and the outer exact place becomes visible again after the block.

### 2026-08-30 — RTTI membership semantics audit

- Corrected source `expr is T` to follow §45: it now queries whether the
  currently active descriptor contains a legal `T` view, exactly matching cast
  success without consuming an owner. Exact-most-derived identity remains a
  distinct generated runtime query rather than being conflated with `is`.
- Runtime tests distinguish membership from exact identity for a multiply
  inherited `D`: an `A` view reports membership in `D`, `A`, and sibling `B`,
  while an exact `A` reports only `A`. During the `D` lifecycle stage, both
  `self is D` and `self is A` hold; the earlier `A` stage still rejects `D`.
- Added type-directed operator support for explicitly annotated local class
  borrows and lowered those local annotations to generated view types.
- Added member-expression operands for direct and uniquely inherited
  class-capability fields, including explicit `Rc`/`Arc` clones before owning
  casts. The declared field type determines the cast helper and access policy.
- Dynamic downcasts now check visibility along the reverse derived-to-base
  path, closing the private-inheritance hole for external `&A as? D`; `is`
  observes the same access rules as `as?`.
- Private ordinary-class methods now use compiler-private per-class traits
  implemented by live and lifecycle-stage backings. They remain absent from
  public view traits, while private virtual slots dispatch normally when live
  and remain capped to the active lifecycle stage. Inherited private and
  ambiguous sibling method calls receive Rust++ diagnostics before emission.
- Invalid cyclic graphs are tolerated by derived analyses after the primary
  graph diagnostic, preventing a compiler stack overflow during recovery.
- Audited roadmap items against normative/deferred wording. Expected-error
  constructor surface details (§105) and explicit object cloning (§106) are
  Part XIV, while §79 permits but does not mandate a bootstrap metadata-import
  syntax. Conservative views satisfy §78 by advertising no auto traits.
- Evidence: 46 compiler tests, all workspace tests, Clippy with warnings
  denied, AST freshness, and the pinned-nightly Miri fixture pass.

### 2026-08-30 — Relocatable ABI identity and agent-facing diagnostics

- Separated diagnostic source paths from logical ABI identity. The new
  `compile_source_with_identity` and `compile_file_with_identity` APIs, plus
  CLI `--abi-identity`, make class IDs independent of build-root paths.
  Default file compilation uses the input filename as its logical identity;
  multi-module/build integrations can and should supply a crate-qualified one.
- Class-ID hashing now length-prefixes ABI and class identities, preventing
  delimiter ambiguity. Equal logical identity and source produce byte-for-byte
  equal generated Rust and metadata after relocation; distinct identities
  produce distinct descriptors and metadata. A 4,096-pair generated corpus
  checks deterministic uniqueness, and emitted metadata is parsed as JSON in
  tests rather than validated only by substring inspection.
- Added stable diagnostic phase IDs: `RPP1001` lexical, `RPP2001` syntax,
  `RPP3001` semantic, `RPP4001` ABI configuration, `RPP9001` internal
  invariant, and `RPP9002` I/O. Rendered diagnostics and `Display` include the
  ID, giving autonomous revision agents durable machine-facing categories.
- Extended source `as?`/`is` operands to declared-capability free calls and
  `self` method calls, including borrowed lifetime-preserving results and
  owning `Rc` results. Helper selection remains type-directed from HIR return
  kinds, and owning calls do not gain an implicit clone. Transparent nested
  parentheses around identifiers and calls are consumed by the same operand
  span analysis and normalized before Rust emission.
- Evidence: 47 compiler tests, all workspace tests, Clippy with warnings
  denied, AST freshness, explicit CLI identity smoke test, and the pinned
  nightly Miri fixture pass.

### 2026-08-30 — Compound capabilities and lifecycle-epoch soundness

- Added type-directed `as?`/`is` operands for immutable and mutable
  borrow/deref compounds over stable owners. These select borrowed cast helpers
  and leave the owner unconsumed. Capability-returning calls now resolve
  through typed receiver parameters and uniquely inherited methods as well as
  `self` and free functions.
- Trait method declarations now normalize parameter binding patterns while
  implementations retain `mut`, fixing Rust's bodyless-trait-signature rule.
  Reference capability lowering now preserves explicit lifetimes such as
  `&'static A` and `&'a mut A`.
- A compile-fail epoch test exposed that direct raw-pointer stage-reference
  construction allowed Rust to infer an unconstrained, potentially `'static`
  lifetime. Replaced it with generated Data-to-stage helper functions whose
  single input/output reference lifetime is tied by Rust's function signature,
  for both local and complete-rooted lifecycle stages.
- Added direct size/alignment assertions for Data, live Storage, complete
  stage, and rooted ancestor-stage wrappers. An external-module test proves
  safe consumers can construct public owners but cannot name hidden Storage.
- Expanded the ledger with a requirement-by-requirement evidence matrix for
  all twenty §87 backend soundness obligations.
- Evidence: 49 compiler tests and all workspace tests pass; Clippy with
  warnings denied, AST freshness, and the pinned-nightly Miri fixture pass.

### 2026-08-30 — Private receiver dispatch and access closure

- Moved private ordinary-class methods behind per-class private supertraits of
  public views. This permits legal same-class calls through another typed
  `&A`, including private virtual dispatch to a derived override, while an
  external generated-bridge consumer still cannot name or invoke the method.
- Added frontend access analysis for typed receiver calls in class methods and
  top-level Rust++ functions. Private calls outside the declaring class are
  rejected before emission; public calls pass; multiply inherited same-name
  methods are diagnosed as ambiguous.
- Completed the self/typed-receiver matrix across own private, inherited
  private, public, ambiguous sibling, live virtual, and lifecycle-capped
  private virtual calls.
- With the §87 audit and receiver matrix green, M2–M5 and the §§47–63 lowering
  row move to implemented/verified. M6 remains partial only for compound
  expressions whose capability cannot yet be derived from an identifier,
  member, borrow/deref, annotation, or declared call result.
- Evidence remains 49 compiler tests, all workspace tests, Clippy with
  warnings denied, AST freshness, and the pinned-nightly Miri fixture.

### 2026-08-30 — Generic compound casts and conformance closure

- Completed parenthesized compound-operand lowering for `as?` and `is`.
  Expressions whose branch capabilities can be derived statically use the
  existing access-checked concrete helpers. Opaque expressions, including
  generic `FnOnce` results, use compiler-private traits implemented only for
  the public borrowed and Box/Rc/Arc source/target capability matrix. This
  retains the operand's borrow or owner kind and preserves failed owners.
- Kept access control ahead of generic dispatch: statically visible private or
  protected relationships are diagnosed by the frontend, and no public
  generic implementation is emitted for a nonpublic source/target path.
  Runtime coverage includes conditional immutable/mutable borrows, Box and Rc
  owners, membership tests, nested borrow/deref expressions, and opaque
  generic factories returning either `&A` or `Box<A>`.
- Generic class methods remain subject to Rust trait-object compatibility and
  are not used as a back door for compound typing; generic free functions
  provide the validation fixture while ordinary class views remain object
  safe.
- Audited the compact model and every core invariant in §§88–101 back to the
  detailed normative rows and the twenty-item §87 soundness matrix. M6 and M9
  are now implemented, and all Parts I–XIII conformance rows are implemented
  or verified. Parts XIV (§§102–108) remain explicitly deferred by the spec.
- Final evidence: 49 compiler tests and all workspace tests pass;
  `cargo clippy --workspace --all-targets -- -D warnings`, generated-AST
  freshness, and `cargo +nightly-2026-08-30 miri run -p
  ordinary-class-demo` are green.

### 2026-08-30 — Executable `.rpp` HTTP entry point

- Added `examples/http_server_demo`, a real loopback HTTP/1.1 program whose
  imports, application types, request handling, client integration check, and
  `fn main()` all live in `src/main.rpp`. The sole `.rs` source is Cargo's
  one-line generated-file inclusion shim.
- The example composes standard-library TCP and I/O, `Result`/`?`, threads,
  arrays and slices, a movable RAII `Fd` value class, a stable `Connection`
  owner, three-level server inheritance, inherited operations, mutable class
  views, and virtual HTTP dispatch through `TcpServerBox`.
- Practical compatibility findings are explicit: grouped top-level imports
  currently confuse the outer item scanner; `construct` lowering inside class
  method bodies is incomplete (the generated safe `construct_box` façade is
  used); and class-field expressions inside arbitrary macro token trees should
  first be bound to an ordinary local. These are frontend coverage gaps, not
  object-model or generated-runtime gaps.

### 2026-08-30 — Shared fully-active method emission

- Kept fully-active lifecycle-stage wrappers and live Storage as distinct Rust
  types: the former is an epoch-bounded borrow with no ownership/Drop role,
  while the latter owns the complete deactivation obligation. Their effective
  dynamic class is nevertheless identical.
- Factored each complete-class/target-view public and private method item set
  into one compiler-private `macro_rules!` definition. The fully-active stage
  and Storage trait impls now contain only macro instantiations, so method
  source is emitted once without merging the semantically distinct backing
  types. Rooted ancestor stages retain their own implementations because their
  capped RTTI and virtual-slot resolution genuinely differ.
- Added `fully_active_stage_and_storage_share_emitted_method_bodies`, which
  checks one body occurrence, two trait-impl instantiations, successful Rust
  compilation, and runtime dispatch. Final evidence is 50 compiler tests,
  workspace tests, warning-free Clippy, AST freshness, the HTTP loopback run,
  and pinned-nightly Miri.

### 2026-08-30 — Stateful object-view comparison

- Added four binaries under `examples/object_model_comparison`: Rust++ plus
  handwritten Rust query-interface, closed-enum, and unified-trait designs.
  All construct a toggle button, erase it
  behind a widget owner, mutate it through a clickable sibling view, inspect
  it through an accessible sibling view, recover the concrete owner, verify
  stable complete identity throughout, and produce identical output.
- The Rust++ source uses three concrete stateful bases plus general `is`,
  borrowed `as?`, owning `as?`, and implicit owner upcasting. The Rust
  alternatives expose their respective tradeoffs without treating them as
  mistakes: per-facet query/`Any` boilerplate, a centrally closed enum plus
  extra stable-owner indirection, or a concise central trait that removes
  independently discoverable capabilities.
- All four binaries run successfully with identical output; workspace tests,
  warning-free Clippy, and AST freshness remain green.
