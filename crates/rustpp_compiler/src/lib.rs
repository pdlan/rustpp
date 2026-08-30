mod ast;
mod codegen;
mod diagnostic;
mod hir;
mod lexer;
mod metadata;
mod parser;
pub mod syntax;

use std::fs;
use std::path::Path;

pub use diagnostic::{Diagnostic, DiagnosticCode, Severity, Span};

#[derive(Debug, Clone)]
pub struct Compilation {
    pub rust_source: String,
    pub metadata: String,
}

pub fn compile_source(source_name: &str, source: &str) -> Result<Compilation, Vec<Diagnostic>> {
    compile_source_with_identity(source_name, source_name, source)
}

pub fn compile_source_with_identity(
    _source_name: &str,
    abi_identity: &str,
    source: &str,
) -> Result<Compilation, Vec<Diagnostic>> {
    if abi_identity.is_empty() || abi_identity.chars().any(char::is_control) {
        return Err(vec![Diagnostic::abi_configuration(
            "ABI identity must be nonempty and contain no control characters",
        )]);
    }
    let parsed = parser::parse(source);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }

    let module = hir::lower(&parsed.syntax)?;
    let rust_source = codegen::emit(abi_identity, &module)?;
    let metadata = metadata::emit(abi_identity, &module);
    Ok(Compilation {
        rust_source,
        metadata,
    })
}

pub fn compile_file(input: &Path, output: &Path) -> Result<(), Vec<Diagnostic>> {
    let abi_identity = input.file_name().map_or_else(
        || input.to_string_lossy().into_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    compile_file_with_identity(input, output, &abi_identity)
}

pub fn compile_file_with_identity(
    input: &Path,
    output: &Path,
    abi_identity: &str,
) -> Result<(), Vec<Diagnostic>> {
    let source = fs::read_to_string(input).map_err(|error| {
        vec![Diagnostic::io(format!(
            "failed to read {}: {error}",
            input.display()
        ))]
    })?;
    let compilation =
        compile_source_with_identity(&input.display().to_string(), abi_identity, &source)?;
    fs::write(output, &compilation.rust_source).map_err(|error| {
        vec![Diagnostic::io(format!(
            "failed to write {}: {error}",
            output.display()
        ))]
    })?;
    let metadata_output = output.with_extension("rppmeta");
    fs::write(&metadata_output, &compilation.metadata).map_err(|error| {
        vec![Diagnostic::io(format!(
            "failed to write {}: {error}",
            metadata_output.display()
        ))]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const POINT: &str = r#"
value class Point {
    x: f64,
    pub labels: Vec<String>,

    constructor(x: f64, labels: Vec<String>) {
        new {
            x,
            labels: labels,
        }
    }
}
"#;

    #[test]
    fn diagnostics_have_stable_phase_ids() {
        let lexical = compile_source("lexical.rpp", "r###\"never closed").unwrap_err();
        assert_eq!(lexical[0].code, DiagnosticCode::LEXICAL);

        let syntax = compile_source("syntax.rpp", "value class").unwrap_err();
        assert_eq!(syntax[0].code, DiagnosticCode::SYNTAX);

        let semantic = compile_source(
            "semantic.rpp",
            "value class V { constructor() { new {} } } value class V { constructor() { new {} } }",
        )
        .unwrap_err();
        assert_eq!(semantic[0].code, DiagnosticCode::SEMANTIC);
        assert!(
            semantic[0]
                .render("semantic.rpp", "")
                .contains("error[RPP3001]")
        );

        let configuration = compile_source_with_identity("identity.rpp", "", POINT).unwrap_err();
        assert_eq!(configuration[0].code, DiagnosticCode::ABI_CONFIGURATION);

        let missing = std::env::temp_dir().join(format!(
            "rustpp-definitely-missing-{}-diagnostic.rpp",
            std::process::id()
        ));
        let io = compile_file(&missing, &missing.with_extension("rs")).unwrap_err();
        assert_eq!(io[0].code, DiagnosticCode::IO);
    }

    #[test]
    fn compiles_value_class_to_rust() {
        let output = compile_source("point.rpp", POINT).unwrap().rust_source;
        assert!(output.contains("pub struct Point"));
        assert!(output.contains("x: f64"));
        assert!(output.contains("pub labels: Vec < String >"));
        let compact: String = output.split_whitespace().collect();
        assert!(compact.contains("pubfnnew(x:f64,labels:Vec<String>)->Self"));
        assert!(output.contains("Self {\nx,\nlabels: labels,"));
        assert!(!output.contains("unsafe"));
    }

    #[test]
    fn value_class_structural_drop_runs_once_after_ordinary_moves() {
        let source = r#"
value class Resource {
    label: String,
    constructor(label: String) { new { label } }
    pub fn label(&self) -> &str { &self.label }
    destructor { drop { println!("drop {}", self.label); } }
}
"#;
        let generated = compile_source("value-drop.rpp", source)
            .unwrap()
            .rust_source;
        let output = compile_and_run(
            &generated,
            r#"
let first = Resource::new("value".to_owned());
let moved = first;
assert_eq!(moved.label(), "value");
let mut values = Vec::new();
values.push(moved);
drop(values);
"#,
        );
        assert_eq!(output, "drop value\n");

        let invalid = r#"
value class Bad {
    constructor() { new {} }
    destructor { deinit {} }
}
"#;
        let diagnostics = compile_source("value-deinit.rpp", invalid).unwrap_err();
        assert!(diagnostics.iter().any(|item| item.message
            == "value class `Bad` has no object lifecycle and cannot declare `deinit`"));

        let duplicate = r#"
value class Bad {
    constructor() { new {} }
    destructor { drop {} }
    destructor { drop {} }
}
"#;
        let diagnostics = compile_source("value-duplicate-drop.rpp", duplicate).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "value class `Bad` may declare at most one destructor")
        );
    }

    #[test]
    fn value_classes_support_type_lifetime_and_const_generics() {
        let source = r#"
value class Pair<T: Clone, U> {
    first: T,
    second: U,
    constructor(first: T, second: U) { new { first, second } }
    pub fn cloned_first(&self) -> T { self.first.clone() }
}
value class Borrowed<'a> {
    text: &'a str,
    constructor(text: &'a str) { new { text } }
    pub fn get(&self) -> &'a str { self.text }
}
value class Bytes<const N: usize> {
    bytes: [u8; N],
    constructor(bytes: [u8; N]) { new { bytes } }
    pub fn len(&self) -> usize { self.bytes.len() }
}
value class GenericDrop<T> {
    value: T,
    constructor(value: T) { new { value } }
    destructor { drop { println!("generic drop"); } }
}
value class WhereBound<T> where T: Clone {
    value: T,
    constructor(value: T) { new { value } }
    pub fn cloned(&self) -> T { self.value.clone() }
}
value class Defaulted<T = String> {
    value: T,
    constructor(value: T) { new { value } }
}
"#;
        let generated = compile_source("value-generics.rpp", source)
            .unwrap()
            .rust_source;
        let main = r#"
let pair = Pair::new(String::from("left"), 7_i32);
assert_eq!(pair.cloned_first(), "left");
let borrowed = Borrowed::new("borrowed");
assert_eq!(borrowed.get(), "borrowed");
let bytes = Bytes::new([1_u8, 2, 3]);
assert_eq!(bytes.len(), 3);
let moved = GenericDrop::new(vec![1, 2, 3]);
drop(moved);
let bounded = WhereBound::new(String::from("where"));
assert_eq!(bounded.cloned(), "where");
let _: Defaulted = Defaulted::new(String::from("default"));
"#;
        let output = compile_and_run(&generated, main);
        assert_eq!(output, "generic drop\n");

        let invalid = r#"
value class Bad<T {
    field: T,
    constructor(field: T) { new { field } }
}
"#;
        let diagnostics = compile_source("invalid-value-generics.rpp", invalid).unwrap_err();
        assert!(diagnostics.iter().any(|item| {
            item.message
                .contains("unclosed value-class generic parameter list")
        }));
    }

    #[test]
    fn value_classes_implement_real_rust_traits_with_normal_coherence() {
        let source = r#"
value class Label<T: std::fmt::Display> {
    value: T,
    constructor(value: T) { new { value } }
}

pub trait Summary { fn summary(&self) -> String; }

impl<T: std::fmt::Display> Summary for Label<T> {
    fn summary(&self) -> String { format!("label={}", self.value) }
}

impl<T: std::fmt::Display> std::fmt::Display for Label<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.value)
    }
}

