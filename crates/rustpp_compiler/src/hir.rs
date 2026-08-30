use std::collections::{HashMap, HashSet};

use crate::ast::{self, AstNode};
use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::SyntaxNode;

#[derive(Debug, Clone)]
pub struct Module {
    pub value_classes: Vec<ValueClass>,
    pub classes: Vec<Class>,
    pub rust_items: Vec<RustItem>,
}

#[derive(Debug, Clone)]
pub struct RustItem {
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId {
    pub owner: ClassId,
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MethodId {
    pub owner: ClassId,
    pub index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    NonVirtual,
    Virtual,
    Override { final_: bool },
}

#[derive(Debug, Clone)]
pub struct Method {
    pub id: MethodId,
    pub name: String,
    pub signature: String,
    pub public: bool,
    pub kind: MethodKind,
    pub body: Option<String>,
    pub span: Span,
    pub slot: Option<MethodId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeKind {
    Value,
    ExactClass(ClassId),
    InvalidClassValue(ClassId),
    ClassOwner { owner: OwnerKind, class: ClassId },
    ClassBorrow { mutable: bool, class: ClassId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerKind {
    Box,
    Rc,
    Arc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef {
    pub source: String,
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleStep {
    ActivateClass(ClassId),
    DeactivateClass(ClassId),
}

#[derive(Debug, Clone)]
pub struct LifecyclePlan {
    pub activation: Vec<LifecycleStep>,
    pub deactivation: Vec<LifecycleStep>,
}

#[derive(Debug, Clone)]
pub struct ValueClass {
    pub id: ClassId,
    pub name: String,
    pub fields: Vec<Field>,
    pub constructor: Constructor,
    pub methods: Vec<Method>,
    pub drop_body: Option<String>,
    pub generics: ValueGenerics,
}

#[derive(Debug, Clone, Default)]
pub struct ValueGenerics {
    pub declaration: String,
    pub impl_params: String,
    pub type_args: String,
    pub where_clause: String,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub id: FieldId,
    pub name: String,
    pub ty: TypeRef,
    pub public: bool,
}

#[derive(Debug, Clone)]
pub struct Constructor {
    pub params: Vec<Param>,
    pub initializers: Vec<Initializer>,
    pub base_initializers: Vec<BaseInitializer>,
}

#[derive(Debug, Clone)]
pub struct BaseInitializer {
    pub class: ClassId,
    pub arguments: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseVisibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone)]
pub struct Base {
    pub class: ClassId,
    pub visibility: BaseVisibility,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone)]
pub struct Initializer {
    pub field: String,
    pub expression: Option<String>,
    pub inline: Option<InlineConstruction>,
}

#[derive(Debug, Clone)]
pub struct InlineConstruction {
    pub class: ClassId,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct Class {
    pub id: ClassId,
    pub name: String,
    pub fields: Vec<Field>,
    pub bases: Vec<Base>,
    pub constructor: Constructor,
    pub init_body: String,
    pub activation_params: Vec<Param>,
    pub deinit_body: String,
    pub drop_body: Option<String>,
    pub lifecycle: LifecyclePlan,
    pub methods: Vec<Method>,
    pub abstract_: bool,
    pub declared_abstract: bool,
    pub final_: bool,
}

fn lower_value_generics(
    source: Option<String>,
    span: Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> ValueGenerics {
    use quote::ToTokens;
    let Some(source) = source else {
        return ValueGenerics::default();
    };
    let generics = match syn::parse_str::<syn::ItemStruct>(&format!(
        "struct __RustppGenericProbe{source} {{}}"
    )) {
        Ok(item) => item.generics,
        Err(error) => {
            diagnostics.push(Diagnostic::error(
                format!("invalid value-class generics: {error}"),
                span,
            ));
            return ValueGenerics::default();
        }
    };
    let (impl_params, type_args, where_clause) = generics.split_for_impl();
    let mut declaration = generics.clone();
    declaration.where_clause = None;
    ValueGenerics {
        declaration: declaration.to_token_stream().to_string(),
        impl_params: impl_params.to_token_stream().to_string(),
        type_args: type_args.to_token_stream().to_string(),
        where_clause: where_clause.to_token_stream().to_string(),
    }
}

pub fn lower(root: &SyntaxNode) -> Result<Module, Vec<Diagnostic>> {
    let source = ast::SourceFile::new(root.clone()).expect("parser must produce SOURCE_FILE");
    let mut diagnostics = Vec::new();
    let mut value_classes = Vec::new();
    let mut classes = Vec::new();
    let mut class_names = HashSet::new();
    let mut unresolved_base_edges = HashMap::<ClassId, Vec<(String, BaseVisibility, Span)>>::new();
    let mut unresolved_base_initializers = HashMap::<ClassId, Vec<(String, String, Span)>>::new();
    let mut rust_items: Vec<RustItem> = source
        .rust_items()
        .map(|item| RustItem {
            source: item.source_text(),
        })
        .collect();

    for class in source.value_classes() {
        let Some(name_token) = class.name_token() else {
            continue;
        };
        let name = name_token.text().to_string();
        let generics = lower_value_generics(
            class
                .generic_params()
                .map(|params| params.syntax().text().to_string()),
            ast::span(class.syntax()),
            &mut diagnostics,
        );
        let class_id = ClassId(value_classes.len());
        if !class_names.insert(name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("duplicate value class `{name}`"),
                ast::token_span(&name_token),
            ));
        }

        let mut fields = Vec::new();
        let mut field_spans = HashMap::<String, Span>::new();
        for (field_index, field) in class.fields().enumerate() {
            let Some(field_name_token) = field.name_token() else {
                continue;
            };
            let field_name = field_name_token.text().to_string();
            if field_spans
                .insert(field_name.clone(), ast::token_span(&field_name_token))
                .is_some()
            {
                diagnostics.push(Diagnostic::error(
                    format!("duplicate field `{field_name}`"),
                    ast::token_span(&field_name_token),
                ));
            }
            let ty = field
                .ty()
                .map(|ty| ty.syntax().text().to_string())
                .unwrap_or_default();
            fields.push(Field {
                id: FieldId {
                    owner: class_id,
                    index: field_index,
                },
                name: field_name,
                ty: unresolved_type(&ty, field.ty().map(|ty| ast::span(ty.syntax()))),
                public: field.is_public(),
            });
        }

        let constructors: Vec<_> = class.constructors().collect();
        if constructors.len() != 1 {
            diagnostics.push(Diagnostic::error(
                format!("value class `{name}` must declare exactly one constructor"),
                ast::span(class.syntax()),
            ));
            continue;
        }
        let constructor = &constructors[0];
        let mut params = Vec::new();
        let mut param_names = HashSet::new();
        for param in constructor.params() {
            let Some(param_name_token) = param.name_token() else {
                continue;
            };
            let param_name = param_name_token.text().to_string();
            if !param_names.insert(param_name.clone()) {
                diagnostics.push(Diagnostic::error(
                    format!("duplicate constructor parameter `{param_name}`"),
                    ast::token_span(&param_name_token),
                ));
            }
            let ty = param
                .ty()
                .map(|ty| ty.syntax().text().to_string())
                .unwrap_or_default();
            params.push(Param {
                name: param_name,
                ty: unresolved_type(&ty, param.ty().map(|ty| ast::span(ty.syntax()))),
            });
        }

        let mut initializers = Vec::new();
        let mut initialized = HashSet::new();
        if let Some(new_expr) = constructor.new_expr() {
            for initializer in new_expr.fields() {
                let Some(field_token) = initializer.name_token() else {
                    continue;
                };
                let field_name = field_token.text().to_string();
                if !field_spans.contains_key(&field_name) {
                    diagnostics.push(Diagnostic::error(
                        format!("unknown field `{field_name}` in `new`"),
                        ast::token_span(&field_token),
                    ));
                }
                if !initialized.insert(field_name.clone()) {
                    diagnostics.push(Diagnostic::error(
                        format!("field `{field_name}` is initialized more than once"),
                        ast::token_span(&field_token),
                    ));
                }
                let expression = initializer
                    .expression()
                    .map(|expr| expr.syntax().text().to_string().trim().to_owned());
                initializers.push(Initializer {
                    field: field_name,
                    expression,
                    inline: None,
                });
            }
        }
        for field in &fields {
            if !initialized.contains(&field.name) {
                diagnostics.push(Diagnostic::error(
                    format!("field `{}` is missing from `new`", field.name),
                    *field_spans.get(&field.name).unwrap(),
                ));
            }
        }
        let methods = lower_methods(class.methods(), class_id, false, &mut diagnostics);
        let destructors: Vec<_> = class.destructors().collect();
        if destructors.len() > 1 {
            diagnostics.push(Diagnostic::error(
                format!("value class `{name}` may declare at most one destructor"),
                ast::span(class.syntax()),
            ));
        }
        let drop_body = destructors
            .first()
            .and_then(|destructor| destructor.drop_block())
            .and_then(|block| block.body())
            .map(|body| body.body_text());
        if destructors
            .first()
            .and_then(|destructor| destructor.deinit_block())
            .is_some()
        {
            diagnostics.push(Diagnostic::error(
                format!("value class `{name}` has no object lifecycle and cannot declare `deinit`"),
                ast::span(destructors[0].syntax()),
            ));
        }
        value_classes.push(ValueClass {
            id: class_id,
            name,
            fields,
            constructor: Constructor {
                params,
                initializers,
                base_initializers: Vec::new(),
            },
            methods,
            drop_body,
            generics,
        });
    }

    for class in source.classes() {
        let Some(name_token) = class.name_token() else {
            continue;
        };
        let name = name_token.text().to_string();
        let declared_abstract = class.is_abstract();
        let final_ = class.is_final();
        let class_id = ClassId(value_classes.len() + classes.len());
        if !class_names.insert(name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("duplicate class `{name}`"),
                ast::token_span(&name_token),
            ));
        }
        let base_edges = class
            .bases()
            .filter_map(|base| {
                let name = base.name_token()?;
                let visibility = match base.visibility_token().map(|token| token.kind()) {
                    Some(crate::syntax::SyntaxKind::PROTECTED_KW) => BaseVisibility::Protected,
                    Some(crate::syntax::SyntaxKind::PRIVATE_KW) => BaseVisibility::Private,
                    _ => BaseVisibility::Public,
                };
                Some((name.text().to_string(), visibility, ast::token_span(&name)))
            })
            .collect();
        unresolved_base_edges.insert(class_id, base_edges);

        let mut fields = Vec::new();
        let mut field_spans = HashMap::<String, Span>::new();
        for (field_index, field) in class.fields().enumerate() {
            let Some(field_name_token) = field.name_token() else {
                continue;
            };
            let field_name = field_name_token.text().to_string();
            if field_spans
                .insert(field_name.clone(), ast::token_span(&field_name_token))
                .is_some()
            {
                diagnostics.push(Diagnostic::error(
                    format!("duplicate field `{field_name}`"),
                    ast::token_span(&field_name_token),
                ));
            }
            let ty = field
                .ty()
                .map(|ty| ty.syntax().text().to_string())
                .unwrap_or_default();
            fields.push(Field {
                id: FieldId {
                    owner: class_id,
                    index: field_index,
                },
                name: field_name,
                ty: unresolved_type(&ty, field.ty().map(|ty| ast::span(ty.syntax()))),
                public: field.is_public(),
            });
        }

        let constructors: Vec<_> = class.constructors().collect();
        if constructors.len() != 1 {
            diagnostics.push(Diagnostic::error(
                format!("class `{name}` must declare exactly one constructor"),
                ast::span(class.syntax()),
            ));
            continue;
        }
        let constructor_node = &constructors[0];
        let mut params = Vec::new();
        let mut param_names = HashSet::new();
        for param in constructor_node.params() {
            let Some(param_name_token) = param.name_token() else {
                continue;
            };
            let param_name = param_name_token.text().to_string();
            if !param_names.insert(param_name.clone()) {
                diagnostics.push(Diagnostic::error(
                    format!("duplicate constructor parameter `{param_name}`"),
                    ast::token_span(&param_name_token),
                ));
            }
            let ty = param
                .ty()
                .map(|ty| ty.syntax().text().to_string())
                .unwrap_or_default();
            params.push(Param {
                name: param_name,
                ty: unresolved_type(&ty, param.ty().map(|ty| ast::span(ty.syntax()))),
            });
        }

        let mut initializers = Vec::new();
        let mut initialized = HashSet::new();
        if let Some(new_expr) = constructor_node.new_expr() {
            for initializer in new_expr.fields() {
                let Some(field_token) = initializer.name_token() else {
                    continue;
                };
                let field_name = field_token.text().to_string();
                if !field_spans.contains_key(&field_name) {
                    diagnostics.push(Diagnostic::error(
                        format!("unknown field `{field_name}` in `new`"),
                        ast::token_span(&field_token),
                    ));
                }
                if !initialized.insert(field_name.clone()) {
                    diagnostics.push(Diagnostic::error(
                        format!("field `{field_name}` is initialized more than once"),
                        ast::token_span(&field_token),
                    ));
                }
                initializers.push(Initializer {
                    field: field_name,
                    expression: initializer
                        .expression()
                        .map(|expr| expr.syntax().text().to_string().trim().to_owned()),
                    inline: None,
                });
            }
            let base_initializers = new_expr
                .bases()
                .filter_map(|base| {
                    let name = base.name_token()?;
                    let arguments = base
                        .arguments()
                        .map(|arguments| arguments.syntax().text().to_string())
                        .unwrap_or_default();
                    Some((
                        name.text().to_string(),
                        arguments.trim().to_owned(),
                        ast::token_span(&name),
                    ))
                })
                .collect();
            unresolved_base_initializers.insert(class_id, base_initializers);
        }
        for field in &fields {
            if !initialized.contains(&field.name) {
                diagnostics.push(Diagnostic::error(
                    format!("field `{}` is missing from `new`", field.name),
                    *field_spans.get(&field.name).unwrap(),
                ));
            }
        }

        let destructors: Vec<_> = class.destructors().collect();
        if destructors.len() > 1 {
            diagnostics.push(Diagnostic::error(
                format!("class `{name}` may declare at most one destructor"),
                ast::span(class.syntax()),
            ));
        }
        let init_body = constructor_node
            .init_block()
            .and_then(|block| block.body())
            .map(|block| block.body_text())
            .unwrap_or_default();
        let deinit_body = destructors
            .first()
            .and_then(|destructor| destructor.deinit_block())
            .and_then(|block| block.body())
            .map(|block| block.body_text())
            .unwrap_or_default();
        let drop_body = destructors
            .first()
            .and_then(|destructor| destructor.drop_block())
            .and_then(|block| block.body())
            .map(|block| block.body_text());
        let activation_params = params
            .iter()
            .filter(|param| body_mentions_identifier(&init_body, &param.name))
            .cloned()
            .collect();
        let methods = lower_methods(class.methods(), class_id, true, &mut diagnostics);
        classes.push(Class {
            id: class_id,
            name,
            fields,
            bases: Vec::new(),
            constructor: Constructor {
                params,
                initializers,
                base_initializers: Vec::new(),
            },
            init_body,
            activation_params,
            deinit_body,
            drop_body,
            lifecycle: LifecyclePlan {
                activation: vec![LifecycleStep::ActivateClass(class_id)],
                deactivation: vec![LifecycleStep::DeactivateClass(class_id)],
            },
            methods,
            abstract_: false,
            declared_abstract,
            final_,
        });
    }

    resolve_type_kinds(&mut value_classes, &mut classes, &mut diagnostics);
    resolve_base_graph(
        &mut classes,
        &unresolved_base_edges,
        &unresolved_base_initializers,
        &mut diagnostics,
    );
    validate_member_accesses(&classes, &mut diagnostics);
    validate_typed_receiver_method_accesses(&classes, &rust_items, &mut diagnostics);
    validate_method_slots(&mut classes, &mut diagnostics);
    resolve_inline_constructions(&mut classes, &mut diagnostics);
    lower_class_view_operators(
        &mut value_classes,
        &mut classes,
        &rust_items,
        &mut diagnostics,
    );
    lower_rust_owner_constructions(&mut rust_items, &classes, &mut diagnostics);

    if diagnostics.is_empty() {
        Ok(Module {
            value_classes,
            classes,
            rust_items,
        })
    } else {
        Err(diagnostics)
    }
}

fn lower_rust_owner_constructions(
    items: &mut [RustItem],
    classes: &[Class],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let by_name: HashMap<_, _> = classes
        .iter()
        .map(|class| (class.name.as_str(), class))
        .collect();
    let class_ids: HashMap<_, _> = classes
        .iter()
        .map(|class| (class.name.clone(), class.id))
        .collect();
    let class_names: HashMap<_, _> = classes
        .iter()
        .map(|class| (class.id, class.name.clone()))
        .collect();
    let base_edges: HashMap<_, _> = classes
        .iter()
        .map(|class| (class.id, class.bases.clone()))
        .collect();
    let method_returns: HashMap<_, _> = classes
        .iter()
        .map(|class| (class.id, method_return_kinds(&class.methods, &class_ids)))
        .collect();
    let callable_returns: HashMap<_, _> = items
        .iter()
        .filter_map(|item| rust_item_function_return_kind(&item.source, &class_ids))
        .collect();
    for item in items {
        validate_rust_direct_place_uses(&item.source, &class_ids, diagnostics);
        let mut environment = callable_returns.clone();
        environment.extend(rust_item_construction_bindings(&item.source, &class_ids));
        item.source = lower_view_operators_in_body(
            &item.source,
            &environment,
            ViewOperatorContext {
                self_kind: None,
                class_ids: &class_ids,
                class_names: &class_names,
                base_edges: &base_edges,
                method_returns: &method_returns,
                access_context: None,
                diagnostic_span: Span::new(0, item.source.len()),
            },
            diagnostics,
        );
        let (tokens, lexical_diagnostics) = crate::lexer::lex(&item.source);
        diagnostics.extend(lexical_diagnostics);
        let significant: Vec<_> = tokens
            .iter()
            .filter(|token| !token.kind.is_trivia())
            .collect();
        let text = |token: &crate::lexer::Token| &item.source[token.span.start..token.span.end];
        let mut replacements = Vec::new();
        for window in significant.windows(6) {
            if text(window[0]) != "construct"
                || !matches!(text(window[1]), "Box" | "Rc" | "Arc")
                || window[2].kind != crate::syntax::SyntaxKind::L_ANGLE
                || window[3].kind != crate::syntax::SyntaxKind::IDENT
                || window[4].kind != crate::syntax::SyntaxKind::R_ANGLE
                || window[5].kind != crate::syntax::SyntaxKind::L_PAREN
            {
                continue;
            }
            let target = text(window[3]);
            let Some(class) = by_name.get(target).copied() else {
                diagnostics.push(Diagnostic::error(
                    format!("unknown ordinary construction class `{target}`"),
                    window[3].span,
                ));
                continue;
            };
            if class.abstract_ {
                diagnostics.push(Diagnostic::error(
                    format!("cannot construct abstract class `{target}`"),
                    window[3].span,
                ));
                continue;
            }
            replacements.push((
                window[0].span.start,
                window[4].span.end,
                format!("{target}::construct_{}", text(window[1]).to_lowercase()),
            ));
        }
        let mut direct_index = 0usize;
        let mut direct_bindings = HashSet::new();
        let mut index = 0usize;
        while index < significant.len() {
            if text(significant[index]) != "let" {
                index += 1;
                continue;
            }
            let mut cursor = index + 1;
            let mutable = significant
                .get(cursor)
                .is_some_and(|token| text(token) == "mut");
            if mutable {
                cursor += 1;
            }
            let Some(binding) = significant.get(cursor) else {
                break;
            };
            if !matches!(
                binding.kind,
                crate::syntax::SyntaxKind::IDENT | crate::syntax::SyntaxKind::VALUE_KW
            ) {
                index += 1;
                continue;
            }
            cursor += 1;
            let annotation = if significant
                .get(cursor)
                .is_some_and(|token| token.kind == crate::syntax::SyntaxKind::COLON)
            {
                cursor += 1;
                let Some(annotation) = significant.get(cursor) else {
                    break;
                };
                cursor += 1;
                Some(*annotation)
            } else {
                None
            };
            if !significant
                .get(cursor)
                .is_some_and(|token| text(token) == "=")
                || !significant
                    .get(cursor + 1)
                    .is_some_and(|token| text(token) == "construct")
                || !significant
                    .get(cursor + 2)
                    .is_some_and(|token| token.kind == crate::syntax::SyntaxKind::IDENT)
                || !significant
                    .get(cursor + 3)
                    .is_some_and(|token| token.kind == crate::syntax::SyntaxKind::L_PAREN)
            {
                index += 1;
                continue;
            }
            let target_token = significant[cursor + 2];
            let target = text(target_token);
            let Some(class) = by_name.get(target).copied() else {
                diagnostics.push(Diagnostic::error(
                    format!("unknown ordinary construction class `{target}`"),
                    target_token.span,
                ));
                index += 1;
                continue;
            };
            if class.abstract_ {
                diagnostics.push(Diagnostic::error(
                    format!("cannot construct abstract class `{target}`"),
                    target_token.span,
                ));
                index += 1;
                continue;
            }
            if let Some(annotation) = annotation
                && text(annotation) != target
            {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "direct object place `{}` is exact and cannot hold constructed `{target}`",
                        text(annotation)
                    ),
                    annotation.span,
                ));
                index += 1;
                continue;
            }
            let open_index = cursor + 3;
            let mut depth = 0usize;
            let mut close_index = None;
            for (candidate, token) in significant.iter().enumerate().skip(open_index) {
                match token.kind {
                    crate::syntax::SyntaxKind::L_PAREN => depth += 1,
                    crate::syntax::SyntaxKind::R_PAREN => {
                        depth -= 1;
                        if depth == 0 {
                            close_index = Some(candidate);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(close_index) = close_index else {
                diagnostics.push(Diagnostic::error(
                    "unclosed direct-place construction arguments",
                    target_token.span,
                ));
                break;
            };
            let Some(semicolon) = significant.get(close_index + 1) else {
                diagnostics.push(Diagnostic::error(
                    "direct-place construction must be a local binding statement",
                    target_token.span,
                ));
                break;
            };
            if semicolon.kind != crate::syntax::SyntaxKind::SEMICOLON {
                diagnostics.push(Diagnostic::error(
                    "direct-place construction must be a local binding statement",
                    target_token.span,
                ));
                index = close_index + 1;
                continue;
            }
            let arguments =
                &item.source[significant[open_index].span.end..significant[close_index].span.start];
            let unique = direct_index;
            direct_index += 1;
            let binding_name = text(binding);
            direct_bindings.insert(binding_name.to_owned());
            let view_mutability = if mutable { "mut " } else { "" };
            let binding_mutability = if mutable { "mut " } else { "" };
            let replacement = format!(
                "let (__rpp_direct_data_{unique}, __rpp_direct_frame_{unique}) = __{target}Data::__rpp_new({arguments});\n\
                 let mut __rpp_direct_slot_{unique} = std::mem::MaybeUninit::new(__rpp_direct_data_{unique});\n\
                 let mut __rpp_direct_guard_{unique} = __RppDirectGuard{} {{ data: __rpp_direct_slot_{unique}.as_mut_ptr(), live: false }};\n\
                 // SAFETY [SC-ACTIVATION-COMMIT]: Data is in its final compiler-owned direct slot before activation and the guard owns rollback/structural cleanup.\n\
                 unsafe {{ (*__rpp_direct_guard_{unique}.data).__rpp_init_complete(__rpp_direct_frame_{unique}); }}\n\
                 __rpp_direct_guard_{unique}.live = true;\n\
                 let {binding_mutability}{binding_name}: &{view_mutability}dyn {target}View = {{\n\
                 // SAFETY [SC-DATA-STORAGE-LAYOUT, SC-ACTIVATION-COMMIT]: successful activation permits a live transparent Storage view at the unchanged direct-slot address.\n\
                 unsafe {{ &{view_mutability}*(__rpp_direct_guard_{unique}.data.cast::<__{target}Storage>()) }}\n\
                 }};",
                class.id.0
            );
            replacements.push((
                significant[index].span.start,
                semicolon.span.end,
                replacement,
            ));
            index = close_index + 2;
        }
        for (start, end, replacement) in replacements.into_iter().rev() {
            item.source.replace_range(start..end, &replacement);
        }
        lower_direct_place_borrows(&mut item.source, &direct_bindings);
        lower_capability_types(&mut item.source, &class_ids);
        lower_ordinary_class_trait_impl_target(&mut item.source, &class_ids);
        validate_rust_item_value_kinds(&item.source, &class_ids, diagnostics);
    }
}

fn rust_item_function_return_kind(
    source: &str,
    classes: &HashMap<String, ClassId>,
) -> Option<(String, TypeKind)> {
    use quote::ToTokens;
    let (tokens, _) = crate::lexer::lex(source);
    let open = tokens
        .iter()
        .find(|token| token.kind == crate::syntax::SyntaxKind::L_BRACE)?;
    let mut signature = source[..open.span.start].trim();
    if let Some(rest) = signature.strip_prefix("pub ") {
        signature = rest;
    }
    let signature = syn::parse_str::<syn::Signature>(signature).ok()?;
    let syn::ReturnType::Type(_, ty) = signature.output else {
        return None;
    };
    let kind = classify_type(&ty.to_token_stream().to_string(), classes);
    matches!(
        kind,
        TypeKind::ClassBorrow { .. } | TypeKind::ClassOwner { .. }
    )
    .then(|| (signature.ident.to_string(), kind))
}

fn lower_ordinary_class_trait_impl_target(source: &mut String, classes: &HashMap<String, ClassId>) {
    let (tokens, _) = crate::lexer::lex(source);
    let significant: Vec<_> = tokens
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let text = |token: &crate::lexer::Token| &source[token.span.start..token.span.end];
    let Some(impl_index) = significant.iter().position(|token| text(token) == "impl") else {
        return;
    };
    let Some(for_index) = significant
        .iter()
        .enumerate()
        .skip(impl_index + 1)
        .find_map(|(index, token)| (text(token) == "for").then_some(index))
    else {
        return;
    };
    let Some(target) = significant.get(for_index + 1) else {
        return;
    };
    let target_name = text(target);
    if classes.contains_key(target_name) {
        source.replace_range(
            target.span.start..target.span.end,
            &format!("dyn {target_name}View"),
        );
    }
}

fn validate_rust_item_value_kinds(
    source: &str,
    classes: &HashMap<String, ClassId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use quote::ToTokens;
    use syn::visit::Visit;

    struct KindVisitor<'a> {
        classes: &'a HashMap<String, ClassId>,
        invalid: bool,
    }

    impl<'ast> Visit<'ast> for KindVisitor<'_> {
        fn visit_type(&mut self, ty: &'ast syn::Type) {
            if matches!(
                classify_type(&ty.to_token_stream().to_string(), self.classes),
                TypeKind::ExactClass(_) | TypeKind::InvalidClassValue(_)
            ) {
                self.invalid = true;
            }
            // `classify_type` recursively checks the complete type expression,
            // so descending would duplicate one source error for every nested node.
        }
    }

    let Ok(item) = syn::parse_str::<syn::Item>(source) else {
        return;
    };
    let mut visitor = KindVisitor {
        classes,
        invalid: false,
    };
    visitor.visit_item(&item);
    if visitor.invalid {
        diagnostics.push(Diagnostic::error(
            "ordinary classes cannot appear in movable value positions in a Rust++ function; use a view, stable owner, or direct-place construction",
            Span::new(0, source.len()),
        ));
    }
}

fn lower_direct_place_borrows(source: &mut String, bindings: &HashSet<String>) {
    let (tokens, _) = crate::lexer::lex(source);
    let significant: Vec<_> = tokens
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let text = |token: &crate::lexer::Token| &source[token.span.start..token.span.end];
    let mut replacements = Vec::new();
    for (index, token) in significant.iter().enumerate() {
        if text(token) != "&" {
            continue;
        }
        let mutable = significant
            .get(index + 1)
            .is_some_and(|token| text(token) == "mut");
        let binding_index = index + if mutable { 2 } else { 1 };
        let Some(binding) = significant.get(binding_index) else {
            continue;
        };
        if bindings.contains(text(binding)) {
            replacements.push((token.span.start, binding.span.end, text(binding).to_owned()));
        }
    }
    for (start, end, replacement) in replacements.into_iter().rev() {
        source.replace_range(start..end, &replacement);
    }
}

fn validate_rust_direct_place_uses(
    source: &str,
    classes: &HashMap<String, ClassId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (tokens, _) = crate::lexer::lex(source);
    let significant: Vec<_> = tokens
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let text = |token: &crate::lexer::Token| &source[token.span.start..token.span.end];
    let mut next_scope = 0usize;
    let mut scope = vec![next_scope];
    let mut token_scopes = Vec::with_capacity(significant.len());
    for token in &significant {
        token_scopes.push(scope.clone());
        match token.kind {
            crate::syntax::SyntaxKind::L_BRACE => {
                next_scope += 1;
                scope.push(next_scope);
            }
            crate::syntax::SyntaxKind::R_BRACE => {
                scope.pop();
            }
            _ => {}
        }
    }
    let declarations: Vec<_> = significant
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            (text(token) == "let").then(|| {
                let mut binding = index + 1;
                if significant
                    .get(binding)
                    .is_some_and(|token| text(token) == "mut")
                {
                    binding += 1;
                }
                significant.get(binding).map(|token| (text(token), binding))
            })?
        })
        .collect();
    let mut bindings = Vec::<(String, usize)>::new();
    for (index, token) in significant.iter().enumerate() {
        if text(token) != "let" {
            continue;
        }
        let mut cursor = index + 1;
        if significant
            .get(cursor)
            .is_some_and(|token| text(token) == "mut")
        {
            cursor += 1;
        }
        let Some(binding) = significant.get(cursor) else {
            continue;
        };
        let binding_index = cursor;
        cursor += 1;
        if significant
            .get(cursor)
            .is_some_and(|token| token.kind == crate::syntax::SyntaxKind::COLON)
        {
            cursor += 2;
        }
        if !significant
            .get(cursor)
            .is_some_and(|token| text(token) == "=")
            || !significant
                .get(cursor + 1)
                .is_some_and(|token| text(token) == "construct")
        {
            continue;
        }
        let Some(target) = significant.get(cursor + 2) else {
            continue;
        };
        if matches!(text(target), "Box" | "Rc" | "Arc") || !classes.contains_key(text(target)) {
            continue;
        }
        bindings.push((text(binding).to_owned(), binding_index));
    }

