use rowan::Language;

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    EOF,
    ERROR,
    WHITESPACE,
    COMMENT,
    IDENT,
    NUMBER,
    STRING,
    CHAR,
    VALUE_KW,
    CLASS_KW,
    CONSTRUCTOR_KW,
    NEW_KW,
    PUB_KW,
    INIT_KW,
    DESTRUCTOR_KW,
    DEINIT_KW,
    DROP_KW,
    PUBLIC_KW,
    PROTECTED_KW,
    PRIVATE_KW,
    BASE_KW,
    FN_KW,
    VIRTUAL_KW,
    OVERRIDE_KW,
    FINAL_KW,
    ABSTRACT_KW,
    L_BRACE,
    R_BRACE,
    L_PAREN,
    R_PAREN,
    L_BRACKET,
    R_BRACKET,
    L_ANGLE,
    R_ANGLE,
    COLON,
    COMMA,
    SEMICOLON,
    COLON_COLON,
    PUNCT,
    SOURCE_FILE,
    VALUE_CLASS,
    CLASS_DEF,
    FIELD_DEF,
    CONSTRUCTOR_DEF,
    PARAM_LIST,
    PARAM,
    NEW_EXPR,
    NEW_FIELD,
    TYPE_REF,
    EXPR,
    DESTRUCTOR_DEF,
    INIT_BLOCK,
    DEINIT_BLOCK,
    DROP_BLOCK,
    BLOCK,
    BASE_LIST,
    BASE_SPEC,
    BASE_INIT,
    METHOD_DEF,
    METHOD_SIGNATURE,
    RUST_ITEM,
    GENERIC_PARAM_LIST,
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(self, Self::WHITESPACE | Self::COMMENT)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RustppLanguage {}

impl Language for RustppLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= SyntaxKind::GENERIC_PARAM_LIST as u16);
        // SAFETY: SyntaxKind is repr(u16), contiguous, and bounded above.
        unsafe { std::mem::transmute(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<RustppLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<RustppLanguage>;