impl<T: std::fmt::Display + Clone> Clone for Label<T> {
    fn clone(&self) -> Self { Self::new(self.value.clone()) }
}
"#;
        let generated = compile_source("value-traits.rpp", source)
            .unwrap()
            .rust_source;
        compile_and_run(
            &generated,
            r#"
let label = Label::new(42);
assert_eq!(label.summary(), "label=42");
assert_eq!(format!("{label}"), "42");
let duplicate = label.clone();
assert_eq!(duplicate.summary(), "label=42");
"#,
        );
    }

    #[test]
    fn ordinary_class_rust_traits_bridge_to_polymorphic_live_views() {
        let source = r#"
class Named {
    constructor() { new {} }
    pub virtual fn text(&self) -> &'static str { "base" }
}
class Derived : public Named {
    constructor() { new { base Named() } }
    pub override fn text(&self) -> &'static str { "derived" }
}

pub trait Describe { fn describe(&self) -> String; }

impl Describe for Named {
    fn describe(&self) -> String { format!("description={}", self.text()) }
}

impl std::fmt::Display for Named {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.text())
    }
}
"#;
        let generated = compile_source("ordinary-traits.rpp", source)
            .unwrap()
            .rust_source;
        assert!(generated.contains("impl Describe for dyn NamedView"));
        let main = r#"