    for (binding, declaration_index) in bindings {
        for (index, token) in significant.iter().enumerate() {
            if index <= declaration_index || text(token) != binding {
                continue;
            }
            let shadowed = declarations.iter().any(|(name, shadow_index)| {
                *name == binding
                    && *shadow_index > declaration_index
                    && *shadow_index <= index
                    && token_scopes[*shadow_index]
                        .iter()
                        .zip(&token_scopes[index])
                        .all(|(left, right)| left == right)
                    && token_scopes[*shadow_index].len() <= token_scopes[index].len()
            });
            if shadowed {
                continue;
            }
            let previous = index.checked_sub(1).map(|index| text(significant[index]));
            let previous_previous = index.checked_sub(2).map(|index| text(significant[index]));
            let next = significant.get(index + 1).map(|token| text(token));
            let explicitly_borrowed = previous == Some("&")
                || (previous == Some("mut") && previous_previous == Some("&"));
            let view_operation = matches!(next, Some(".") | Some("is") | Some("as"));
            if explicitly_borrowed || view_operation {
                continue;
            }
            diagnostics.push(Diagnostic::error(
                format!(
                    "direct object place `{binding}` cannot be moved, assigned, passed, or returned by value"
                ),
                token.span,
            ));
            break;
        }
    }
}

