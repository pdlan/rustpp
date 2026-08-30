use crate::diagnostic::{Diagnostic, Span};
use crate::syntax::SyntaxKind;

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: SyntaxKind,
    pub span: Span,
}

pub fn lex(source: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut diagnostics = Vec::new();
    let mut cursor = 0;

    while cursor < bytes.len() {
        let start = cursor;
        let byte = bytes[cursor];
        let kind = if byte.is_ascii_whitespace() {
            cursor += 1;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            SyntaxKind::WHITESPACE
        } else if source[start..].starts_with("//") {
            cursor += 2;
            while cursor < bytes.len() && bytes[cursor] != b'\n' {
                cursor += 1;
            }
            SyntaxKind::COMMENT
        } else if source[start..].starts_with("/*") {
            cursor += 2;
            let mut depth = 1usize;
            while cursor < bytes.len() && depth > 0 {
                if source[cursor..].starts_with("/*") {
                    depth += 1;
                    cursor += 2;
                } else if source[cursor..].starts_with("*/") {
                    depth -= 1;
                    cursor += 2;
                } else {
                    cursor += utf8_width(bytes[cursor]);
                }
            }
            if depth != 0 {
                diagnostics.push(Diagnostic::lexical(
                    "unterminated block comment",
                    Span::new(start, source.len()),
                ));
            }
            SyntaxKind::COMMENT
        } else if let Some((end, terminated)) = (byte == b'r')
            .then(|| raw_string_end(source, start))
            .flatten()
        {
            cursor = end;
            if !terminated {
                diagnostics.push(Diagnostic::lexical(
                    "unterminated raw string literal",
                    Span::new(start, source.len()),
                ));
            }
            SyntaxKind::STRING
        } else if is_ident_start(byte) {
            cursor += 1;
            while cursor < bytes.len() && is_ident_continue(bytes[cursor]) {
                cursor += 1;
            }
            match &source[start..cursor] {
                "value" => SyntaxKind::VALUE_KW,
                "class" => SyntaxKind::CLASS_KW,
                "constructor" => SyntaxKind::CONSTRUCTOR_KW,
                "new" => SyntaxKind::NEW_KW,
                "pub" => SyntaxKind::PUB_KW,
                "init" => SyntaxKind::INIT_KW,
                "destructor" => SyntaxKind::DESTRUCTOR_KW,
                "deinit" => SyntaxKind::DEINIT_KW,
                "drop" => SyntaxKind::DROP_KW,
                "public" => SyntaxKind::PUBLIC_KW,
                "protected" => SyntaxKind::PROTECTED_KW,
                "private" => SyntaxKind::PRIVATE_KW,
                "base" => SyntaxKind::BASE_KW,
                "fn" => SyntaxKind::FN_KW,
                "virtual" => SyntaxKind::VIRTUAL_KW,
                "override" => SyntaxKind::OVERRIDE_KW,
                "final" => SyntaxKind::FINAL_KW,
                "abstract" => SyntaxKind::ABSTRACT_KW,
                _ => SyntaxKind::IDENT,
            }
        } else if byte.is_ascii_digit() {
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || matches!(bytes[cursor], b'_' | b'.'))
            {
                cursor += 1;
            }
            SyntaxKind::NUMBER
        } else if byte == b'"' {
            cursor = quoted_end(source, start, b'"', &mut diagnostics);
            SyntaxKind::STRING
        } else if byte == b'\'' && start + 1 < bytes.len() && is_ident_start(bytes[start + 1]) && {
            let mut lifetime_end = start + 2;
            while lifetime_end < bytes.len() && is_ident_continue(bytes[lifetime_end]) {
                lifetime_end += 1;
            }
            if bytes.get(lifetime_end) != Some(&b'\'') {
                cursor = lifetime_end;
                true
            } else {
                false
            }
        } {
            SyntaxKind::PUNCT
        } else if byte == b'\'' {
            cursor = quoted_end(source, start, b'\'', &mut diagnostics);
            SyntaxKind::CHAR
        } else {
            cursor += 1;
            match byte {
                b'{' => SyntaxKind::L_BRACE,
                b'}' => SyntaxKind::R_BRACE,
                b'(' => SyntaxKind::L_PAREN,
                b')' => SyntaxKind::R_PAREN,
                b'[' => SyntaxKind::L_BRACKET,
                b']' => SyntaxKind::R_BRACKET,
                b'<' => SyntaxKind::L_ANGLE,
                b'>' => SyntaxKind::R_ANGLE,
                b',' => SyntaxKind::COMMA,
                b';' => SyntaxKind::SEMICOLON,
                b':' if cursor < bytes.len() && bytes[cursor] == b':' => {
                    cursor += 1;
                    SyntaxKind::COLON_COLON
                }
                b':' => SyntaxKind::COLON,
                _ => SyntaxKind::PUNCT,
            }
        };
        tokens.push(Token {
            kind,
            span: Span::new(start, cursor),
        });
    }

    (tokens, diagnostics)
}

fn is_ident_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic() || byte >= 0x80
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

fn utf8_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn quoted_end(source: &str, start: usize, quote: u8, diagnostics: &mut Vec<Diagnostic>) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = start + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
        } else if bytes[cursor] == quote {
            return cursor + 1;
        } else {
            cursor += utf8_width(bytes[cursor]);
        }
    }
    diagnostics.push(Diagnostic::error(
        "unterminated quoted literal",
        Span::new(start, source.len()),
    ));
    source.len()
}

fn raw_string_end(source: &str, start: usize) -> Option<(usize, bool)> {
    let bytes = source.as_bytes();
    if bytes.get(start) != Some(&b'r') {
        return None;
    }
    let mut cursor = start + 1;
    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    let hashes = cursor - start - 1;
    cursor += 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && bytes.get(cursor + 1..cursor + 1 + hashes) == Some(&vec![b'#'; hashes][..])
        {
            return Some((cursor + 1 + hashes, true));
        }
        cursor += utf8_width(bytes[cursor]);
    }
    Some((source.len(), false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn losslessly_tokenizes_comments_and_raw_strings() {
        let source = "/* outer /* nested */ done */ value class X { x: String, constructor(x: String) { new { x: r#\"a,b}\"# } } }";
        let (tokens, diagnostics) = lex(source);
        assert!(diagnostics.is_empty());
        let rebuilt: String = tokens
            .iter()
            .map(|token| &source[token.span.start..token.span.end])
            .collect();
        assert_eq!(rebuilt, source);
        assert!(tokens.iter().any(|token| token.kind == SyntaxKind::STRING));
    }

    #[test]
    fn diagnoses_unterminated_raw_string() {
        let (_, diagnostics) = lex("r###\"never closed");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].message, "unterminated raw string literal");
    }
}