let base = Named::construct_box();
assert_eq!(base.describe(), "description=base");
assert_eq!(format!("{base}"), "base");
let derived: NamedBox = Derived::construct_box();
assert_eq!(derived.describe(), "description=derived");
assert_eq!(format!("{derived}"), "derived");
"#;
        compile_and_run(&generated, main);
    }

    #[test]
    fn default_polymorphic_views_do_not_accidentally_claim_send_or_sync() {
        let source = r#"
class SafeToday { constructor() { new {} } }
"#;
        let generated = compile_source("auto-trait-defaults.rpp", source)
            .unwrap()
            .rust_source;
        let box_error = compile_expect_failure(
            &generated,
            "fn require_send<T: Send>(_: T) {} require_send(SafeToday::construct_box());",
        );
        assert!(
            box_error.contains("cannot be sent between threads safely"),
            "unexpected rustc diagnostic: {box_error}"
        );
        let arc_error = compile_expect_failure(
            &generated,
            "fn require_send_sync<T: Send + Sync>(_: T) {} require_send_sync(SafeToday::construct_arc());",
        );
        assert!(
            arc_error.contains("cannot be sent between threads safely")
                || arc_error.contains("cannot be shared between threads safely"),
            "unexpected rustc diagnostic: {arc_error}"
        );
    }

    #[test]
    fn every_compiler_generated_unsafe_block_has_a_nearby_safety_contract() {
        let source = r#"
class A { constructor() { new {} } }
class B { constructor() { new {} } }
class D : public A, public B { constructor() { new { base A(), base B() } } }
fn direct() { let value = construct D(); assert!(value is D); }
"#;
        let generated = compile_source("unsafe-audit.rpp", source)
            .unwrap()
            .rust_source;
        let lines: Vec<_> = generated.lines().collect();
        let unsafe_blocks: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains("unsafe {") || line.contains("unsafe {{"))
            .collect();
        assert!(!unsafe_blocks.is_empty());
        for (index, line) in unsafe_blocks {
            let start = index.saturating_sub(3);
            assert!(
                lines[start..=index]
                    .iter()
                    .any(|nearby| nearby.contains("SAFETY [SC-")),
                "generated unsafe block lacks a nearby contract at line {}: {line}",
                index + 1
            );
        }
    }

    #[test]
    fn transparent_lifecycle_layouts_match_and_safe_bridge_hides_storage() {
        let source = r#"
class A { value: u8, constructor() { new { value: 1 } } }
class D : public A {
    payload: u64,
    constructor() { new { base A(), payload: 2 } }
}
"#;
        let generated = compile_source("layout-contracts.rpp", source)
            .unwrap()
            .rust_source;
        compile_and_run(
            &generated,
            r#"
assert_eq!(std::mem::size_of::<__DData>(), std::mem::size_of::<__DStorage>());
assert_eq!(std::mem::align_of::<__DData>(), std::mem::align_of::<__DStorage>());
assert_eq!(std::mem::size_of::<__DData>(), std::mem::size_of::<__RppStage1>());
assert_eq!(std::mem::align_of::<__DData>(), std::mem::align_of::<__RppStage1>());
assert_eq!(std::mem::size_of::<__DData>(), std::mem::size_of::<__RppStage1As0>());
assert_eq!(std::mem::align_of::<__DData>(), std::mem::align_of::<__RppStage1As0>());
drop(D::construct_box());
"#,
        );

        let wrapped = format!("#[allow(dead_code)] mod generated {{\n{generated}\n}}");
        let stderr = compile_expect_failure(
            &wrapped,
            "let _ = std::mem::size_of::<generated::__DStorage>();",
        );
        assert!(
            stderr.contains("struct `__DStorage` is private") || stderr.contains("private struct"),
            "unexpected rustc diagnostic: {stderr}"
        );
        compile_and_run(&wrapped, "drop(generated::D::construct_box());");
    }

    #[test]
    fn class_views_cannot_escape_their_borrow_or_lifecycle_epoch() {
        let ordinary = r#"
class A {
    constructor() { new {} }
    pub fn leak(&self) -> &'static A { self }
}
"#;
        let generated = compile_source("borrow-escape.rpp", ordinary)
            .unwrap()
            .rust_source;
        let stderr = compile_expect_failure(&generated, "drop(A::construct_box());");
        assert!(
            stderr.contains("lifetime may not live long enough"),
            "unexpected rustc diagnostic: {stderr}"
        );

        let lifecycle = r#"
fn stash(_value: &'static A) {}
class A {
    constructor() {
        new {}
        init { stash(self); }
    }
}
"#;
        let generated = compile_source("stage-escape.rpp", lifecycle)
            .unwrap()
            .rust_source;
        let stderr = compile_expect_failure(&generated, "drop(A::construct_box());");
        assert!(
            stderr.contains("borrowed data escapes outside")
                || stderr.contains("lifetime may not live long enough"),
            "unexpected rustc diagnostic: {stderr}"
        );
    }

    #[test]
    fn metadata_is_versioned_deterministic_and_written_beside_generated_rust() {
        let source = r#"
value class Wrapper<T> {
    value: T,
    constructor(value: T) { new { value } }
}
class A {
    constructor() { new {} }
    pub virtual fn value(&self) -> i32 { 1 }
}
final class D : protected A {
    constructor() { new { base A() } }
    pub final override fn value(&self) -> i32 { 2 }
}
"#;
        let first = compile_source("crate/logical.rpp", source).unwrap();
        let second = compile_source("crate/logical.rpp", source).unwrap();
        assert_eq!(first.metadata, second.metadata);
        assert!(
            first
                .metadata
                .contains("\"abi_identity\": \"crate/logical.rpp\"")
        );
        let parsed: serde_json::Value = serde_json::from_str(&first.metadata).unwrap();
        assert_eq!(parsed["abi_version"], metadata::ABI_VERSION);
        assert_eq!(parsed["classes"][1]["name"], "D");
        assert!(
            first
                .metadata
                .contains("\"format\": \"rustpp-bootstrap-metadata\"")
        );
        assert!(first.metadata.contains("\"abi_version\": 1"));
        assert!(first.metadata.contains("\"generics\": \"< T > \""));
        assert!(first.metadata.contains("\"visibility\": \"protected\""));
        assert!(first.metadata.contains("\"kind\": \"final_override\""));
        assert!(first.metadata.contains("\"activation\": [\"A\", \"D\"]"));
        assert!(first.metadata.contains("\"deactivation\": [\"D\", \"A\"]"));
        assert!(first.metadata.contains("\"slot\": \"A:0\""));
        assert_ne!(
            first.metadata,
            compile_source("other/logical.rpp", source)
                .unwrap()
                .metadata
        );
        let relocated = compile_source_with_identity(
            "/different/build/root/logical.rpp",
            "crate/logical.rpp",
            source,
        )
        .unwrap();
        assert_eq!(first.rust_source, relocated.rust_source);
        assert_eq!(first.metadata, relocated.metadata);
        let distinct_identity =
            compile_source_with_identity("crate/logical.rpp", "other-crate/logical.rpp", source)
                .unwrap();
        assert_ne!(first.rust_source, distinct_identity.rust_source);
        assert_ne!(first.metadata, distinct_identity.metadata);
        assert!(compile_source_with_identity("logical.rpp", "", source).is_err());
        let mut ids = std::collections::HashSet::new();
        for crate_index in 0..128 {
            for class_index in 0..32 {
                let id = metadata::stable_class_id(
                    &format!("workspace/crate-{crate_index}"),
                    &format!("Class{class_index}"),
                );
                assert!(ids.insert(id), "class ID collision in generated corpus");
            }
        }

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("rustpp-metadata-test-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let input = directory.join("library.rpp");
        let rust_output = directory.join("library.rs");
        std::fs::write(&input, source).unwrap();
        compile_file(&input, &rust_output).unwrap();
        assert!(rust_output.exists());
        let metadata_output = rust_output.with_extension("rppmeta");
        assert_eq!(
            std::fs::read_to_string(metadata_output).unwrap(),
            compile_source_with_identity(&input.display().to_string(), "library.rpp", source)
                .unwrap()
                .metadata
        );
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn accepts_opaque_rust_expression() {
        let source = r#"
value class Number {
    result: i32,
    constructor(input: i32) { new { result: (input + call(2, 3)) * 4 } }
}
"#;
        let output = compile_source("number.rpp", source).unwrap().rust_source;
        assert!(output.contains("result: (input + call(2, 3)) * 4"));
    }

    #[test]
    fn reports_missing_and_unknown_initializers() {
        let source = r#"
value class Bad {
    expected: i32,
    constructor() { new { surprise: 1 } }
}
"#;
        let diagnostics = compile_source("bad.rpp", source).unwrap_err();
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|item| item.message.as_str())
            .collect();
        assert!(messages.contains(&"unknown field `surprise` in `new`"));
        assert!(messages.contains(&"field `expected` is missing from `new`"));
    }

    #[test]
    fn reports_duplicate_declarations() {
        let source = r#"
value class Duplicate {
    x: i32,
    x: i32,
    constructor(x: i32, x: i32) { new { x, x } }
}
"#;
        let diagnostics = compile_source("duplicate.rpp", source).unwrap_err();
        assert!(diagnostics.len() >= 3);
    }

    #[test]
    fn ordinary_class_runs_lifecycle_at_one_stable_address() {
        let source = r#"
class Stable {
    address: usize,
    constructor() {
        new { address: 0 }
        init {
            self.address = self as *const _ as usize;
            println!("init");
        }
    }
    destructor {
        deinit {
            assert_eq!(self.address, self as *const _ as usize);
            println!("deinit");
        }
        drop {
            assert_eq!(self.address, self as *const _ as usize);
            println!("drop");
        }
    }
}
"#;
        let generated = compile_source("stable.rpp", source).unwrap().rust_source;
        let output = compile_and_run(
            &generated,
            "let owner = Stable::construct_box(); let moved = owner; drop(moved);",
        );
        assert_eq!(output, "init\ndeinit\ndrop\n");
    }

    #[test]
    fn fully_active_stage_and_storage_share_emitted_method_bodies() {
        let source = r#"
class SharedMethods {
    constructor() { new {} init {} }
    pub virtual fn answer(&self) -> &'static str { "METHOD_BODY_ONCE" }
}
"#;
        let generated = compile_source("shared-methods.rpp", source)
            .unwrap()
            .rust_source;
        assert_eq!(generated.matches("METHOD_BODY_ONCE").count(), 1);
        assert_eq!(
            generated
                .matches("macro_rules! __rpp_view_methods_0_0")
                .count(),
            1
        );
        assert_eq!(generated.matches("__rpp_view_methods_0_0!();").count(), 2);
        compile_and_run(
            &generated,
            "let owner = SharedMethods::construct_box(); assert_eq!(owner.answer(), \"METHOD_BODY_ONCE\");",
        );
    }

    #[test]
    fn ordinary_class_rejects_duplicate_destructors() {
        let source = r#"
class Bad {
    constructor() { new {} }
    destructor {}
    destructor {}
}
"#;
        let diagnostics = compile_source("bad.rpp", source).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "class `Bad` may declare at most one destructor")
        );
    }

    #[test]
    fn all_stable_owner_kinds_preserve_identity_and_destroy_once() {
        let source = r#"
class Owned {
    address: usize,
    constructor() {
        new { address: 0 }
        init {
            self.address = self as *const _ as usize;
            println!("init");
        }
    }
    destructor {
        deinit {
            assert_eq!(self.address, self as *const _ as usize);
            println!("deinit");
        }
        drop { println!("drop"); }
    }
}
"#;
        let generated = compile_source("owned.rpp", source).unwrap().rust_source;
        let main = r#"
{
    let owner = Owned::construct_box();
    let moved = owner;
    drop(moved);
}
{
    let owner = Owned::construct_rc();
    let clone = owner.clone();
    assert_eq!(std::rc::Rc::strong_count(&owner), 2);
    drop(owner);
    drop(clone);
}
{
    let owner = Owned::construct_arc();
    let clone = owner.clone();
    assert_eq!(std::sync::Arc::strong_count(&owner), 2);
    drop(owner);
    drop(clone);
}
"#;
        let output = compile_and_run(&generated, main);
        assert_eq!(output, "init\ndeinit\ndrop\n".repeat(3));
    }

    #[test]
    fn panicking_init_drops_data_without_deinit() {
        let source = r#"
class Fails {
    field: Vec<i32>,
    constructor() {
        new { field: Vec::new() }
        init {
            println!("init");
            panic!("activation failed");
        }
    }
    destructor {
        deinit { println!("deinit"); }
        drop {
            assert!(self.field.is_empty());
            println!("drop");
        }
    }
}
"#;
        let generated = compile_source("fails.rpp", source).unwrap().rust_source;
        for constructor in [
            "Fails::construct_box()",
            "Fails::construct_rc()",
            "Fails::construct_arc()",
        ] {
            let main = format!(
                "let result = std::panic::catch_unwind(|| {{ let _owner = {constructor}; }}); assert!(result.is_err()); println!(\"caught\");"
            );
            let output = compile_and_run(&generated, &main);
            assert_eq!(output, "init\ndrop\ncaught\n");
        }
    }

    #[test]
    fn enforces_class_and_value_type_categories() {
        let invalid = r#"
class Object { constructor() { new {} } }
value class BadValue {
    objects: Vec<Object>,
    constructor(objects: Vec<Object>) { new { objects } }
}
"#;
        let diagnostics = compile_source("invalid-kinds.rpp", invalid).unwrap_err();
        assert!(diagnostics.iter().any(|item| {
            item.message
                .contains("cannot be stored in value class field `objects`")
                && item.span.is_some_and(|span| span.start > 0)
        }));

        let valid = r#"
class Object { constructor() { new {} } }
value class Capabilities {
    boxed: Box<Object>,
    constructor(boxed: Box<Object>) { new { boxed } }
}
class Consumer {
    constructor(object: &Object, owned: Rc<Object>) { new {} }
}
"#;
        let generated = compile_source("valid-kinds.rpp", valid)
            .unwrap()
            .rust_source;
        assert!(generated.contains("boxed: ObjectBox"));
    }

    #[test]
    fn recursive_value_kinds_accept_capabilities_and_reject_bare_class_payloads() {
        let valid = r#"
class Object { constructor() { new {} } }
value class CapabilitySet {
    boxed: Vec<Box<Object>>,
    shared: Option<Rc<Object>>,
    atomic: Result<Arc<Object>, String>,
    constructor(boxed: Vec<Box<Object>>, shared: Option<Rc<Object>>, atomic: Result<Arc<Object>, String>) {
        new { boxed, shared, atomic }
    }
}
class Container {
    owned: Vec<Box<Object>>,
    constructor(owned: Vec<Box<Object>>) { new { owned } }
    pub fn accepts(&self, value: Option<Box<Object>>) -> bool { value.is_some() }
}
"#;
        let generated = compile_source("recursive-valid-kinds.rpp", valid)
            .unwrap()
            .rust_source;
        assert!(generated.contains("Vec < ObjectBox >"));
        assert!(generated.contains("Option < ObjectRc >"));
        assert!(generated.contains("Result < ObjectArc , String >"));
        let main = r#"
let values = CapabilitySet::new(
    vec![Object::construct_box()],
    Some(Object::construct_rc()),
    Ok(Object::construct_arc()),
);
assert_eq!(values.boxed.len(), 1);
let container = Container::construct_box(vec![Object::construct_box()]);
assert!(container.accepts(Some(Object::construct_box())));
"#;
        compile_and_run(&generated, main);

        for (name, payload) in [
            ("option", "Option<Object>"),
            ("result", "Result<i32, Object>"),
            ("tuple", "(i32, Object)"),
            ("nested", "Vec<Option<Object>>"),
        ] {
            let source = format!(
                "class Object {{ constructor() {{ new {{}} }} }}\nvalue class Invalid {{ field: {payload}, constructor(field: {payload}) {{ new {{ field }} }} }}"
            );
            let diagnostics =
                compile_source(&format!("{name}-bare-class.rpp"), &source).unwrap_err();
            assert!(
                diagnostics.iter().any(|item| item
                    .message
                    .contains("cannot be stored in value class field `field`")),
                "{name}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn rustpp_function_types_obey_recursive_value_kinds() {
        let valid = r#"
class Object { constructor() { new {} } }
pub fn capability_count(values: Vec<Box<Object>>, shared: Option<Rc<Object>>) -> usize {
    values.len() + usize::from(shared.is_some())
}
"#;
        let generated = compile_source("function-capability-kinds.rpp", valid)
            .unwrap()
            .rust_source;
        assert!(generated.contains("Vec<ObjectBox>"));
        assert!(generated.contains("Option<ObjectRc>"));
        compile_and_run(
            &generated,
            "assert_eq!(capability_count(vec![Object::construct_box()], Some(Object::construct_rc())), 2);",
        );

        for source in [
            r#"class Object { constructor() { new {} } } fn bad(value: Vec<Object>) {}"#,
            r#"class Object { constructor() { new {} } } fn bad() -> Object { loop {} }"#,
            r#"class Object { constructor() { new {} } } fn bad() { let value: Option<Object> = None; drop(value); }"#,
        ] {
            let diagnostics = compile_source("function-invalid-kind.rpp", source).unwrap_err();
            assert!(diagnostics.iter().any(|item| item.message
                == "ordinary classes cannot appear in movable value positions in a Rust++ function; use a view, stable owner, or direct-place construction"));
        }
    }

    #[test]
    fn inline_classes_use_recursive_frames_and_two_pass_destruction() {
        let source = r#"
class Child {
    address: usize,
    constructor(seed: i32) {
        new { address: 0 }
        init {
            self.address = self as *const _ as usize;
            println!("child init {seed}");
        }
    }
    destructor {
        deinit {
            assert_eq!(self.address, self as *const _ as usize);
            println!("child deinit");
        }
        drop { println!("child drop"); }
    }
}
class Parent {
    child: Child,
    constructor() {
        new { child: construct Child(7) }
        init { println!("parent init"); }
    }
    destructor {
        deinit { println!("parent deinit"); }
        drop { println!("parent drop"); }
    }
}
"#;
        let generated = compile_source("inline.rpp", source).unwrap().rust_source;
        let output = compile_and_run(&generated, "drop(Parent::construct_box());");
        assert_eq!(
            output,
            "child init 7\nparent init\nparent deinit\nchild deinit\nparent drop\nchild drop\n"
        );
    }

    #[test]
    fn inline_activation_rolls_back_completed_children() {
        let source = r#"
class Child {
    constructor() { new {} init { println!("child init"); } }
    destructor {
        deinit { println!("child deinit"); }
        drop { println!("child drop"); }
    }
}
class Parent {
    child: Child,
    constructor() {
        new { child: construct Child() }
        init { println!("parent init"); panic!("parent failed"); }
    }
    destructor {
        deinit { println!("parent deinit"); }
        drop { println!("parent drop"); }
    }
}
"#;
        let generated = compile_source("inline-rollback.rpp", source)
            .unwrap()
            .rust_source;
        let main = "let result = std::panic::catch_unwind(|| { let _ = Parent::construct_box(); }); assert!(result.is_err());";
        let output = compile_and_run(&generated, main);
        assert_eq!(
            output,
            "child init\nparent init\nchild deinit\nparent drop\nchild drop\n"
        );
    }

    #[test]
    fn panicking_deinit_runs_remaining_object_and_structural_cleanup() {
        let source = r#"
class Child {
    constructor() { new {} }
    destructor { deinit { println!("child deinit"); } drop { println!("child drop"); } }
}
class Parent {
    child: Child,
    constructor() { new { child: construct Child() } }
    destructor {
        deinit { println!("parent deinit"); panic!("deinit failed"); }
        drop { println!("parent drop"); }
    }
}
"#;
        let generated = compile_source("deinit-unwind.rpp", source)
            .unwrap()
            .rust_source;
        let main = "let result = std::panic::catch_unwind(|| drop(Parent::construct_box())); assert!(result.is_err());";
        let output = compile_and_run(&generated, main);
        assert_eq!(
            output,
            "parent deinit\nchild deinit\nparent drop\nchild drop\n"
        );
    }

    #[test]
    fn multiple_bases_follow_declared_lifecycle_and_structural_order() {
        let source = r#"
class A {
    address: usize,
    constructor(seed: i32) {
        new { address: 0 }
        init {
            assert_eq!(seed, 1);
            self.address = self as *const _ as usize;
            println!("A init");
        }
    }
    destructor {
        deinit { assert_eq!(self.address, self as *const _ as usize); println!("A deinit"); }
        drop { println!("A drop"); }
    }
}
class B {
    constructor(seed: i32) { new {} init { assert_eq!(seed, 2); println!("B init"); } }
    destructor { deinit { println!("B deinit"); } drop { println!("B drop"); } }
}
class D : public A, protected B {
    constructor() {
        new { base A(1), base B(2) }
        init { println!("D init"); }
    }
    destructor { deinit { println!("D deinit"); } drop { println!("D drop"); } }
}
"#;
        let generated = compile_source("bases.rpp", source).unwrap().rust_source;
        let output = compile_and_run(&generated, "drop(D::construct_box());");
        assert_eq!(
            output,
            "A init\nB init\nD init\nD deinit\nB deinit\nA deinit\nD drop\nB drop\nA drop\n"
        );
    }

    #[test]
    fn rejects_cycles_and_repeated_concrete_bases() {
        let cycle = r#"
class A : B { constructor() { new { base B() } } }
class B : A { constructor() { new { base A() } } }
"#;
        let diagnostics = compile_source("cycle.rpp", cycle).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message.contains("inheritance cycle"))
        );

        let diamond = r#"
class A { constructor() { new {} } }
class B : A { constructor() { new { base A() } } }
class C : A { constructor() { new { base A() } } }
class D : B, C { constructor() { new { base B(), base C() } } }
"#;
        let diagnostics = compile_source("diamond.rpp", diamond).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "repeated concrete base `A` is not allowed")
        );
    }

    #[test]
    fn lifecycle_plans_match_an_exhaustive_small_forest_model() {
        fn enumerate(index: usize, parents: &mut [Option<usize>], checked: &mut usize) {
            if index == parents.len() {
                let mut source = String::new();
                for (class, parent) in parents.iter().copied().enumerate() {
                    if let Some(parent) = parent {
                        source.push_str(&format!(
                            "class C{class} : public C{parent} {{ constructor() {{ new {{ base C{parent}() }} }} }}\n"
                        ));
                    } else {
                        source.push_str(&format!(
                            "class C{class} {{ constructor() {{ new {{}} }} }}\n"
                        ));
                    }
                }
                let parsed = crate::parser::parse(&source);
                assert!(parsed.diagnostics.is_empty(), "{source:?}");
                let module = crate::hir::lower(&parsed.syntax).unwrap();
                for class in 0..parents.len() {
                    let mut expected = Vec::new();
                    let mut cursor = Some(class);
                    while let Some(current) = cursor {
                        expected.push(crate::hir::ClassId(current));
                        cursor = parents[current];
                    }
                    expected.reverse();
                    let activation: Vec<_> = module.classes[class]
                        .lifecycle
                        .activation
                        .iter()
                        .map(|step| match step {
                            crate::hir::LifecycleStep::ActivateClass(id) => *id,
                            crate::hir::LifecycleStep::DeactivateClass(_) => unreachable!(),
                        })
                        .collect();
                    assert_eq!(activation, expected, "parents={parents:?}, class={class}");
                    let deactivation: Vec<_> = module.classes[class]
                        .lifecycle
                        .deactivation
                        .iter()
                        .map(|step| match step {
                            crate::hir::LifecycleStep::DeactivateClass(id) => *id,
                            crate::hir::LifecycleStep::ActivateClass(_) => unreachable!(),
                        })
                        .collect();
                    expected.reverse();
                    assert_eq!(deactivation, expected, "parents={parents:?}, class={class}");
                }
                *checked += 1;
                return;
            }
            for parent in std::iter::once(None).chain((0..index).map(Some)) {
                parents[index] = parent;
                enumerate(index + 1, parents, checked);
            }
        }

        let mut parents = [None; 5];
        let mut checked = 0;
        enumerate(0, &mut parents, &mut checked);
        assert_eq!(checked, 120);
    }

    #[test]
    fn public_view_upcasts_preserve_complete_identity_and_owner() {
        let source = r#"
class A { constructor() { new {} } }
class D : public A { constructor() { new { base A() } } }
"#;
        let generated = compile_source("upcast.rpp", source).unwrap().rust_source;
        let main = r#"
let derived = D::construct_box();
let address = derived.__rpp_complete_address();
let base: ABox = derived;
assert_eq!(address, base.__rpp_complete_address());

let derived = D::construct_rc();
let address = derived.__rpp_complete_address();
let base: ARc = derived;
assert_eq!(address, base.__rpp_complete_address());
assert_eq!(std::rc::Rc::strong_count(&base), 1);

let derived = D::construct_arc();
let address = derived.__rpp_complete_address();
let base: AArc = derived;
assert_eq!(address, base.__rpp_complete_address());
assert_eq!(std::sync::Arc::strong_count(&base), 1);
"#;
        compile_and_run(&generated, main);
    }

    #[test]
    fn nonpublic_bases_are_internal_and_mutable_views_borrow_the_complete_root() {
        let source = r#"
class A {
    constructor() { new {} }
    pub fn inherited(&self) -> i32 { 7 }
}
class B { constructor() { new {} } }
class D : private A, public B {
    constructor() { new { base A(), base B() } }
    pub fn internal_base_call(&self) -> i32 { self.inherited() }
}
"#;
        let generated = compile_source("view-access.rpp", source)
            .unwrap()
            .rust_source;
        compile_and_run(
            &generated,
            "assert_eq!(D::construct_box().internal_base_call(), 7);",
        );

        let stderr = compile_expect_failure(
            &generated,
            "let derived = D::construct_box(); let _base: ABox = derived;",
        );
        assert!(
            stderr.contains("mismatched types") || stderr.contains("trait upcasting coercion"),
            "unexpected rustc diagnostic: {stderr}"
        );

        let mutable_source = r#"
class A { constructor() { new {} } }
class B { constructor() { new {} } }
class D : public A, public B { constructor() { new { base A(), base B() } } }
"#;
        let generated = compile_source("mutable-root.rpp", mutable_source)
            .unwrap()
            .rust_source;
        let stderr = compile_expect_failure(
            &generated,
            r#"
let mut derived = D::construct_box();
let a: &mut dyn AView = &mut *derived;
let b = __rpp_cast_mut_a_to_b(a).unwrap();
let _second: &mut dyn AView = &mut *derived;
assert_eq!(b.__rpp_complete_address(), 0);
"#,
        );
        assert!(
            stderr.contains("cannot borrow") && stderr.contains("more than once"),
            "unexpected rustc diagnostic: {stderr}"
        );
    }

    #[test]
    fn source_casts_enforce_private_and_protected_base_access() {
        let internal = r#"
class A { constructor() { new {} } }
class D : private A {
    constructor() { new { base A() } }
    pub fn can_view_base(&self) -> bool { (self as? A).is_some() }
    pub fn can_recover_derived(&self, value: &A) -> bool { (value as? D).is_some() }
}
class P : protected A { constructor() { new { base A() } } }
class E : public P {
    constructor() { new { base P() } }
    pub fn can_view_protected_base(&self) -> bool { (self as? A).is_some() }
}
"#;
        let generated = compile_source("internal-base-casts.rpp", internal)
            .unwrap()
            .rust_source;
        compile_and_run(
            &generated,
            "assert!(D::construct_box().can_view_base()); assert!(E::construct_box().can_view_protected_base());",
        );

        let external = r#"
class A { constructor() { new {} } }
class D : private A { constructor() { new { base A() } } }
class Outside {
    constructor() { new {} }
    pub fn forbidden(&self, value: &D) -> bool { (value as? A).is_some() }
}
"#;
        let diagnostics = compile_source("private-base-cast.rpp", external).unwrap_err();
        assert!(diagnostics.iter().any(
            |item| item.message == "class view `A` is not accessible from `D` in this context"
        ));

        let external_is = r#"
class A { constructor() { new {} } }
class D : private A { constructor() { new { base A() } } }
class Outside {
    constructor() { new {} }
    pub fn forbidden(&self, value: &D) -> bool { value is A }
}
"#;
        let diagnostics = compile_source("private-base-is.rpp", external_is).unwrap_err();
        assert!(diagnostics.iter().any(
            |item| item.message == "class view `A` is not accessible from `D` in this context"
        ));

        let external_downcast = r#"
class A { constructor() { new {} } }
class D : private A { constructor() { new { base A() } } }
class Outside {
    constructor() { new {} }
    pub fn forbidden(&self, value: &A) -> bool { (value as? D).is_some() }
}
"#;
        let diagnostics =
            compile_source("private-base-downcast.rpp", external_downcast).unwrap_err();
        assert!(diagnostics.iter().any(
            |item| item.message == "class view `D` is not accessible from `A` in this context"
        ));

        let external_conditional = r#"
class A { constructor() { new {} } }
class D : private A { constructor() { new { base A() } } }
class Outside {
    constructor() { new {} }
    pub fn forbidden(&self, choose_left: bool, left: &D, right: &D) -> bool {
        ((if choose_left { left } else { right }) as? A).is_some()
    }
}
"#;
        let diagnostics =
            compile_source("private-conditional-cast.rpp", external_conditional).unwrap_err();
        assert!(diagnostics.iter().any(
            |item| item.message == "class view `A` is not accessible from `D` in this context"
        ));

        let protected = r#"
class A { constructor() { new {} } }
class D : protected A { constructor() { new { base A() } } }
class Outside {
    constructor() { new {} }
    pub fn forbidden(&self, value: &D) -> bool { (value as? A).is_some() }
}
"#;
        let diagnostics = compile_source("protected-base-cast.rpp", protected).unwrap_err();
        assert!(diagnostics.iter().any(
            |item| item.message == "class view `A` is not accessible from `D` in this context"
        ));
    }

    #[test]
    fn method_signatures_lower_class_capabilities_to_views_and_owner_aliases() {
        let source = r#"
class A { constructor() { new {} } }
class D : public A { constructor() { new { base A() } } }
class Consumer {
    constructor() { new {} }
    pub fn borrowed(&self, value: &A) -> usize { value.__rpp_complete_address() }
    pub fn borrowed_mut(&self, value: &mut A) -> usize { value.__rpp_complete_address() }
    pub fn boxed(&self, value: Box<A>) -> usize { value.__rpp_complete_address() }
    pub fn shared(&self, value: Rc<A>) -> usize { value.__rpp_complete_address() }
    pub fn atomic(&self, value: Arc<A>) -> usize { value.__rpp_complete_address() }
}
"#;
        let generated = compile_source("method-capabilities.rpp", source)
            .unwrap()
            .rust_source;
        let main = r#"
let consumer = Consumer::construct_box();
let mut value: ABox = D::construct_box();
let address = value.__rpp_complete_address();
assert_eq!(consumer.borrowed(&*value), address);
assert_eq!(consumer.borrowed_mut(&mut *value), address);
assert_eq!(consumer.boxed(value), address);
let value: ARc = D::construct_rc();
let address = value.__rpp_complete_address();
assert_eq!(consumer.shared(value), address);
let value: AArc = D::construct_arc();
let address = value.__rpp_complete_address();
assert_eq!(consumer.atomic(value), address);
"#;
        compile_and_run(&generated, main);
    }

    #[test]
    fn source_view_operators_preserve_borrow_and_owner_capabilities() {
        let source = r#"
fn identity_a(value: &A) -> &A { value }
fn make_a() -> Rc<A> { construct Rc<D>() }
fn opaque_ref<'a, F: FnOnce() -> &'a A>(factory: F) -> bool {
    ((factory()) as? B).is_some()
}
fn opaque_box<F: FnOnce() -> Box<A>>(factory: F) -> bool {
    ((factory()) as? D).is_ok()
}
class A { constructor() { new {} } }
class B { constructor() { new {} } }
class D : public A, public B { constructor() { new { base A(), base B() } } }
class Holder {
    value: Rc<A>,
    constructor(value: Rc<A>) { new { value } }
    pub fn contains_b(&self) -> bool { (self.value.clone() as? B).is_ok() }
}
class BaseHolder {
    pub value: Rc<A>,
    constructor(value: Rc<A>) { new { value } }
    pub fn view(&self) -> &A { &*self.value }
}
class DerivedHolder : public BaseHolder {
    constructor(value: Rc<A>) { new { base BaseHolder(value) } }
    pub fn contains_b(&self) -> bool { (self.value.clone() as? B).is_ok() }
    pub fn inherited_call(&self) -> bool { (self.view() as? B).is_some() }
}
class Casts {
    constructor() { new {} }
    pub fn contains_d(&self, value: &A) -> bool { value is D }
    pub fn contains_a(&self, value: &A) -> bool { value is A }
    pub fn contains_b(&self, value: &A) -> bool { value is B }
    pub fn typed_local(&self, value: &A) -> bool {
        let local: &A = value;
        local is B
    }
    pub fn relay<'a>(&self, value: &'a A) -> &'a A { value }
    pub fn free_call(&self, value: &A) -> bool { (identity_a(value) as? B).is_some() }
    pub fn method_call(&self, value: &A) -> bool { (self.relay(value) as? B).is_some() }
    pub fn owning_call(&self) -> bool { (make_a() as? B).is_ok() }
    pub fn parenthesized(&self, value: &A) -> bool { ((value)) is B }
    pub fn parenthesized_call(&self, value: &A) -> bool {
        (((identity_a(value))) as? B).is_some()
    }
    pub fn deref_borrow(&self, value: Box<A>) -> bool { ((&*value) as? B).is_some() }
    pub fn deref_borrow_mut(&self, mut value: Box<A>) -> bool {
        ((&mut *value) as? D).is_some()
    }
    pub fn receiver_call(&self, holder: &BaseHolder) -> bool {
        (holder.view() as? B).is_some()
    }
    pub fn conditional_ref(&self, choose_left: bool, left: &A, right: &A) -> bool {
        ((if choose_left { left } else { right }) as? B).is_some()
    }
    pub fn conditional_mut<'a>(
        &self,
        choose_left: bool,
        left: &'a mut A,
        right: &'a mut A,
    ) -> bool {
        ((if choose_left { left } else { right }) as? D).is_some()
    }
    pub fn conditional_box(
        &self,
        choose_left: bool,
        left: Box<A>,
        right: Box<A>,
    ) -> bool {
        ((if choose_left { left } else { right }) as? D).is_ok()
    }
    pub fn conditional_rc(&self, choose_left: bool, left: Rc<A>, right: Rc<A>) -> bool {
        ((if choose_left { left } else { right }) as? B).is_ok()
    }
    pub fn conditional_is(&self, choose_left: bool, left: &A, right: &A) -> bool {
        (if choose_left { left } else { right }) is B
    }
    pub fn borrowed(&self, value: &A) -> bool { (value as? B).is_some() }
    pub fn borrowed_mut(&self, value: &mut A) -> bool { (value as? D).is_some() }
    pub fn boxed(&self, value: Box<A>) -> bool { (value as? D).is_ok() }
    pub fn shared(&self, value: Rc<A>) -> bool { (value as? B).is_ok() }
    pub fn atomic(&self, value: Arc<A>) -> bool { (value as? D).is_ok() }
    pub fn retained_shared(&self, value: Rc<A>) -> bool {
        let casted = (value.clone() as? D).ok().unwrap();
        std::rc::Rc::strong_count(&value) == 2 && casted is D
    }
    pub fn retained_atomic(&self, value: Arc<A>) -> bool {
        let casted = (value.clone() as? D).ok().unwrap();
        std::sync::Arc::strong_count(&value) == 2 && casted is D
    }
}
"#;
        let generated = compile_source("source-casts.rpp", source)
            .unwrap()
            .rust_source;
        let main = r#"