fn rust_item_construction_bindings(
    source: &str,
    classes: &HashMap<String, ClassId>,
) -> HashMap<String, TypeKind> {
    let (tokens, _) = crate::lexer::lex(source);
    let significant: Vec<_> = tokens
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let text = |token: &crate::lexer::Token| &source[token.span.start..token.span.end];
    let mut environment = HashMap::new();
    for (index, token) in significant.iter().enumerate() {
        if text(token) != "let" {
            continue;
        }
        let mut cursor = index + 1;
        let mutable = significant
            .get(cursor)
            .is_some_and(|token| text(token) == "mut");
        if mutable {
            cursor += 1;
        }
        let Some(binding) = significant.get(cursor) else {
            continue;
        };
        cursor += 1;
        if significant
            .get(cursor)
            .is_some_and(|token| token.kind == crate::syntax::SyntaxKind::COLON)
        {
            cursor += 2;
        }
        if !significant
            .get(cursor)
            .is_some_and(|token| text(token) == "=")
            || !significant
                .get(cursor + 1)
                .is_some_and(|token| text(token) == "construct")
        {
            continue;
        }
        let first = significant.get(cursor + 2);
        let owner = first.and_then(|token| match text(token) {
            "Box" => Some(OwnerKind::Box),
            "Rc" => Some(OwnerKind::Rc),
            "Arc" => Some(OwnerKind::Arc),
            _ => None,
        });
        let target = if owner.is_some() {
            significant.get(cursor + 4)
        } else {
            first
        };
        let Some(class) = target.and_then(|token| classes.get(text(token))).copied() else {
            continue;
        };
        let kind = owner.map_or(TypeKind::ClassBorrow { mutable, class }, |owner| {
            TypeKind::ClassOwner { owner, class }
        });
        environment.insert(text(binding).to_owned(), kind);
    }
    environment
}

fn lower_capability_types(source: &mut String, classes: &HashMap<String, ClassId>) {
    let (tokens, _) = crate::lexer::lex(source);
    let significant: Vec<_> = tokens
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let text = |token: &crate::lexer::Token| &source[token.span.start..token.span.end];
    let mut replacements = Vec::new();
    let mut index = 0usize;
    while index < significant.len() {
        if index + 3 < significant.len()
            && matches!(text(significant[index]), "Box" | "Rc" | "Arc")
            && significant[index + 1].kind == crate::syntax::SyntaxKind::L_ANGLE
            && classes.contains_key(text(significant[index + 2]))
            && significant[index + 3].kind == crate::syntax::SyntaxKind::R_ANGLE
        {
            replacements.push((
                significant[index].span.start,
                significant[index + 3].span.end,
                format!(
                    "{}{}",
                    text(significant[index + 2]),
                    text(significant[index])
                ),
            ));
            index += 4;
            continue;
        }
        if text(significant[index]) == "&" {
            let lifetime = significant
                .get(index + 1)
                .filter(|token| text(token).starts_with('\''));
            let after_lifetime = index + 1 + usize::from(lifetime.is_some());
            let mutable = significant
                .get(after_lifetime)
                .is_some_and(|token| text(token) == "mut");
            let class_index = after_lifetime + usize::from(mutable);
            if let Some(class) = significant.get(class_index)
                && classes.contains_key(text(class))
            {
                replacements.push((
                    significant[index].span.start,
                    class.span.end,
                    format!(
                        "&{}{}{}dyn {}View",
                        lifetime.map_or("", |token| text(token)),
                        if lifetime.is_some() { " " } else { "" },
                        if mutable { "mut " } else { "" },
                        text(class)
                    ),
                ));
                index = class_index + 1;
                continue;
            }
        }
        index += 1;
    }
    for (start, end, replacement) in replacements.into_iter().rev() {
        source.replace_range(start..end, &replacement);
    }
}

