// @generated-compatible typed AST facade described by grammar/rustpp.ungram.

use crate::diagnostic::Span;
use crate::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

pub trait AstNode: Sized {
    const KIND: SyntaxKind;

    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;
}

macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            const KIND: SyntaxKind = SyntaxKind::$kind;

            fn cast(node: SyntaxNode) -> Option<Self> {
                (node.kind() == Self::KIND).then_some(Self(node))
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

include!("ast/generated.rs");

impl SourceFile {
    pub fn new(root: SyntaxNode) -> Option<Self> {
        Self::cast(root)
    }

    pub fn value_classes(&self) -> impl Iterator<Item = ValueClass> + '_ {
        self.syntax().children().filter_map(ValueClass::cast)
    }

    pub fn classes(&self) -> impl Iterator<Item = ClassDef> + '_ {
        self.syntax().children().filter_map(ClassDef::cast)
    }

    pub fn rust_items(&self) -> impl Iterator<Item = RustItem> + '_ {
        self.syntax().children().filter_map(RustItem::cast)
    }
}

impl RustItem {
    pub fn source_text(&self) -> String {
        self.syntax().text().to_string()
    }
}

impl ClassDef {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        direct_tokens(self.syntax())
            .find(|token| matches!(token.kind(), SyntaxKind::IDENT | SyntaxKind::VALUE_KW))
    }

    pub fn is_abstract(&self) -> bool {
        direct_tokens(self.syntax()).any(|token| token.kind() == SyntaxKind::ABSTRACT_KW)
    }

    pub fn is_final(&self) -> bool {
        direct_tokens(self.syntax()).any(|token| token.kind() == SyntaxKind::FINAL_KW)
    }

    pub fn fields(&self) -> impl Iterator<Item = FieldDef> + '_ {
        self.syntax().children().filter_map(FieldDef::cast)
    }

    pub fn constructors(&self) -> impl Iterator<Item = ConstructorDef> + '_ {
        self.syntax().children().filter_map(ConstructorDef::cast)
    }

    pub fn destructors(&self) -> impl Iterator<Item = DestructorDef> + '_ {
        self.syntax().children().filter_map(DestructorDef::cast)
    }

    pub fn methods(&self) -> impl Iterator<Item = MethodDef> + '_ {
        self.syntax().children().filter_map(MethodDef::cast)
    }

    pub fn bases(&self) -> impl Iterator<Item = BaseSpec> + '_ {
        self.syntax()
            .children()
            .filter_map(BaseList::cast)
            .flat_map(|list| list.syntax().children().filter_map(BaseSpec::cast))
    }
}

impl BaseSpec {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        direct_tokens(self.syntax()).find(|token| token.kind() == SyntaxKind::IDENT)
    }

    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        direct_tokens(self.syntax()).find(|token| {
            matches!(
                token.kind(),
                SyntaxKind::PUBLIC_KW | SyntaxKind::PROTECTED_KW | SyntaxKind::PRIVATE_KW
            )
        })
    }
}

impl ValueClass {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        direct_tokens(self.syntax()).find(|token| token.kind() == SyntaxKind::IDENT)
    }

    pub fn generic_params(&self) -> Option<GenericParamList> {
        self.syntax().children().find_map(GenericParamList::cast)
    }

    pub fn fields(&self) -> impl Iterator<Item = FieldDef> + '_ {
        self.syntax().children().filter_map(FieldDef::cast)
    }

    pub fn constructors(&self) -> impl Iterator<Item = ConstructorDef> + '_ {
        self.syntax().children().filter_map(ConstructorDef::cast)
    }

    pub fn methods(&self) -> impl Iterator<Item = MethodDef> + '_ {
        self.syntax().children().filter_map(MethodDef::cast)
    }

    pub fn destructors(&self) -> impl Iterator<Item = DestructorDef> + '_ {
        self.syntax().children().filter_map(DestructorDef::cast)
    }
}

impl FieldDef {
    pub fn is_public(&self) -> bool {
        direct_tokens(self.syntax()).any(|token| token.kind() == SyntaxKind::PUB_KW)
    }

    pub fn name_token(&self) -> Option<SyntaxToken> {
        direct_tokens(self.syntax())
            .find(|token| matches!(token.kind(), SyntaxKind::IDENT | SyntaxKind::VALUE_KW))
    }

    pub fn ty(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
}

impl ConstructorDef {
    pub fn params(&self) -> impl Iterator<Item = Param> + '_ {
        self.syntax().descendants().filter_map(Param::cast)
    }

    pub fn new_expr(&self) -> Option<NewExpr> {
        self.syntax().children().find_map(NewExpr::cast)
    }

    pub fn init_block(&self) -> Option<InitBlock> {
        self.syntax().children().find_map(InitBlock::cast)
    }
}

impl DestructorDef {
    pub fn deinit_block(&self) -> Option<DeinitBlock> {
        self.syntax().children().find_map(DeinitBlock::cast)
    }

    pub fn drop_block(&self) -> Option<DropBlock> {
        self.syntax().children().find_map(DropBlock::cast)
    }
}

macro_rules! lifecycle_body {
    ($node:ident) => {
        impl $node {
            pub fn body(&self) -> Option<Block> {
                self.syntax().children().find_map(Block::cast)
            }
        }
    };
}

lifecycle_body!(InitBlock);
lifecycle_body!(DeinitBlock);
lifecycle_body!(DropBlock);

impl Block {
    pub fn body_text(&self) -> String {
        let text = self.syntax().text().to_string();
        let text = text.trim();
        text.strip_prefix('{')
            .and_then(|text| text.strip_suffix('}'))
            .unwrap_or(text)
            .to_owned()
    }
}

impl MethodDef {
    pub fn signature(&self) -> Option<MethodSignature> {
        self.syntax().children().find_map(MethodSignature::cast)
    }

    pub fn body(&self) -> Option<Block> {
        self.syntax().children().find_map(Block::cast)
    }

    pub fn has_modifier(&self, kind: SyntaxKind) -> bool {
        direct_tokens(self.syntax()).any(|token| token.kind() == kind)
    }
}

impl Param {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        direct_tokens(self.syntax())
            .find(|token| matches!(token.kind(), SyntaxKind::IDENT | SyntaxKind::VALUE_KW))
    }

    pub fn ty(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
}

impl NewExpr {
    pub fn fields(&self) -> impl Iterator<Item = NewField> + '_ {
        self.syntax().children().filter_map(NewField::cast)
    }

    pub fn bases(&self) -> impl Iterator<Item = BaseInit> + '_ {
        self.syntax().children().filter_map(BaseInit::cast)
    }
}

impl BaseInit {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        direct_tokens(self.syntax()).find(|token| token.kind() == SyntaxKind::IDENT)
    }

    pub fn arguments(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl NewField {
    pub fn name_token(&self) -> Option<SyntaxToken> {
        direct_tokens(self.syntax())
            .find(|token| matches!(token.kind(), SyntaxKind::IDENT | SyntaxKind::VALUE_KW))
    }

    pub fn expression(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

pub fn span(node: &SyntaxNode) -> Span {
    let range = node.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}

pub fn token_span(token: &SyntaxToken) -> Span {
    let range = token.text_range();
    Span::new(
        u32::from(range.start()) as usize,
        u32::from(range.end()) as usize,
    )
}

fn direct_tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + '_ {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
}