let casts = Casts::construct_box();
let holder = Holder::construct_box(D::construct_rc());
assert!(holder.contains_b());
let holder = DerivedHolder::construct_box(D::construct_rc());
assert!(holder.contains_b());
assert!(holder.inherited_call());
assert!(casts.receiver_call(&*holder));
let mut borrowed: ABox = D::construct_box();
let exact_for_condition = A::construct_box();
assert!(casts.conditional_ref(true, &*borrowed, &*exact_for_condition));
assert!(!casts.conditional_ref(false, &*borrowed, &*exact_for_condition));
assert!(casts.conditional_is(true, &*borrowed, &*exact_for_condition));
assert!(!casts.conditional_is(false, &*borrowed, &*exact_for_condition));
let mut left_mut: ABox = D::construct_box();
let mut right_mut: ABox = A::construct_box();
assert!(casts.conditional_mut(true, &mut *left_mut, &mut *right_mut));
assert!(casts.conditional_box(true, D::construct_box(), A::construct_box()));
assert!(casts.conditional_rc(true, D::construct_rc(), A::construct_rc()));
assert!(opaque_ref(|| &*borrowed));
assert!(opaque_box(|| D::construct_box()));
assert!(casts.contains_d(&*borrowed));
assert!(casts.contains_a(&*borrowed));
assert!(casts.contains_b(&*borrowed));
assert!(casts.typed_local(&*borrowed));
assert!(casts.free_call(&*borrowed));
assert!(casts.method_call(&*borrowed));
assert!(casts.owning_call());
assert!(casts.parenthesized(&*borrowed));
assert!(casts.parenthesized_call(&*borrowed));
assert!(casts.deref_borrow(D::construct_box()));
assert!(casts.deref_borrow_mut(D::construct_box()));
assert!(casts.borrowed(&*borrowed));
assert!(casts.borrowed_mut(&mut *borrowed));
assert!(casts.boxed(borrowed));
let shared: ARc = D::construct_rc();
assert!(casts.shared(shared));
let atomic: AArc = D::construct_arc();
assert!(casts.atomic(atomic));
let shared: ARc = D::construct_rc();
assert!(casts.retained_shared(shared));
let atomic: AArc = D::construct_arc();
assert!(casts.retained_atomic(atomic));
let exact = A::construct_box();
assert!(!casts.contains_d(&*exact));
assert!(casts.contains_a(&*exact));
assert!(!casts.contains_b(&*exact));
assert!(!casts.typed_local(&*exact));
assert!(!casts.free_call(&*exact));
assert!(!casts.method_call(&*exact));
assert!(!casts.parenthesized(&*exact));
assert!(!casts.parenthesized_call(&*exact));
assert!(!casts.borrowed(&*exact));
"#;
        compile_and_run(&generated, main);
    }

    #[test]
    fn source_view_operators_and_method_class_values_are_diagnosed() {
        let unknown_target = r#"
class A { constructor() { new {} } pub fn bad(&self) -> bool { self is Missing } }
"#;
        let diagnostics = compile_source("unknown-cast-target.rpp", unknown_target).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "unknown class-view cast target `Missing`")
        );

        let non_view = r#"