fn validate_member_accesses(classes: &[Class], diagnostics: &mut Vec<Diagnostic>) {
    let by_id: HashMap<ClassId, &Class> = classes.iter().map(|class| (class.id, class)).collect();
    let adjacency: HashMap<ClassId, Vec<ClassId>> = classes
        .iter()
        .map(|class| {
            (
                class.id,
                class.bases.iter().map(|base| base.class).collect(),
            )
        })
        .collect();

    fn referenced_self_fields(body: &str) -> Vec<String> {
        let (tokens, _) = crate::lexer::lex(body);
        let significant: Vec<_> = tokens
            .iter()
            .filter(|token| !token.kind.is_trivia())
            .collect();
        let mut output = Vec::new();
        for window in significant.windows(4) {
            let text = |token: &crate::lexer::Token| &body[token.span.start..token.span.end];
            if text(window[0]) == "self"
                && text(window[1]) == "."
                && window[2].kind == crate::syntax::SyntaxKind::IDENT
                && text(window[3]) != "("
            {
                output.push(text(window[2]).to_owned());
            }
        }
        if significant.len() >= 3 {
            let tail = &significant[significant.len() - 3..];
            let text = |token: &crate::lexer::Token| &body[token.span.start..token.span.end];
            if text(tail[0]) == "self"
                && text(tail[1]) == "."
                && tail[2].kind == crate::syntax::SyntaxKind::IDENT
            {
                output.push(text(tail[2]).to_owned());
            }
        }
        output
    }

    fn referenced_self_methods(body: &str) -> Vec<String> {
        let (tokens, _) = crate::lexer::lex(body);
        let significant: Vec<_> = tokens
            .iter()
            .filter(|token| !token.kind.is_trivia())
            .collect();
        let text = |token: &crate::lexer::Token| &body[token.span.start..token.span.end];
        significant
            .windows(4)
            .filter(|window| {
                text(window[0]) == "self"
                    && text(window[1]) == "."
                    && window[2].kind == crate::syntax::SyntaxKind::IDENT
                    && text(window[3]) == "("
            })
            .map(|window| text(window[2]).to_owned())
            .collect()
    }

    for class in classes {
        let mut bodies = vec![
            (&class.init_body, Span::new(0, 0)),
            (&class.deinit_body, Span::new(0, 0)),
        ];
        bodies.extend(
            class
                .methods
                .iter()
                .filter_map(|method| method.body.as_ref().map(|body| (body, method.span))),
        );
        for (body, span) in bodies {
            for name in referenced_self_fields(body) {
                if class.fields.iter().any(|field| field.name == name) {
                    continue;
                }
                let candidates: Vec<_> = ancestor_ids(class.id, &adjacency)
                    .into_iter()
                    .filter_map(|id| {
                        by_id[&id]
                            .fields
                            .iter()
                            .find(|field| field.name == name)
                            .map(|field| (by_id[&id], field))
                    })
                    .collect();
                match candidates.as_slice() {
                    [] => {}
                    [(_, field)] if field.public => {}
                    [owner] => diagnostics.push(Diagnostic::error(
                        format!("field `{name}` is private to base class `{}`", owner.0.name),
                        span,
                    )),
                    _ => diagnostics.push(Diagnostic::error(
                        format!("inherited field `{name}` is ambiguous between base classes"),
                        span,
                    )),
                }
            }
            for name in referenced_self_methods(body) {
                if class.methods.iter().any(|method| method.name == name) {
                    continue;
                }
                let candidates: Vec<_> = ancestor_ids(class.id, &adjacency)
                    .into_iter()
                    .filter_map(|id| {
                        by_id[&id]
                            .methods
                            .iter()
                            .find(|method| method.name == name)
                            .map(|method| (by_id[&id], method))
                    })
                    .collect();
                match candidates.as_slice() {
                    [] => {}
                    [(_, method)] if method.public => {}
                    [owner] => diagnostics.push(Diagnostic::error(
                        format!(
                            "method `{name}` is private to base class `{}`",
                            owner.0.name
                        ),
                        span,
                    )),
                    _ => diagnostics.push(Diagnostic::error(
                        format!("inherited method `{name}` is ambiguous between base classes"),
                        span,
                    )),
                }
            }
        }
    }
}

fn validate_typed_receiver_method_accesses(
    classes: &[Class],
    rust_items: &[RustItem],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let class_ids: HashMap<_, _> = classes
        .iter()
        .map(|class| (class.name.clone(), class.id))
        .collect();
    let by_id: HashMap<_, _> = classes.iter().map(|class| (class.id, class)).collect();
    let adjacency: HashMap<ClassId, Vec<ClassId>> = classes
        .iter()
        .map(|class| {
            (
                class.id,
                class.bases.iter().map(|base| base.class).collect(),
            )
        })
        .collect();

    fn calls(body: &str) -> Vec<(String, String)> {
        let (tokens, _) = crate::lexer::lex(body);
        let significant: Vec<_> = tokens
            .iter()
            .filter(|token| !token.kind.is_trivia())
            .collect();
        let text = |token: &crate::lexer::Token| &body[token.span.start..token.span.end];
        significant
            .windows(4)
            .filter(|window| {
                matches!(
                    window[0].kind,
                    crate::syntax::SyntaxKind::IDENT | crate::syntax::SyntaxKind::VALUE_KW
                ) && text(window[0]) != "self"
                    && text(window[1]) == "."
                    && window[2].kind == crate::syntax::SyntaxKind::IDENT
                    && text(window[3]) == "("
            })
            .map(|window| (text(window[0]).to_owned(), text(window[2]).to_owned()))
            .collect()
    }

    let validate_body = |body: &str,
                         environment: &HashMap<String, TypeKind>,
                         context: Option<ClassId>,
                         span: Span,
                         diagnostics: &mut Vec<Diagnostic>| {
        for (receiver, name) in calls(body) {
            let source = match environment.get(&receiver) {
                Some(TypeKind::ClassBorrow { class, .. } | TypeKind::ClassOwner { class, .. }) => {
                    *class
                }
                _ => continue,
            };
            let source_class = by_id[&source];
            let candidates: Vec<_> = if let Some(method) = source_class
                .methods
                .iter()
                .find(|method| method.name == name)
            {
                vec![(source_class, method)]
            } else {
                ancestor_ids(source, &adjacency)
                    .into_iter()
                    .filter_map(|id| {
                        by_id[&id]
                            .methods
                            .iter()
                            .find(|method| method.name == name)
                            .map(|method| (by_id[&id], method))
                    })
                    .collect()
            };
            match candidates.as_slice() {
                [] => {}
                [(_, method)] if method.public => {}
                [(owner, _)] if context == Some(owner.id) => {}
                [(owner, _)] => diagnostics.push(Diagnostic::error(
                    format!("method `{name}` is private to class `{}`", owner.name),
                    span,
                )),
                _ => diagnostics.push(Diagnostic::error(
                    format!("method `{name}` is ambiguous for receiver `{receiver}`"),
                    span,
                )),
            }
        }
    };

    for class in classes {
        for method in &class.methods {
            if let Some(body) = &method.body {
                let environment = method_parameter_kinds(method, &class_ids);
                validate_body(body, &environment, Some(class.id), method.span, diagnostics);
            }
        }
    }
    for item in rust_items {
        let Ok(function) = syn::parse_str::<syn::ItemFn>(&item.source) else {
            continue;
        };
        use quote::ToTokens;
        let environment: HashMap<_, _> = function
            .sig
            .inputs
            .iter()
            .filter_map(|input| {
                let syn::FnArg::Typed(input) = input else {
                    return None;
                };
                let syn::Pat::Ident(pattern) = input.pat.as_ref() else {
                    return None;
                };
                Some((
                    pattern.ident.to_string(),
                    classify_type(&input.ty.to_token_stream().to_string(), &class_ids),
                ))
            })
            .collect();
        validate_body(
            &item.source,
            &environment,
            None,
            Span::new(0, item.source.len()),
            diagnostics,
        );
    }
}

fn lower_class_view_operators(
    value_classes: &mut [ValueClass],
    classes: &mut [Class],
    rust_items: &[RustItem],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let class_ids: HashMap<String, ClassId> = classes
        .iter()
        .map(|class| (class.name.clone(), class.id))
        .collect();
    let class_names: HashMap<ClassId, String> = classes
        .iter()
        .map(|class| (class.id, class.name.clone()))
        .collect();
    let base_edges: HashMap<ClassId, Vec<Base>> = classes
        .iter()
        .map(|class| (class.id, class.bases.clone()))
        .collect();
    let free_callable_returns: HashMap<_, _> = rust_items
        .iter()
        .filter_map(|item| rust_item_function_return_kind(&item.source, &class_ids))
        .collect();
    let field_environments: HashMap<ClassId, HashMap<String, TypeKind>> = {
        let by_id: HashMap<_, _> = classes.iter().map(|class| (class.id, class)).collect();
        fn visible_fields(
            class: ClassId,
            by_id: &HashMap<ClassId, &Class>,
            visiting: &mut HashSet<ClassId>,
        ) -> HashMap<String, Vec<TypeKind>> {
            if !visiting.insert(class) {
                return HashMap::new();
            }
            let current = by_id[&class];
            let mut fields: HashMap<String, Vec<TypeKind>> = current
                .fields
                .iter()
                .map(|field| (field.name.clone(), vec![field.ty.kind.clone()]))
                .collect();
            for base in &current.bases {
                for (name, kinds) in visible_fields(base.class, by_id, visiting) {
                    if !current.fields.iter().any(|field| field.name == name) {
                        fields.entry(name).or_default().extend(kinds);
                    }
                }
            }
            visiting.remove(&class);
            fields
        }
        classes
            .iter()
            .map(|class| {
                let fields = visible_fields(class.id, &by_id, &mut HashSet::new())
                    .into_iter()
                    .filter_map(|(name, kinds)| {
                        (kinds.len() == 1)
                            .then(|| (format!("self.{name}"), kinds.into_iter().next().unwrap()))
                    })
                    .collect();
                (class.id, fields)
            })
            .collect()
    };
    let method_environments: HashMap<ClassId, HashMap<String, TypeKind>> = {
        let by_id: HashMap<_, _> = classes.iter().map(|class| (class.id, class)).collect();
        fn visible_methods(
            class: ClassId,
            by_id: &HashMap<ClassId, &Class>,
            classes: &HashMap<String, ClassId>,
            visiting: &mut HashSet<ClassId>,
        ) -> HashMap<String, Vec<TypeKind>> {
            if !visiting.insert(class) {
                return HashMap::new();
            }
            let current = by_id[&class];
            let own = method_return_kinds(&current.methods, classes);
            let mut methods: HashMap<String, Vec<TypeKind>> = own
                .iter()
                .map(|(name, kind)| (name.clone(), vec![kind.clone()]))
                .collect();
            for base in &current.bases {
                for (name, kinds) in visible_methods(base.class, by_id, classes, visiting) {
                    if !own.contains_key(&name) {
                        methods.entry(name).or_default().extend(kinds);
                    }
                }
            }
            visiting.remove(&class);
            methods
        }
        classes
            .iter()
            .map(|class| {
                let methods = visible_methods(class.id, &by_id, &class_ids, &mut HashSet::new())
                    .into_iter()
                    .filter_map(|(name, kinds)| {
                        (kinds.len() == 1).then(|| (name, kinds.into_iter().next().unwrap()))
                    })
                    .collect();
                (class.id, methods)
            })
            .collect()
    };

    for value_class in value_classes {
        let callable_returns = method_return_kinds(&value_class.methods, &class_ids);
        for method in &mut value_class.methods {
            let mut environment = free_callable_returns.clone();
            environment.extend(method_parameter_kinds(method, &class_ids));
            environment.extend(callable_returns.clone());
            if let Some(body) = &mut method.body {
                *body = lower_view_operators_in_body(
                    body,
                    &environment,
                    ViewOperatorContext {
                        self_kind: None,
                        class_ids: &class_ids,
                        class_names: &class_names,
                        base_edges: &base_edges,
                        method_returns: &method_environments,
                        access_context: None,
                        diagnostic_span: method.span,
                    },
                    diagnostics,
                );
                lower_capability_types(body, &class_ids);
            }
        }
    }
    for class in classes {
        let callable_returns = method_environments[&class.id].clone();
        let mut constructor_environment = free_callable_returns.clone();
        constructor_environment.extend(
            class
                .constructor
                .params
                .iter()
                .map(|param| (param.name.clone(), param.ty.kind.clone())),
        );
        constructor_environment.extend(field_environments[&class.id].clone());
        constructor_environment.extend(callable_returns.clone());
        class.init_body = lower_view_operators_in_body(
            &class.init_body,
            &constructor_environment,
            ViewOperatorContext {
                self_kind: Some((class.id, true)),
                class_ids: &class_ids,
                class_names: &class_names,
                base_edges: &base_edges,
                method_returns: &method_environments,
                access_context: Some(class.id),
                diagnostic_span: class
                    .methods
                    .first()
                    .map_or(Span::new(0, 0), |method| method.span),
            },
            diagnostics,
        );
        lower_capability_types(&mut class.init_body, &class_ids);
        let mut deinit_environment = free_callable_returns.clone();
        deinit_environment.extend(field_environments[&class.id].clone());
        deinit_environment.extend(callable_returns.clone());
        class.deinit_body = lower_view_operators_in_body(
            &class.deinit_body,
            &deinit_environment,
            ViewOperatorContext {
                self_kind: Some((class.id, true)),
                class_ids: &class_ids,
                class_names: &class_names,
                base_edges: &base_edges,
                method_returns: &method_environments,
                access_context: Some(class.id),
                diagnostic_span: class
                    .methods
                    .first()
                    .map_or(Span::new(0, 0), |method| method.span),
            },
            diagnostics,
        );
        lower_capability_types(&mut class.deinit_body, &class_ids);
        for method in &mut class.methods {
            let mut environment = free_callable_returns.clone();
            environment.extend(method_parameter_kinds(method, &class_ids));
            environment.extend(callable_returns.clone());
            let signature = syn::parse_str::<syn::Signature>(&method.signature)
                .expect("method signatures were validated during lowering");
            let self_mutable = signature
                .inputs
                .first()
                .and_then(|input| match input {
                    syn::FnArg::Receiver(receiver) => Some(receiver.mutability.is_some()),
                    syn::FnArg::Typed(_) => None,
                })
                .unwrap_or(false);
            environment.insert(
                "self".to_owned(),
                TypeKind::ClassBorrow {
                    mutable: self_mutable,
                    class: class.id,
                },
            );
            environment.extend(field_environments[&class.id].clone());
            if let Some(body) = &mut method.body {
                *body = lower_view_operators_in_body(
                    body,
                    &environment,
                    ViewOperatorContext {
                        self_kind: Some((class.id, self_mutable)),
                        class_ids: &class_ids,
                        class_names: &class_names,
                        base_edges: &base_edges,
                        method_returns: &method_environments,
                        access_context: Some(class.id),
                        diagnostic_span: method.span,
                    },
                    diagnostics,
                );
                lower_capability_types(body, &class_ids);
            }
        }
    }
}