class A { constructor() { new {} } pub fn bad(&self, number: i32) -> bool { (number as? A).is_some() } }
"#;
        let diagnostics = compile_source("non-view-cast.rpp", non_view).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "`number` is not a class view or stable class owner")
        );

        let movable_class = r#"
class A { constructor() { new {} } }
class Bad { constructor() { new {} } pub fn move_it(&self, item: A) -> A { item } }
"#;
        let diagnostics = compile_source("method-class-value.rpp", movable_class).unwrap_err();
        assert!(diagnostics.iter().any(|item| item.message
            == "method `move_it` cannot pass or return an ordinary class as a movable value; use a view or stable owner"));
    }

    #[test]
    fn rustpp_functions_construct_all_stable_owner_kinds_and_upcast() {
        let source = r#"
class A { constructor() { new {} } }
class D : public A { constructor() { new { base A() } } }

pub fn boxed() -> Box<A> { construct Box<D>() }
pub fn shared() -> Rc<A> { construct Rc<D>() }
pub fn atomic() -> Arc<A> { construct Arc<D>() }
"#;
        let generated = compile_source("source-owner-construction.rpp", source)
            .unwrap()
            .rust_source;
        let main = r#"
let value = boxed(); assert!(__rpp_is_exact_d(&*value));
let value = shared(); assert!(__rpp_is_exact_d(&*value));
let value = atomic(); assert!(__rpp_is_exact_d(&*value));
"#;
        compile_and_run(&generated, main);

        let abstract_target = r#"
abstract class A {
    constructor() { new {} }
    pub virtual fn required(&self);
}
fn invalid() { let _ = construct Box<A>(); }
"#;
        let diagnostics = compile_source("abstract-construction.rpp", abstract_target).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "cannot construct abstract class `A`")
        );
    }

    #[test]
    fn rustpp_direct_places_activate_at_the_final_address_and_clean_up() {
        let source = r#"
class Direct {
    address: usize,
    constructor(seed: i32) {
        new { address: 0 }
        init {
            assert_eq!(seed, 9);
            self.address = self.__rpp_complete_address();
            println!("init");
        }
    }
    pub fn stable(&self) -> bool { self.address == self.__rpp_complete_address() }
    destructor {
        deinit { assert!(self.stable()); println!("deinit"); }
        drop { println!("drop"); }
    }
}

pub fn direct_scope() {
    let direct: Direct = construct Direct(9);
    assert!(direct is Direct);
    assert!(direct.stable());
    println!("body");
}
"#;
        let generated = compile_source("direct-place.rpp", source)
            .unwrap()
            .rust_source;
        assert!(generated.contains("SAFETY [SC-ACTIVATION-COMMIT]"));
        let output = compile_and_run(&generated, "direct_scope();");
        assert_eq!(output, "init\nbody\ndeinit\ndrop\n");

        let mismatch = r#"
class A { constructor() { new {} } }
class D : public A { constructor() { new { base A() } } }
fn invalid() { let place: A = construct D(); }
"#;
        let diagnostics = compile_source("inexact-direct-place.rpp", mismatch).unwrap_err();
        assert!(
            diagnostics.iter().any(|item| item.message
                == "direct object place `A` is exact and cannot hold constructed `D`"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn rustpp_direct_place_panic_rolls_back_and_structurally_drops() {
        let source = r#"
class Child {
    constructor() { new {} init { println!("child init"); } }
    destructor { deinit { println!("child deinit"); } drop { println!("child drop"); } }
}
class Parent {
    child: Child,
    constructor() {
        new { child: construct Child() }
        init { println!("parent init"); panic!("fail"); }
    }
    destructor { deinit { println!("parent deinit"); } drop { println!("parent drop"); } }
}
fn failing_direct() { let _parent = construct Parent(); }
"#;
        let generated = compile_source("direct-place-panic.rpp", source)
            .unwrap()
            .rust_source;
        let main =
            "let result = std::panic::catch_unwind(failing_direct); assert!(result.is_err());";
        let output = compile_and_run(&generated, main);
        assert_eq!(
            output,
            "child init\nparent init\nchild deinit\nparent drop\nchild drop\n"
        );
    }

    #[test]
    fn rustpp_direct_place_deinit_panic_still_structurally_drops() {
        let source = r#"
class Child {
    constructor() { new {} }
    destructor { deinit { println!("child deinit"); } drop { println!("child drop"); } }
}
class Parent {
    child: Child,
    constructor() { new { child: construct Child() } }
    destructor {
        deinit { println!("parent deinit"); panic!("fail"); }
        drop { println!("parent drop"); }
    }
}
fn failing_deinit() { let _parent = construct Parent(); }
"#;
        let generated = compile_source("direct-place-deinit-panic.rpp", source)
            .unwrap()
            .rust_source;
        let main =
            "let result = std::panic::catch_unwind(failing_deinit); assert!(result.is_err());";
        let output = compile_and_run(&generated, main);
        assert_eq!(
            output,
            "parent deinit\nchild deinit\nparent drop\nchild drop\n"
        );
    }

    #[test]
    fn direct_object_places_cannot_be_used_as_movable_values() {
        let moved = r#"
class D { constructor() { new {} } }
fn invalid() { let direct = construct D(); let moved = direct; drop(moved); }
"#;
        let diagnostics = compile_source("move-direct.rpp", moved).unwrap_err();
        assert!(diagnostics.iter().any(|item| item.message
            == "direct object place `direct` cannot be moved, assigned, passed, or returned by value"));

        let passed = r#"
class D { constructor() { new {} } }
fn consume<T>(_value: T) {}
fn invalid() { let direct = construct D(); consume(direct); }
"#;
        let diagnostics = compile_source("pass-direct.rpp", passed).unwrap_err();
        assert!(diagnostics.iter().any(|item| item.message
            == "direct object place `direct` cannot be moved, assigned, passed, or returned by value"));

        let assigned = r#"
class D { constructor() { new {} } }
fn invalid() { let mut direct = construct D(); direct = construct D(); }
"#;
        let diagnostics = compile_source("assign-direct.rpp", assigned).unwrap_err();
        assert!(diagnostics.iter().any(|item| item.message
            == "direct object place `direct` cannot be moved, assigned, passed, or returned by value"));

        let borrowed = r#"
class D { constructor() { new {} } }
fn inspect(_value: &D) {}
fn valid() { let direct = construct D(); inspect(&direct); }
"#;
        let generated = compile_source("borrow-direct.rpp", borrowed)
            .unwrap()
            .rust_source;
        compile_and_run(&generated, "valid();");

        let shadowed = r#"
class D { constructor() { new {} } }
fn valid() {
    let direct = construct D();
    { let direct = 7; assert_eq!(direct, 7); }
    let _view = &direct;
}
"#;
        let generated = compile_source("shadow-direct.rpp", shadowed)
            .unwrap()
            .rust_source;
        compile_and_run(&generated, "valid();");
    }

    #[test]
    fn virtual_overrides_dispatch_through_derived_and_base_views() {
        let source = r#"
class A {
    constructor() { new {} }
    pub virtual fn kind(&self) -> i32 { 1 }
    pub fn fixed(&self) -> i32 { self.kind() * 10 }
}
class D : public A {
    amount: i32,
    constructor(amount: i32) { new { base A(), amount } }
    pub override fn kind(&self) -> i32 { self.amount }
}
"#;
        let generated = compile_source("virtual.rpp", source).unwrap().rust_source;
        let main = r#"
let derived = D::construct_box(7);
assert_eq!(derived.kind(), 7);
assert_eq!(derived.fixed(), 70);
let base: ABox = derived;
assert_eq!(base.kind(), 7);
assert_eq!(base.fixed(), 70);
"#;
        compile_and_run(&generated, main);
    }

    #[test]
    fn abstract_slots_require_a_concrete_override() {
        let source = r#"
abstract class Abstract {
    constructor() { new {} }
    pub virtual fn answer(&self) -> i32;
}
class Concrete : public Abstract {
    constructor() { new { base Abstract() } }
    pub final override fn answer(&self) -> i32 { 42 }
}
"#;
        let generated = compile_source("abstract.rpp", source).unwrap().rust_source;
        assert!(!generated.contains("impl Abstract {"));
        compile_and_run(
            &generated,
            "let value: AbstractBox = Concrete::construct_box(); assert_eq!(value.answer(), 42);",
        );

        let invalid = r#"
class A { constructor() { new {} } pub virtual fn f(&self) -> i32 { 1 } }
class B : A { constructor() { new { base A() } } pub final override fn f(&self) -> i32 { 2 } }
class C : B { constructor() { new { base B() } } pub override fn f(&self) -> i32 { 3 } }
"#;
        let diagnostics = compile_source("final.rpp", invalid).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "cannot override final method `f`")
        );
    }

    #[test]
    fn class_modifiers_are_validated_semantically() {
        let implicit = r#"
class A {
    constructor() { new {} }
    pub virtual fn f(&self) -> i32;
}
"#;
        let diagnostics = compile_source("implicit-abstract.rpp", implicit).unwrap_err();
        assert!(diagnostics.iter().any(|item| item.message
            == "class `A` has unimplemented virtual slots and must be declared `abstract`"));

        let contradictory = r#"
abstract final class A {
    constructor() { new {} }
    pub virtual fn f(&self) -> i32;
}
"#;
        let diagnostics = compile_source("abstract-final.rpp", contradictory).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "class `A` cannot be both abstract and final")
        );

        let final_base = r#"