fn method_return_kinds(
    methods: &[Method],
    classes: &HashMap<String, ClassId>,
) -> HashMap<String, TypeKind> {
    use quote::ToTokens;
    methods
        .iter()
        .filter_map(|method| {
            let signature = syn::parse_str::<syn::Signature>(&method.signature).ok()?;
            let syn::ReturnType::Type(_, ty) = signature.output else {
                return None;
            };
            let kind = classify_type(&ty.to_token_stream().to_string(), classes);
            matches!(
                kind,
                TypeKind::ClassBorrow { .. } | TypeKind::ClassOwner { .. }
            )
            .then(|| (format!("self.{}", method.name), kind))
        })
        .collect()
}

fn method_parameter_kinds(
    method: &Method,
    classes: &HashMap<String, ClassId>,
) -> HashMap<String, TypeKind> {
    use quote::ToTokens;
    let signature = syn::parse_str::<syn::Signature>(&method.signature)
        .expect("method signatures were validated during lowering");
    signature
        .inputs
        .iter()
        .filter_map(|input| {
            let syn::FnArg::Typed(input) = input else {
                return None;
            };
            let syn::Pat::Ident(pattern) = input.pat.as_ref() else {
                return None;
            };
            Some((
                pattern.ident.to_string(),
                classify_type(&input.ty.to_token_stream().to_string(), classes),
            ))
        })
        .collect()
}

#[derive(Clone, Copy)]
struct ViewOperatorContext<'a> {
    self_kind: Option<(ClassId, bool)>,
    class_ids: &'a HashMap<String, ClassId>,
    class_names: &'a HashMap<ClassId, String>,
    base_edges: &'a HashMap<ClassId, Vec<Base>>,
    method_returns: &'a HashMap<ClassId, HashMap<String, TypeKind>>,
    access_context: Option<ClassId>,
    diagnostic_span: Span,
}

fn lower_view_operators_in_body(
    body: &str,
    environment: &HashMap<String, TypeKind>,
    context: ViewOperatorContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let (tokens, lexical_diagnostics) = crate::lexer::lex(body);
    diagnostics.extend(lexical_diagnostics);
    let significant: Vec<_> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| !token.kind.is_trivia())
        .collect();
    let text = |token: &crate::lexer::Token| &body[token.span.start..token.span.end];
    let mut known_types = environment.clone();
    for (index, (_, token)) in significant.iter().enumerate() {
        if text(token) != "let" {
            continue;
        }
        let mut binding_index = index + 1;
        if significant
            .get(binding_index)
            .is_some_and(|(_, token)| text(token) == "mut")
        {
            binding_index += 1;
        }
        let Some((_, binding)) = significant.get(binding_index) else {
            continue;
        };
        let Some((_, colon)) = significant.get(binding_index + 1) else {
            continue;
        };
        if colon.kind != crate::syntax::SyntaxKind::COLON {
            continue;
        }
        let Some(equal_index) = significant
            .iter()
            .enumerate()
            .skip(binding_index + 2)
            .find_map(|(index, (_, token))| (text(token) == "=").then_some(index))
        else {
            continue;
        };
        if equal_index == binding_index + 2 {
            continue;
        }
        let type_start = significant[binding_index + 2].1.span.start;
        let type_end = significant[equal_index - 1].1.span.end;
        let kind = classify_type(&body[type_start..type_end], context.class_ids);
        if matches!(
            kind,
            TypeKind::ClassBorrow { .. } | TypeKind::ClassOwner { .. }
        ) {
            known_types.insert(text(binding).to_owned(), kind);
        }
    }
    let mut replacements = Vec::<(usize, usize, String)>::new();
    let mut cursor = 0usize;
    while cursor < significant.len() {
        let (_, operand_token) = significant[cursor];
        if !matches!(
            operand_token.kind,
            crate::syntax::SyntaxKind::IDENT | crate::syntax::SyntaxKind::VALUE_KW
        ) {
            cursor += 1;
            continue;
        }
        let simple_operand_name = text(operand_token);
        let member_operand = significant
            .get(cursor + 1)
            .is_some_and(|(_, token)| text(token) == ".")
            && significant.get(cursor + 2).is_some_and(|(_, token)| {
                matches!(
                    token.kind,
                    crate::syntax::SyntaxKind::IDENT | crate::syntax::SyntaxKind::VALUE_KW
                ) && text(token) != "clone"
            });
        let operand_name = if member_operand {
            format!(
                "{}.{}",
                simple_operand_name,
                text(significant[cursor + 2].1)
            )
        } else {
            simple_operand_name.to_owned()
        };
        let method_operand_kind = if member_operand
            && significant
                .get(cursor + 3)
                .is_some_and(|(_, token)| text(token) == "(")
        {
            let receiver_kind = if simple_operand_name == "self" {
                context
                    .self_kind
                    .map(|(class, mutable)| TypeKind::ClassBorrow { mutable, class })
            } else {
                known_types.get(simple_operand_name).cloned()
            };
            let receiver_class = match receiver_kind {
                Some(TypeKind::ClassBorrow { class, .. } | TypeKind::ClassOwner { class, .. }) => {
                    Some(class)
                }
                _ => None,
            };
            receiver_class.and_then(|class| {
                context
                    .method_returns
                    .get(&class)
                    .and_then(|methods| {
                        methods.get(&format!("self.{}", text(significant[cursor + 2].1)))
                    })
                    .cloned()
            })
        } else {
            None
        };
        let call_open = if member_operand
            && (known_types.get(&operand_name).is_some_and(|kind| {
                matches!(
                    kind,
                    TypeKind::ClassBorrow { .. } | TypeKind::ClassOwner { .. }
                )
            }) || method_operand_kind.is_some())
            && significant
                .get(cursor + 3)
                .is_some_and(|(_, token)| text(token) == "(")
        {
            Some(cursor + 3)
        } else if !member_operand
            && known_types.get(&operand_name).is_some_and(|kind| {
                matches!(
                    kind,
                    TypeKind::ClassBorrow { .. } | TypeKind::ClassOwner { .. }
                )
            })
            && significant
                .get(cursor + 1)
                .is_some_and(|(_, token)| text(token) == "(")
        {
            Some(cursor + 1)
        } else {
            None
        };
        let call_close = call_open.and_then(|open| {
            let mut depth = 0usize;
            for (index, (_, token)) in significant.iter().enumerate().skip(open) {
                match token.kind {
                    crate::syntax::SyntaxKind::L_PAREN => depth += 1,
                    crate::syntax::SyntaxKind::R_PAREN => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(index);
                        }
                    }
                    _ => {}
                }
            }
            None
        });
        let core_operand_len = call_close.map_or(if member_operand { 3 } else { 1 }, |close| {
            close - cursor + 1
        });
        let clone_operand = significant
            .get(cursor + core_operand_len)
            .is_some_and(|(_, token)| text(token) == ".")
            && significant
                .get(cursor + core_operand_len + 1)
                .is_some_and(|(_, token)| text(token) == "clone")
            && significant
                .get(cursor + core_operand_len + 2)
                .is_some_and(|(_, token)| text(token) == "(")
            && significant
                .get(cursor + core_operand_len + 3)
                .is_some_and(|(_, token)| text(token) == ")");
        let unwrapped_len = core_operand_len + if clone_operand { 4 } else { 0 };
        let (borrow_prefix, borrow_mutable) = if cursor >= 3
            && text(significant[cursor - 3].1) == "&"
            && text(significant[cursor - 2].1) == "mut"
            && text(significant[cursor - 1].1) == "*"
        {
            (3usize, Some(true))
        } else if cursor >= 2
            && text(significant[cursor - 2].1) == "&"
            && text(significant[cursor - 1].1) == "*"
        {
            (2usize, Some(false))
        } else {
            (0usize, None)
        };
        let syntactic_start = cursor - borrow_prefix;
        let mut wrappers = 0usize;
        while syntactic_start > wrappers
            && significant
                .get(syntactic_start - wrappers - 1)
                .is_some_and(|(_, token)| text(token) == "(")
            && significant
                .get(cursor + unwrapped_len + wrappers)
                .is_some_and(|(_, token)| text(token) == ")")
        {
            wrappers += 1;
        }
        let operator_offset = unwrapped_len + wrappers;
        let is_form = significant
            .get(cursor + operator_offset)
            .is_some_and(|(_, token)| text(token) == "is");
        let as_form = significant
            .get(cursor + operator_offset)
            .is_some_and(|(_, token)| text(token) == "as")
            && significant
                .get(cursor + operator_offset + 1)
                .is_some_and(|(_, token)| text(token) == "?");
        let target_offset = if is_form {
            operator_offset + 1
        } else if as_form {
            operator_offset + 2
        } else {
            cursor += 1;
            continue;
        };
        let Some((_, target_token)) = significant.get(cursor + target_offset) else {
            diagnostics.push(Diagnostic::error(
                "class-view operator requires a target class",
                context.diagnostic_span,
            ));
            break;
        };
        let target = text(target_token);
        let Some(target_id) = context.class_ids.get(target).copied() else {
            diagnostics.push(Diagnostic::error(
                format!("unknown class-view cast target `{target}`"),
                context.diagnostic_span,
            ));
            cursor += target_offset + 1;
            continue;
        };
        let operand_kind = if let Some(kind) = method_operand_kind {
            Some(kind)
        } else if operand_name == "self" {
            context
                .self_kind
                .map(|(class, mutable)| TypeKind::ClassBorrow { mutable, class })
        } else {
            known_types.get(&operand_name).cloned()
        };
        let operand_kind = match (borrow_mutable, operand_kind) {
            (Some(mutable), Some(TypeKind::ClassOwner { class, .. })) => {
                Some(TypeKind::ClassBorrow { mutable, class })
            }
            (Some(_), _) => None,
            (None, kind) => kind,
        };
        let Some(operand_kind) = operand_kind else {
            diagnostics.push(Diagnostic::error(
                format!("cannot determine class-view capability of `{operand_name}`"),
                context.diagnostic_span,
            ));
            cursor += target_offset + 1;
            continue;
        };
        let result_kind = match &operand_kind {
            TypeKind::ClassBorrow { mutable, .. } => Some(TypeKind::ClassBorrow {
                mutable: *mutable,
                class: target_id,
            }),
            TypeKind::ClassOwner { owner, .. } => Some(TypeKind::ClassOwner {
                owner: *owner,
                class: target_id,
            }),
            _ => None,
        };
        let (source_id, helper) = match operand_kind {
            TypeKind::ClassBorrow { mutable, class } if as_form => {
                (class, if mutable { "cast_mut" } else { "cast_ref" })
            }
            TypeKind::ClassOwner { owner, class } if as_form => (
                class,
                match owner {
                    OwnerKind::Box => "cast_box",
                    OwnerKind::Rc => "cast_rc",
                    OwnerKind::Arc => "cast_arc",
                },
            ),
            TypeKind::ClassBorrow { class, .. } | TypeKind::ClassOwner { class, .. } if is_form => {
                (class, "is")
            }
            _ => {
                diagnostics.push(Diagnostic::error(
                    format!("`{operand_name}` is not a class view or stable class owner"),
                    context.diagnostic_span,
                ));
                cursor += target_offset + 1;
                continue;
            }
        };
        if !class_view_conversion_is_accessible(
            source_id,
            target_id,
            context.access_context,
            context.base_edges,
        ) {
            diagnostics.push(Diagnostic::error(
                format!(
                    "class view `{target}` is not accessible from `{}` in this context",
                    context.class_names.get(&source_id).unwrap()
                ),
                context.diagnostic_span,
            ));
            cursor += target_offset + 1;
            continue;
        }
        let source = context.class_names.get(&source_id).unwrap().to_lowercase();
        let target_lower = context.class_names.get(&target_id).unwrap().to_lowercase();
        let expression_start = significant[syntactic_start - wrappers].1.span.start;
        let operand_end = significant[cursor + unwrapped_len - 1].1.span.end;
        let operand_expression = &body[significant[syntactic_start].1.span.start..operand_end];
        let replacement = if is_form {
            format!("__rpp_is_{target_lower}(&*{operand_expression})")
        } else {
            format!("__rpp_{helper}_{source}_to_{target_lower}({operand_expression})")
        };
        replacements.push((expression_start, target_token.span.end, replacement));
        // A successful cast immediately unwrapped into a simple local retains
        // its capability kind. This makes subsequent view operations on that
        // local type-directed without attempting to type-check arbitrary Rust
        // expressions in the bootstrap frontend.
        let direct_unwrap = significant
            .get(cursor + target_offset + 2)
            .is_some_and(|(_, token)| text(token) == ".")
            && significant
                .get(cursor + target_offset + 3)
                .is_some_and(|(_, token)| text(token) == "unwrap");
        let result_ok_unwrap = significant
            .get(cursor + target_offset + 3)
            .is_some_and(|(_, token)| text(token) == "ok")
            && significant
                .get(cursor + target_offset + 6)
                .is_some_and(|(_, token)| text(token) == ".")
            && significant
                .get(cursor + target_offset + 7)
                .is_some_and(|(_, token)| text(token) == "unwrap");
        if as_form && (direct_unwrap || result_ok_unwrap) {
            let equals = (0..cursor)
                .rev()
                .find(|index| cursor - *index <= 4 && text(significant[*index].1) == "=");
            if let Some(equals) = equals
                && equals >= 2
                && text(significant[equals - 2].1) == "let"
                && matches!(
                    significant[equals - 1].1.kind,
                    crate::syntax::SyntaxKind::IDENT | crate::syntax::SyntaxKind::VALUE_KW
                )
                && let Some(result_kind) = result_kind
            {
                known_types.insert(text(significant[equals - 1].1).to_owned(), result_kind);
            }
        }
        cursor += target_offset + 1;
    }
    let mut output = body.to_owned();
    for (start, end, replacement) in replacements.into_iter().rev() {
        output.replace_range(start..end, &replacement);
    }
    lower_parenthesized_dynamic_operators(&output, &known_types, context, diagnostics)
}

fn lower_parenthesized_dynamic_operators(
    body: &str,
    environment: &HashMap<String, TypeKind>,
    context: ViewOperatorContext<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    let (tokens, lexical_diagnostics) = crate::lexer::lex(body);
    diagnostics.extend(lexical_diagnostics);
    let significant: Vec<_> = tokens
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    let text = |token: &crate::lexer::Token| &body[token.span.start..token.span.end];
    let mut replacements = Vec::new();
    let mut index = 0usize;
    while index < significant.len() {
        let as_form = text(significant[index]) == "as"
            && significant
                .get(index + 1)
                .is_some_and(|token| text(token) == "?");
        let is_form = text(significant[index]) == "is";
        if !as_form && !is_form {
            index += 1;
            continue;
        }
        let target_index = index + if as_form { 2 } else { 1 };
        let Some(target_token) = significant.get(target_index) else {
            index += 1;
            continue;
        };
        let target = text(target_token);
        let Some(target_id) = context.class_ids.get(target).copied() else {
            index += 1;
            continue;
        };
        let Some(close_index) = index
            .checked_sub(1)
            .filter(|close| significant[*close].kind == crate::syntax::SyntaxKind::R_PAREN)
        else {
            index += 1;
            continue;
        };
        let mut depth = 0usize;
        let mut open_index = None;
        for candidate in (0..=close_index).rev() {
            match significant[candidate].kind {
                crate::syntax::SyntaxKind::R_PAREN => depth += 1,
                crate::syntax::SyntaxKind::L_PAREN => {
                    depth -= 1;
                    if depth == 0 {
                        open_index = Some(candidate);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(open_index) = open_index else {
            index += 1;
            continue;
        };
        let expression =
            &body[significant[open_index].span.end..significant[close_index].span.start];
        let target_lower = context.class_names[&target_id].to_lowercase();
        let expression_kinds: Vec<_> = significant[open_index + 1..close_index]
            .iter()
            .filter_map(|token| environment.get(text(token)).cloned())
            .filter(|kind| {
                matches!(
                    kind,
                    TypeKind::ClassBorrow { .. } | TypeKind::ClassOwner { .. }
                )
            })
            .collect();
        let inferred_kind = expression_kinds.first().filter(|first| {
            expression_kinds
                .iter()
                .all(|kind| same_class_capability_kind(first, kind))
        });
        let inferred_source = inferred_kind.map(|kind| match kind {
            TypeKind::ClassBorrow { class, .. } | TypeKind::ClassOwner { class, .. } => *class,
            _ => unreachable!(),
        });
        if let Some(source) = inferred_source
            && !class_view_conversion_is_accessible(
                source,
                target_id,
                context.access_context,
                context.base_edges,
            )
        {
            diagnostics.push(Diagnostic::error(
                format!(
                    "class view `{target}` is not accessible from `{}` in this context",
                    context.class_names[&source]
                ),
                context.diagnostic_span,
            ));
            index = target_index + 1;
            continue;
        }
        let replacement = if as_form {
            if let Some(kind) = inferred_kind {
                let source = match kind {
                    TypeKind::ClassBorrow { class, .. } | TypeKind::ClassOwner { class, .. } => {
                        context.class_names[class].to_lowercase()
                    }
                    _ => unreachable!(),
                };
                let helper = match kind {
                    TypeKind::ClassBorrow { mutable: false, .. } => "cast_ref",
                    TypeKind::ClassBorrow { mutable: true, .. } => "cast_mut",
                    TypeKind::ClassOwner {
                        owner: OwnerKind::Box,
                        ..
                    } => "cast_box",
                    TypeKind::ClassOwner {
                        owner: OwnerKind::Rc,
                        ..
                    } => "cast_rc",
                    TypeKind::ClassOwner {
                        owner: OwnerKind::Arc,
                        ..
                    } => "cast_arc",
                    _ => unreachable!(),
                };
                format!("__rpp_{helper}_{source}_to_{target_lower}({expression})")
            } else {
                format!("__rpp_dynamic_as_{target_lower}({expression})")
            }
        } else {
            format!("__rpp_is_{target_lower}(&*({expression}))")
        };
        replacements.push((
            significant[open_index].span.start,
            target_token.span.end,
            replacement,
        ));
        index = target_index + 1;
    }
    let mut output = body.to_owned();
    for (start, end, replacement) in replacements.into_iter().rev() {
        output.replace_range(start..end, &replacement);
    }
    let (remaining, _) = crate::lexer::lex(&output);
    let remaining: Vec<_> = remaining
        .iter()
        .filter(|token| !token.kind.is_trivia())
        .collect();
    if remaining
        .windows(2)
        .any(|window| text_in(&output, window[0]) == "as" && text_in(&output, window[1]) == "?")
    {
        diagnostics.push(Diagnostic::error(
            "cannot determine the class-view capability of this compound cast expression; parenthesize the complete expression or add a typed local binding",
            context.diagnostic_span,
        ));
    }
    output
}

fn same_class_capability_kind(left: &TypeKind, right: &TypeKind) -> bool {
    match (left, right) {
        (
            TypeKind::ClassBorrow {
                mutable: left_mutable,
                class: left_class,
            },
            TypeKind::ClassBorrow {
                mutable: right_mutable,
                class: right_class,
            },
        ) => left_mutable == right_mutable && left_class == right_class,
        (
            TypeKind::ClassOwner {
                owner: left_owner,
                class: left_class,
            },
            TypeKind::ClassOwner {
                owner: right_owner,
                class: right_class,
            },
        ) => left_owner == right_owner && left_class == right_class,
        _ => false,
    }
}

fn text_in<'a>(source: &'a str, token: &crate::lexer::Token) -> &'a str {
    &source[token.span.start..token.span.end]
}

fn class_view_conversion_is_accessible(
    source: ClassId,
    target: ClassId,
    context: Option<ClassId>,
    edges: &HashMap<ClassId, Vec<Base>>,
) -> bool {
    fn find_path(
        current: ClassId,
        target: ClassId,
        edges: &HashMap<ClassId, Vec<Base>>,
        path: &mut Vec<(ClassId, BaseVisibility)>,
    ) -> bool {
        if current == target {
            return true;
        }
        for edge in edges.get(&current).into_iter().flatten() {
            path.push((current, edge.visibility));
            if find_path(edge.class, target, edges, path) {
                return true;
            }
            path.pop();
        }
        false
    }

    fn derives_from(
        candidate: ClassId,
        base: ClassId,
        edges: &HashMap<ClassId, Vec<Base>>,
    ) -> bool {
        let mut path = Vec::new();
        find_path(candidate, base, edges, &mut path)
    }

    let mut path = Vec::new();
    if !find_path(source, target, edges, &mut path) {
        path.clear();
        if !find_path(target, source, edges, &mut path) {
            // Unrelated static views may be siblings in an unknown complete
            // dynamic class. Its descriptor determines whether both exist.
            return true;
        }
    }
    path.into_iter()
        .all(|(owner, visibility)| match visibility {
            BaseVisibility::Public => true,
            BaseVisibility::Private => context == Some(owner),
            BaseVisibility::Protected => context
                .is_some_and(|context| context == owner || derives_from(context, owner, edges)),
        })
}

fn resolve_inline_constructions(classes: &mut [Class], diagnostics: &mut Vec<Diagnostic>) {
    let names: HashMap<ClassId, String> = classes
        .iter()
        .map(|class| (class.id, class.name.clone()))
        .collect();
    for class in classes {
        for initializer in &mut class.constructor.initializers {
            let Some(field) = class
                .fields
                .iter()
                .find(|field| field.name == initializer.field)
            else {
                continue;
            };
            match field.ty.kind {
                TypeKind::ExactClass(target) => {
                    let target_name = names.get(&target).expect("resolved class ID must exist");
                    match initializer
                        .expression
                        .as_deref()
                        .and_then(parse_construct_expression)
                    {
                        Some((written_target, arguments)) if written_target == target_name => {
                            initializer.inline = Some(InlineConstruction {
                                class: target,
                                arguments: arguments.to_owned(),
                            });
                        }
                        Some((written_target, _)) => diagnostics.push(Diagnostic::error(
                            format!(
                                "inline field `{}` has type `{target_name}` but constructs `{written_target}`",
                                field.name
                            ),
                            field.ty.span,
                        )),
                        None => diagnostics.push(Diagnostic::error(
                            format!(
                                "ordinary class field `{}` must be initialized with `construct {target_name}(...)`",
                                field.name
                            ),
                            field.ty.span,
                        )),
                    }
                }
                _ if initializer
                    .expression
                    .as_deref()
                    .is_some_and(|expression| parse_construct_expression(expression).is_some()) =>
                {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "`construct` initializer is only valid for an ordinary class field, not `{}`",
                            field.name
                        ),
                        field.ty.span,
                    ));
                }
                _ => {}
            }
        }
    }
}