final class A { constructor() { new {} } }
class D : public A { constructor() { new { base A() } } }
"#;
        let diagnostics = compile_source("derive-final.rpp", final_base).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "cannot derive from final class `A`")
        );
    }

    #[test]
    fn inherited_field_access_respects_privacy_and_ambiguity() {
        let private = r#"
class A { secret: i32, constructor() { new { secret: 1 } } }
class D : public A {
    constructor() { new { base A() } }
    pub fn expose(&self) -> i32 { self.secret }
}
"#;
        let diagnostics = compile_source("private-field.rpp", private).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "field `secret` is private to base class `A`")
        );

        let ambiguous = r#"
class A { pub amount: i32, constructor() { new { amount: 1 } } }
class B { pub amount: i32, constructor() { new { amount: 2 } } }
class D : public A, public B {
    constructor() { new { base A(), base B() } }
    pub fn amount(&self) -> i32 { self.amount }
}
"#;
        let diagnostics = compile_source("ambiguous-field.rpp", ambiguous).unwrap_err();
        assert!(diagnostics.iter().any(
            |item| item.message == "inherited field `amount` is ambiguous between base classes"
        ));

        let public = r#"
class A { pub amount: i32, constructor() { new { amount: 7 } } }
class D : public A {
    constructor() { new { base A() } }
    pub fn amount(&self) -> i32 { self.amount }
}
"#;
        let generated = compile_source("public-field.rpp", public)
            .unwrap()
            .rust_source;
        compile_and_run(&generated, "assert_eq!(D::construct_box().amount(), 7);");

        let own_private_method = r#"