fn resolve_base_graph(
    classes: &mut [Class],
    unresolved_edges: &HashMap<ClassId, Vec<(String, BaseVisibility, Span)>>,
    unresolved_initializers: &HashMap<ClassId, Vec<(String, String, Span)>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ids: HashMap<String, ClassId> = classes
        .iter()
        .map(|class| (class.name.clone(), class.id))
        .collect();
    let final_classes: HashSet<ClassId> = classes
        .iter()
        .filter(|class| class.final_)
        .map(|class| class.id)
        .collect();
    for class in classes.iter_mut() {
        let mut direct = HashSet::new();
        for (name, visibility, span) in unresolved_edges.get(&class.id).into_iter().flatten() {
            let Some(base_id) = ids.get(name).copied() else {
                diagnostics.push(Diagnostic::error(
                    format!("unknown ordinary base class `{name}`"),
                    *span,
                ));
                continue;
            };
            if base_id == class.id {
                diagnostics.push(Diagnostic::error(
                    format!("class `{}` cannot directly inherit from itself", class.name),
                    *span,
                ));
                continue;
            }
            if final_classes.contains(&base_id) {
                diagnostics.push(Diagnostic::error(
                    format!("cannot derive from final class `{name}`"),
                    *span,
                ));
                continue;
            }
            if !direct.insert(base_id) {
                diagnostics.push(Diagnostic::error(
                    format!("duplicate direct base `{name}` in class `{}`", class.name),
                    *span,
                ));
                continue;
            }
            class.bases.push(Base {
                class: base_id,
                visibility: *visibility,
                span: *span,
            });
        }

        let mut initialized = HashSet::new();
        for (name, arguments, span) in unresolved_initializers.get(&class.id).into_iter().flatten()
        {
            let Some(base_id) = ids.get(name).copied() else {
                diagnostics.push(Diagnostic::error(
                    format!("unknown base initializer `{name}`"),
                    *span,
                ));
                continue;
            };
            if !class.bases.iter().any(|base| base.class == base_id) {
                diagnostics.push(Diagnostic::error(
                    format!("`{name}` is not a direct base of `{}`", class.name),
                    *span,
                ));
                continue;
            }
            if !initialized.insert(base_id) {
                diagnostics.push(Diagnostic::error(
                    format!("base `{name}` is initialized more than once"),
                    *span,
                ));
                continue;
            }
            class.constructor.base_initializers.push(BaseInitializer {
                class: base_id,
                arguments: arguments.clone(),
            });
        }
        for base in &class.bases {
            if !initialized.contains(&base.class) {
                let name = ids
                    .iter()
                    .find_map(|(name, id)| (*id == base.class).then_some(name.as_str()))
                    .unwrap();
                diagnostics.push(Diagnostic::error(
                    format!("direct base `{name}` is missing `base {name}(...)` initialization"),
                    base.span,
                ));
            }
        }
    }

    let adjacency: HashMap<ClassId, Vec<ClassId>> = classes
        .iter()
        .map(|class| {
            (
                class.id,
                class.bases.iter().map(|base| base.class).collect(),
            )
        })
        .collect();
    let names: HashMap<ClassId, String> = classes
        .iter()
        .map(|class| (class.id, class.name.clone()))
        .collect();
    for class in classes.iter() {
        let mut seen = HashSet::new();
        let mut path = vec![class.id];
        if let Err(message) =
            validate_base_closure(class.id, &adjacency, &names, &mut seen, &mut path)
        {
            diagnostics.push(Diagnostic::error(
                message,
                class
                    .bases
                    .first()
                    .map_or(Span::new(0, 0), |base| base.span),
            ));
        }
    }
    if diagnostics.is_empty() {
        for class in classes.iter_mut() {
            let mut activation = Vec::new();
            append_base_activation(class.id, &adjacency, &mut activation);
            class.lifecycle.activation = activation
                .iter()
                .copied()
                .map(LifecycleStep::ActivateClass)
                .collect();
            class.lifecycle.deactivation = activation
                .into_iter()
                .rev()
                .map(LifecycleStep::DeactivateClass)
                .collect();
        }
    }
}

fn validate_base_closure(
    current: ClassId,
    adjacency: &HashMap<ClassId, Vec<ClassId>>,
    names: &HashMap<ClassId, String>,
    seen: &mut HashSet<ClassId>,
    path: &mut Vec<ClassId>,
) -> Result<(), String> {
    for base in adjacency.get(&current).into_iter().flatten().copied() {
        if path.contains(&base) {
            return Err(format!(
                "inheritance cycle involving `{}`",
                names.get(&base).unwrap()
            ));
        }
        if !seen.insert(base) {
            return Err(format!(
                "repeated concrete base `{}` is not allowed",
                names.get(&base).unwrap()
            ));
        }
        path.push(base);
        validate_base_closure(base, adjacency, names, seen, path)?;
        path.pop();
    }
    Ok(())
}

fn append_base_activation(
    class: ClassId,
    adjacency: &HashMap<ClassId, Vec<ClassId>>,
    output: &mut Vec<ClassId>,
) {
    for base in adjacency.get(&class).into_iter().flatten().copied() {
        append_base_activation(base, adjacency, output);
    }
    if output.is_empty() || output.last() != Some(&class) {
        output.push(class);
    }
}

fn parse_construct_expression(expression: &str) -> Option<(&str, &str)> {
    let rest = expression.trim().strip_prefix("construct")?.trim_start();
    let open = rest.find('(')?;
    let target = rest[..open].trim();
    let arguments = rest[open + 1..].strip_suffix(')')?;
    (!target.is_empty()).then_some((target, arguments))
}

fn body_mentions_identifier(body: &str, name: &str) -> bool {
    // Rust format strings can capture identifiers (`"{name}"`) even though
    // the lexer correctly treats the literal as one token. Boundary matching
    // covers both ordinary paths and macro captures; rustc remains the final
    // ownership checker for a parameter used by both `new` and `init`.
    type_contains_name(body, name)
}

fn lower_methods(
    methods: impl Iterator<Item = ast::MethodDef>,
    owner: ClassId,
    ordinary_class: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Method> {
    let mut lowered = Vec::new();
    let mut names = HashSet::new();
    for method in methods {
        let span = ast::span(method.syntax());
        let signature_source = method
            .signature()
            .map(|signature| signature.syntax().text().to_string())
            .unwrap_or_default();
        let signature = match syn::parse_str::<syn::Signature>(&signature_source) {
            Ok(signature) => signature,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    format!("invalid method signature: {error}"),
                    span,
                ));
                continue;
            }
        };
        let name = signature.ident.to_string();
        if !names.insert(name.clone()) {
            diagnostics.push(Diagnostic::error(
                format!("duplicate method `{name}`"),
                span,
            ));
        }
        let is_virtual = method.has_modifier(crate::syntax::SyntaxKind::VIRTUAL_KW);
        let is_override = method.has_modifier(crate::syntax::SyntaxKind::OVERRIDE_KW);
        let is_final = method.has_modifier(crate::syntax::SyntaxKind::FINAL_KW);
        if is_virtual && is_override {
            diagnostics.push(Diagnostic::error(
                "a method cannot be both `virtual` and `override`",
                span,
            ));
        }
        if is_final && !is_override {
            diagnostics.push(Diagnostic::error(
                "`final` is only valid on an `override` method",
                span,
            ));
        }
        if !ordinary_class && (is_virtual || is_override || is_final) {
            diagnostics.push(Diagnostic::error(
                "value-class methods cannot be virtual or override class slots",
                span,
            ));
        }
        let kind = if is_override {
            MethodKind::Override { final_: is_final }
        } else if is_virtual {
            MethodKind::Virtual
        } else {
            MethodKind::NonVirtual
        };
        if !matches!(kind, MethodKind::NonVirtual) && !signature.generics.params.is_empty() {
            diagnostics.push(Diagnostic::error(
                "virtual methods cannot have type, lifetime, or const parameters",
                span,
            ));
        }
        let Some(syn::FnArg::Receiver(receiver)) = signature.inputs.first() else {
            diagnostics.push(Diagnostic::error(
                "class methods require a borrowed `&self` or `&mut self` receiver",
                span,
            ));
            continue;
        };
        if receiver.reference.is_none() {
            diagnostics.push(Diagnostic::error(
                "class methods cannot take `self` by value",
                span,
            ));
        }
        let _mutable = receiver.mutability.is_some();
        for input in signature.inputs.iter().skip(1) {
            let syn::FnArg::Typed(input) = input else {
                continue;
            };
            let syn::Pat::Ident(_ident) = input.pat.as_ref() else {
                diagnostics.push(Diagnostic::error(
                    "method parameters must use identifier patterns",
                    span,
                ));
                continue;
            };
        }
        let body = method.body().map(|body| body.body_text());
        if body.is_none() && !matches!(kind, MethodKind::Virtual) {
            diagnostics.push(Diagnostic::error(
                "only a newly declared virtual method may omit its body",
                span,
            ));
        }
        lowered.push(Method {
            id: MethodId {
                owner,
                index: lowered.len(),
            },
            name,
            signature: signature_source,
            public: method.has_modifier(crate::syntax::SyntaxKind::PUB_KW),
            kind,
            body,
            span,
            slot: None,
        });
    }
    lowered
}

fn validate_method_slots(classes: &mut [Class], diagnostics: &mut Vec<Diagnostic>) {
    let adjacency: HashMap<ClassId, Vec<ClassId>> = classes
        .iter()
        .map(|class| {
            (
                class.id,
                class.bases.iter().map(|base| base.class).collect(),
            )
        })
        .collect();
    let mut order = Vec::new();
    let mut ordered = HashSet::new();
    for class in classes.iter() {
        append_class_topological(class.id, &adjacency, &mut ordered, &mut order);
    }

    for class_id in order {
        let index = classes
            .iter()
            .position(|class| class.id == class_id)
            .unwrap();
        let ancestors = ancestor_ids(class_id, &adjacency);
        let inherited: Vec<_> = classes
            .iter()
            .filter(|class| ancestors.contains(&class.id))
            .flat_map(|class| class.methods.iter())
            .filter_map(|method| {
                method.slot.map(|slot| {
                    (
                        method.id,
                        slot,
                        method.name.clone(),
                        signature_shape(&method.signature),
                        matches!(method.kind, MethodKind::Override { final_: true }),
                    )
                })
            })
            .collect();
        for method in &mut classes[index].methods {
            match method.kind {
                MethodKind::Virtual => method.slot = Some(method.id),
                MethodKind::Override { .. } => {
                    let candidates: Vec<_> = inherited
                        .iter()
                        .filter(|(_, _, name, shape, _)| {
                            *name == method.name && *shape == signature_shape(&method.signature)
                        })
                        .collect();
                    let slots: HashSet<_> = candidates.iter().map(|item| item.1).collect();
                    if slots.len() != 1 {
                        diagnostics.push(Diagnostic::error(
                            if slots.is_empty() {
                                format!(
                                    "override `{}` does not match an inherited virtual slot",
                                    method.name
                                )
                            } else {
                                format!(
                                    "override `{}` is ambiguous between inherited virtual slots",
                                    method.name
                                )
                            },
                            method.span,
                        ));
                    } else if candidates.iter().any(|candidate| candidate.4) {
                        diagnostics.push(Diagnostic::error(
                            format!("cannot override final method `{}`", method.name),
                            method.span,
                        ));
                    } else {
                        method.slot = slots.into_iter().next();
                    }
                }
                MethodKind::NonVirtual => {
                    if inherited
                        .iter()
                        .any(|(_, _, name, _, _)| *name == method.name)
                    {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "method `{}` hides an inherited virtual slot; declare it `override`",
                                method.name
                            ),
                            method.span,
                        ));
                    }
                }
            }
        }
    }

    let snapshots: HashMap<ClassId, Vec<Method>> = classes
        .iter()
        .map(|class| (class.id, class.methods.clone()))
        .collect();
    for class in classes {
        let mut closure = ancestor_ids(class.id, &adjacency);
        closure.push(class.id);
        let mut slots = HashSet::new();
        for id in &closure {
            for method in &snapshots[id] {
                if let Some(slot) = method.slot {
                    slots.insert(slot);
                }
            }
        }
        let has_unimplemented_slot = slots.into_iter().any(|slot| {
            closure
                .iter()
                .rev()
                .flat_map(|id| snapshots[id].iter())
                .find(|method| method.slot == Some(slot))
                .is_none_or(|method| method.body.is_none())
        });
        if has_unimplemented_slot && !class.declared_abstract {
            diagnostics.push(Diagnostic::error(
                format!(
                    "class `{}` has unimplemented virtual slots and must be declared `abstract`",
                    class.name
                ),
                class
                    .methods
                    .first()
                    .map_or(Span::new(0, 0), |method| method.span),
            ));
        }
        if class.declared_abstract && class.final_ {
            diagnostics.push(Diagnostic::error(
                format!("class `{}` cannot be both abstract and final", class.name),
                class
                    .methods
                    .first()
                    .map_or(Span::new(0, 0), |method| method.span),
            ));
        }
        class.abstract_ = class.declared_abstract || has_unimplemented_slot;
    }
}

fn append_class_topological(
    class: ClassId,
    adjacency: &HashMap<ClassId, Vec<ClassId>>,
    seen: &mut HashSet<ClassId>,
    output: &mut Vec<ClassId>,
) {
    if !seen.insert(class) {
        return;
    }
    for base in adjacency.get(&class).into_iter().flatten().copied() {
        append_class_topological(base, adjacency, seen, output);
    }
    output.push(class);
}

fn ancestor_ids(class: ClassId, adjacency: &HashMap<ClassId, Vec<ClassId>>) -> Vec<ClassId> {
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    fn visit(
        class: ClassId,
        adjacency: &HashMap<ClassId, Vec<ClassId>>,
        seen: &mut HashSet<ClassId>,
        output: &mut Vec<ClassId>,
    ) {
        for base in adjacency.get(&class).into_iter().flatten().copied() {
            if seen.insert(base) {
                visit(base, adjacency, seen, output);
                output.push(base);
            }
        }
    }
    visit(class, adjacency, &mut seen, &mut output);
    output
}

fn signature_shape(source: &str) -> String {
    use quote::ToTokens;
    let mut signature = syn::parse_str::<syn::Signature>(source)
        .expect("method signatures were validated during lowering");
    signature.ident = syn::Ident::new("__slot", signature.ident.span());
    for input in signature.inputs.iter_mut().skip(1) {
        if let syn::FnArg::Typed(input) = input {
            *input.pat = syn::parse_quote!(__arg);
        }
    }
    signature.to_token_stream().to_string()
}

fn unresolved_type(text: &str, span: Option<Span>) -> TypeRef {
    TypeRef {
        source: text.trim().to_owned(),
        kind: TypeKind::Value,
        span: span.unwrap_or(Span::new(0, 0)),
    }
}

fn resolve_type_kinds(
    value_classes: &mut [ValueClass],
    classes: &mut [Class],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let class_ids: HashMap<String, ClassId> = classes
        .iter()
        .map(|class| (class.name.clone(), class.id))
        .collect();

    for value_class in value_classes {
        for field in &mut value_class.fields {
            field.ty.kind = classify_type(&field.ty.source, &class_ids);
            if matches!(
                field.ty.kind,
                TypeKind::ExactClass(_) | TypeKind::InvalidClassValue(_)
            ) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "ordinary class type is not a value and cannot be stored in value class field `{}`",
                        field.name
                    ),
                    field.ty.span,
                ));
            }
        }
        for param in &mut value_class.constructor.params {
            param.ty.kind = classify_type(&param.ty.source, &class_ids);
            if matches!(
                param.ty.kind,
                TypeKind::ExactClass(_) | TypeKind::InvalidClassValue(_)
            ) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "ordinary class constructor parameter `{}` must be a view or stable owner, not a bare class value",
                        param.name
                    ),
                    param.ty.span,
                ));
            }
        }
        validate_method_signature_types(&value_class.methods, &class_ids, diagnostics);
    }

    for class in classes {
        for field in &mut class.fields {
            field.ty.kind = classify_type(&field.ty.source, &class_ids);
            if matches!(field.ty.kind, TypeKind::InvalidClassValue(_)) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "ordinary class `{}` cannot appear inside value container field `{}`",
                        class.name, field.name
                    ),
                    field.ty.span,
                ));
            }
        }
        for param in &mut class.constructor.params {
            param.ty.kind = classify_type(&param.ty.source, &class_ids);
            if matches!(
                param.ty.kind,
                TypeKind::ExactClass(_) | TypeKind::InvalidClassValue(_)
            ) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "ordinary class constructor parameter `{}` must be a view or stable owner, not a bare class",
                        param.name
                    ),
                    param.ty.span,
                ));
            }
        }
        validate_method_signature_types(&class.methods, &class_ids, diagnostics);
    }
}

fn validate_method_signature_types(
    methods: &[Method],
    classes: &HashMap<String, ClassId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use quote::ToTokens;
    for method in methods {
        let signature = syn::parse_str::<syn::Signature>(&method.signature)
            .expect("method signatures were validated during lowering");
        let invalid_input = signature.inputs.iter().any(|input| match input {
            syn::FnArg::Receiver(_) => false,
            syn::FnArg::Typed(input) => matches!(
                classify_type(&input.ty.to_token_stream().to_string(), classes),
                TypeKind::ExactClass(_) | TypeKind::InvalidClassValue(_)
            ),
        });
        let invalid_output = match &signature.output {
            syn::ReturnType::Default => false,
            syn::ReturnType::Type(_, output) => matches!(
                classify_type(&output.to_token_stream().to_string(), classes),
                TypeKind::ExactClass(_) | TypeKind::InvalidClassValue(_)
            ),
        };
        if invalid_input || invalid_output {
            diagnostics.push(Diagnostic::error(
                format!(
                    "method `{}` cannot pass or return an ordinary class as a movable value; use a view or stable owner",
                    method.name
                ),
                method.span,
            ));
        }
    }
}

fn classify_type(source: &str, classes: &HashMap<String, ClassId>) -> TypeKind {
    fn direct_class(ty: &syn::Type, classes: &HashMap<String, ClassId>) -> Option<ClassId> {
        let syn::Type::Path(path) = ty else {
            return None;
        };
        (path.qself.is_none() && path.path.segments.len() == 1)
            .then(|| path.path.segments.first().unwrap())
            .filter(|segment| matches!(segment.arguments, syn::PathArguments::None))
            .and_then(|segment| classes.get(&segment.ident.to_string()).copied())
    }

    fn owner_class(
        ty: &syn::Type,
        classes: &HashMap<String, ClassId>,
    ) -> Option<(OwnerKind, ClassId)> {
        let syn::Type::Path(path) = ty else {
            return None;
        };
        if path.qself.is_some() || path.path.segments.len() != 1 {
            return None;
        }
        let segment = path.path.segments.first().unwrap();
        let owner = match segment.ident.to_string().as_str() {
            "Box" => OwnerKind::Box,
            "Rc" => OwnerKind::Rc,
            "Arc" => OwnerKind::Arc,
            _ => return None,
        };
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return None;
        };
        if arguments.args.len() != 1 {
            return None;
        }
        let syn::GenericArgument::Type(inner) = arguments.args.first().unwrap() else {
            return None;
        };
        direct_class(inner, classes).map(|class| (owner, class))
    }

    fn first_bare_class(ty: &syn::Type, classes: &HashMap<String, ClassId>) -> Option<ClassId> {
        if let Some(class) = direct_class(ty, classes) {
            return Some(class);
        }
        if owner_class(ty, classes).is_some() {
            return None;
        }
        match ty {
            syn::Type::Reference(reference) if direct_class(&reference.elem, classes).is_some() => {
                None
            }
            syn::Type::Reference(reference) => first_bare_class(&reference.elem, classes),
            syn::Type::Path(path) => path.path.segments.iter().find_map(|segment| {
                let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                    return None;
                };
                arguments.args.iter().find_map(|argument| match argument {
                    syn::GenericArgument::Type(inner) => first_bare_class(inner, classes),
                    syn::GenericArgument::AssocType(assoc) => first_bare_class(&assoc.ty, classes),
                    _ => None,
                })
            }),
            syn::Type::Array(array) => first_bare_class(&array.elem, classes),
            syn::Type::BareFn(function) => function
                .inputs
                .iter()
                .find_map(|input| first_bare_class(&input.ty, classes))
                .or_else(|| match &function.output {
                    syn::ReturnType::Default => None,
                    syn::ReturnType::Type(_, output) => first_bare_class(output, classes),
                }),
            syn::Type::Group(group) => first_bare_class(&group.elem, classes),
            syn::Type::Paren(paren) => first_bare_class(&paren.elem, classes),
            syn::Type::Ptr(pointer) => first_bare_class(&pointer.elem, classes),
            syn::Type::Slice(slice) => first_bare_class(&slice.elem, classes),
            syn::Type::Tuple(tuple) => tuple
                .elems
                .iter()
                .find_map(|element| first_bare_class(element, classes)),
            _ => None,
        }
    }

    let Ok(ty) = syn::parse_str::<syn::Type>(source) else {
        return TypeKind::Value;
    };
    if let Some(class) = direct_class(&ty, classes) {
        return TypeKind::ExactClass(class);
    }
    if let Some((owner, class)) = owner_class(&ty, classes) {
        return TypeKind::ClassOwner { owner, class };
    }
    if let syn::Type::Reference(reference) = &ty
        && let Some(class) = direct_class(&reference.elem, classes)
    {
        return TypeKind::ClassBorrow {
            mutable: reference.mutability.is_some(),
            class,
        };
    }
    first_bare_class(&ty, classes).map_or(TypeKind::Value, TypeKind::InvalidClassValue)
}

fn type_contains_name(source: &str, name: &str) -> bool {
    source.match_indices(name).any(|(index, _)| {
        let before = source[..index].chars().next_back();
        let after = source[index + name.len()..].chars().next();
        before.is_none_or(|ch| !ch.is_alphanumeric() && ch != '_')
            && after.is_none_or(|ch| !ch.is_alphanumeric() && ch != '_')
    })
}