class A {
    constructor() { new {} }
    fn secret(&self) -> i32 { 9 }
    pub fn expose(&self) -> i32 { self.secret() }
}
"#;
        let generated = compile_source("own-private-method.rpp", own_private_method)
            .unwrap()
            .rust_source;
        compile_and_run(&generated, "assert_eq!(A::construct_box().expose(), 9);");

        let private_method = r#"
class A {
    constructor() { new {} }
    fn secret(&self) -> i32 { 1 }
}
class D : public A {
    constructor() { new { base A() } }
    pub fn expose(&self) -> i32 { self.secret() }
}
"#;
        let diagnostics = compile_source("private-method.rpp", private_method).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "method `secret` is private to base class `A`")
        );

        let ambiguous_method = r#"
class A { constructor() { new {} } pub fn number(&self) -> i32 { 1 } }
class B { constructor() { new {} } pub fn number(&self) -> i32 { 2 } }
class D : public A, public B {
    constructor() { new { base A(), base B() } }
    pub fn expose(&self) -> i32 { self.number() }
}
"#;
        let diagnostics = compile_source("ambiguous-method.rpp", ambiguous_method).unwrap_err();
        assert!(
            diagnostics.iter().any(|item| item.message
                == "inherited method `number` is ambiguous between base classes")
        );

        let typed_receiver_methods = r#"
class A { constructor() { new {} } pub fn number(&self) -> i32 { 1 } }
class B { constructor() { new {} } pub fn number(&self) -> i32 { 2 } }
class D : public A, public B { constructor() { new { base A(), base B() } } }
class Caller {
    constructor() { new {} }
    pub fn public_call(&self, value: &A) -> i32 { value.number() }
    pub fn ambiguous_call(&self, value: &D) -> i32 { value.number() }
}
"#;
        let diagnostics =
            compile_source("typed-receiver-methods.rpp", typed_receiver_methods).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "method `number` is ambiguous for receiver `value`")
        );

        let public_receiver = r#"
class A { constructor() { new {} } pub fn number(&self) -> i32 { 7 } }
class Caller {
    constructor() { new {} }
    pub fn call(&self, value: &A) -> i32 { value.number() }
}
"#;
        let generated = compile_source("public-receiver-method.rpp", public_receiver)
            .unwrap()
            .rust_source;
        compile_and_run(
            &generated,
            "assert_eq!(Caller::construct_box().call(&*A::construct_box()), 7);",
        );
    }

    #[test]
    fn private_virtual_methods_dispatch_internally_and_stay_lifecycle_capped() {
        let source = r#"
class A {
    constructor() {
        new {}
        init { assert_eq!(self.expose(), 1); }
    }
    virtual fn secret(&self) -> i32 { 1 }
    pub fn expose(&self) -> i32 { self.secret() }
    pub fn expose_other(&self, other: &A) -> i32 { other.secret() }
}
class D : public A {
    constructor() { new { base A() } }
    override fn secret(&self) -> i32 { 2 }
}
"#;
        let generated = compile_source("private-virtual.rpp", source)
            .unwrap()
            .rust_source;
        compile_and_run(
            &generated,
            r#"
assert_eq!(D::construct_box().expose(), 2);
let other: ABox = D::construct_box();
assert_eq!(D::construct_box().expose_other(&*other), 2);
"#,
        );
        let wrapped = format!("#[allow(dead_code)] mod generated {{\n{generated}\n}}");
        let stderr = compile_expect_failure(
            &wrapped,
            "let owner = generated::D::construct_box(); let _ = owner.secret();",
        );
        assert!(
            stderr.contains("private") || stderr.contains("no method named `secret`"),
            "unexpected rustc diagnostic: {stderr}"
        );

        let illegal = r#"
class A {
    constructor() { new {} }
    fn secret(&self) -> i32 { 1 }
}
class Outside {
    constructor() { new {} }
    pub fn reveal(&self, value: &A) -> i32 { value.secret() }
}
"#;
        let diagnostics = compile_source("private-receiver.rpp", illegal).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "method `secret` is private to class `A`")
        );

        let illegal_function = r#"
class A {
    constructor() { new {} }
    fn secret(&self) -> i32 { 1 }
}
fn reveal(value: &A) -> i32 { value.secret() }
"#;
        let diagnostics =
            compile_source("private-function-receiver.rpp", illegal_function).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|item| item.message == "method `secret` is private to class `A`")
        );
    }

    #[test]
    fn rtti_casts_rebind_borrows_and_all_owner_kinds_without_moving() {
        let source = r#"
class A { constructor() { new {} } }
class B { constructor() { new {} } }
class D : public A, public B {
    constructor() { new { base A(), base B() } }
    pub virtual fn marker(&self) -> i32 { 7 }
}
"#;
        let generated = compile_source("casts.rpp", source).unwrap().rust_source;
        let main = r#"
let derived = D::construct_box();
let address = derived.__rpp_complete_address();
let mut base: ABox = derived;
assert!(__rpp_is_exact_d(&*base));
assert!(!__rpp_is_exact_a(&*base));
let down = __rpp_cast_ref_a_to_d(&*base).unwrap();
assert_eq!(down.__rpp_complete_address(), address);
let sibling = __rpp_cast_ref_a_to_b(&*base).unwrap();
assert_eq!(sibling.__rpp_complete_address(), address);
let down_mut = __rpp_cast_mut_a_to_d(&mut *base).unwrap();
assert_eq!(down_mut.__rpp_complete_address(), address);
let derived = __rpp_cast_box_a_to_d(base).unwrap_or_else(|_| panic!("downcast failed"));
assert_eq!(derived.__rpp_complete_address(), address);

let exact_base = A::construct_box();
assert!(__rpp_is_exact_a(&*exact_base));
let address = exact_base.__rpp_complete_address();
let exact_base = match __rpp_cast_box_a_to_d(exact_base) {
    Err(owner) => owner,
    Ok(_) => panic!("cast unexpectedly succeeded"),
};
assert_eq!(exact_base.__rpp_complete_address(), address);

let derived: ARc = D::construct_rc();
let weak = std::rc::Rc::downgrade(&derived);
let address = derived.__rpp_complete_address();
let derived = __rpp_cast_rc_a_to_d(derived).unwrap_or_else(|_| panic!("Rc cast failed"));
assert_eq!(derived.__rpp_complete_address(), address);
assert_eq!(std::rc::Rc::strong_count(&derived), 1);
assert_eq!(std::rc::Rc::weak_count(&derived), 1);
assert!(weak.upgrade().is_some());

let derived: AArc = D::construct_arc();
let weak = std::sync::Arc::downgrade(&derived);
let address = derived.__rpp_complete_address();
let derived = __rpp_cast_arc_a_to_b(derived).unwrap_or_else(|_| panic!("Arc cross-cast failed"));
assert_eq!(derived.__rpp_complete_address(), address);
assert_eq!(std::sync::Arc::strong_count(&derived), 1);
assert_eq!(std::sync::Arc::weak_count(&derived), 1);
assert!(weak.upgrade().is_some());
"#;
        compile_and_run(&generated, main);
    }

    #[test]
    fn lifecycle_stage_views_cap_indirect_dispatch_and_active_rtti() {
        let source = r#"
class A {
    constructor() {
        new {}
        init {
            assert_eq!(self.helper(), 1);
            assert!(self is A);
            assert_ne!(self.__rpp_type_desc().active_class_id(), self.__rpp_type_desc().complete_storage_class_id());
            assert!((self as? D).is_none());
            println!("A init {}", self.helper());
        }
    }
    pub virtual fn kind(&self) -> i32 { 1 }
    pub fn helper(&self) -> i32 { self.kind() }
    destructor {
        deinit {
            assert_eq!(self.helper(), 1);
            assert!(self is A);
            assert_ne!(self.__rpp_type_desc().active_class_id(), self.__rpp_type_desc().complete_storage_class_id());
            assert!((self as? D).is_none());
            println!("A deinit {}", self.helper());
        }
    }
}
class D : public A {
    constructor() {
        new { base A() }
        init {
            assert_eq!(self.helper(), 2);
            assert!(self is D);
            assert!(self is A);
            assert_eq!(self.__rpp_type_desc().active_class_id(), self.__rpp_type_desc().complete_storage_class_id());
            println!("D init {}", self.helper());
        }
    }
    pub override fn kind(&self) -> i32 { 2 }
    destructor {
        deinit {
            assert_eq!(self.helper(), 2);
            assert!(self is D);
            assert!(self is A);
            assert_eq!(self.__rpp_type_desc().active_class_id(), self.__rpp_type_desc().complete_storage_class_id());
            println!("D deinit {}", self.helper());
        }
    }
}
"#;
        let generated = compile_source("stages.rpp", source).unwrap().rust_source;
        let output = compile_and_run(&generated, "drop(D::construct_box());");
        assert_eq!(output, "A init 1\nD init 2\nD deinit 2\nA deinit 1\n");
    }

    fn compile_and_run(generated: &str, main_body: &str) -> String {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("rustpp-test-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source_path = directory.join("main.rs");
        let binary_path = directory.join("test-bin");
        std::fs::write(
            &source_path,
            format!("{generated}\nfn main() {{ {main_body} }}\n"),
        )
        .unwrap();
        let compile = Command::new("rustc")
            .args(["--edition=2024", "-Dwarnings"])
            .arg(&source_path)
            .arg("-o")
            .arg(&binary_path)
            .output()
            .unwrap();
        assert!(
            compile.status.success(),
            "generated Rust failed to compile:\n{}\n{}",
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&binary_path).output().unwrap();
        assert!(
            run.status.success(),
            "generated program failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let _ = std::fs::remove_dir_all(directory);
        String::from_utf8(run.stdout).unwrap()
    }

    fn compile_expect_failure(generated: &str, main_body: &str) -> String {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("rustpp-fail-test-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let source_path = directory.join("main.rs");
        let binary_path = directory.join("test-bin");
        std::fs::write(
            &source_path,
            format!("{generated}\nfn main() {{ {main_body} }}\n"),
        )
        .unwrap();
        let compile = Command::new("rustc")
            .args(["--edition=2024", "-Dwarnings"])
            .arg(&source_path)
            .arg("-o")
            .arg(&binary_path)
            .output()
            .unwrap();
        assert!(compile.status.code().is_some_and(|code| code != 0));
        let stderr = String::from_utf8(compile.stderr).unwrap();
        let _ = std::fs::remove_dir_all(directory);
        stderr
    }
}
