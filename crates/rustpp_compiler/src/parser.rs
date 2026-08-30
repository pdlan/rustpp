use rowan::GreenNodeBuilder;

use crate::diagnostic::{Diagnostic, Span};
use crate::lexer::{self, Token};
use crate::syntax::{SyntaxKind, SyntaxNode};

pub struct Parse {
    pub syntax: SyntaxNode,
    pub diagnostics: Vec<Diagnostic>,
}

enum Event {
    Start(SyntaxKind),
    Token(usize),
    Finish,
}

pub fn parse(source: &str) -> Parse {
    let (tokens, mut diagnostics) = lexer::lex(source);
    let mut parser = Parser {
        source,
        tokens: &tokens,
        cursor: 0,
        events: Vec::new(),
        diagnostics: Vec::new(),
    };
    parser.source_file();
    diagnostics.extend(parser.diagnostics);

    let mut builder = GreenNodeBuilder::new();
    for event in parser.events {
        match event {
            Event::Start(kind) => builder.start_node(kind.into()),
            Event::Token(index) => {
                let token = &tokens[index];
                builder.token(token.kind.into(), &source[token.span.start..token.span.end]);
            }
            Event::Finish => builder.finish_node(),
        }
    }
    let syntax = SyntaxNode::new_root(builder.finish());
    Parse {
        syntax,
        diagnostics,
    }
}

struct Parser<'a> {
    source: &'a str,
    tokens: &'a [Token],
    cursor: usize,
    events: Vec<Event>,
    diagnostics: Vec<Diagnostic>,
}

impl Parser<'_> {
    fn source_file(&mut self) {
        self.start(SyntaxKind::SOURCE_FILE);
        while self.peek().is_some() {
            let before = self.cursor;
            if self.at(SyntaxKind::VALUE_KW) {
                self.value_class();
            } else if matches!(
                self.peek(),
                Some(SyntaxKind::CLASS_KW | SyntaxKind::ABSTRACT_KW | SyntaxKind::FINAL_KW)
            ) {
                self.class_def();
            } else if self.looks_like_rust_item() {
                self.rust_item();
            } else {
                self.error("expected `value class` or `class`");
                self.bump();
                self.recover_to(&[
                    SyntaxKind::VALUE_KW,
                    SyntaxKind::CLASS_KW,
                    SyntaxKind::ABSTRACT_KW,
                    SyntaxKind::FINAL_KW,
                    SyntaxKind::FN_KW,
                ]);
            }
            self.ensure_progress(before);
        }
        self.eat_trivia();
        self.finish();
    }

    fn looks_like_rust_item(&self) -> bool {
        let mut tokens = self.tokens[self.cursor..]
            .iter()
            .filter(|token| !token.kind.is_trivia())
            .map(|token| (token.kind, &self.source[token.span.start..token.span.end]));
        let Some((kind, text)) = tokens.next() else {
            return false;
        };
        let (kind, text) = if kind == SyntaxKind::PUB_KW {
            let Some(next) = tokens.next() else {
                return false;
            };
            next
        } else {
            (kind, text)
        };
        kind == SyntaxKind::FN_KW
            || matches!(
                text,
                "impl"
                    | "trait"
                    | "use"
                    | "struct"
                    | "enum"
                    | "type"
                    | "const"
                    | "static"
                    | "mod"
                    | "extern"
                    | "unsafe"
            )
    }

    fn rust_item(&mut self) {
        self.start(SyntaxKind::RUST_ITEM);
        let mut brace_depth = 0usize;
        let mut saw_body = false;
        let mut terminated = false;
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::L_BRACE => {
                    saw_body = true;
                    brace_depth += 1;
                    self.bump();
                }
                SyntaxKind::R_BRACE if saw_body => {
                    brace_depth -= 1;
                    self.bump();
                    if brace_depth == 0 {
                        break;
                    }
                }
                SyntaxKind::SEMICOLON if !saw_body => {
                    self.bump();
                    terminated = true;
                    break;
                }
                _ => self.bump(),
            }
        }
        if !saw_body && !terminated {
            self.error("unclosed Rust-compatible item");
        } else if brace_depth != 0 {
            self.error("unclosed Rust++ function body");
        }
        self.finish();
    }

    fn class_def(&mut self) {
        self.start(SyntaxKind::CLASS_DEF);
        while matches!(
            self.peek(),
            Some(SyntaxKind::ABSTRACT_KW | SyntaxKind::FINAL_KW)
        ) {
            self.bump();
        }
        self.expect(SyntaxKind::CLASS_KW, "expected `class`");
        self.expect(SyntaxKind::IDENT, "expected class name");
        if self.at(SyntaxKind::COLON) {
            self.base_list();
        }
        if !self.expect(SyntaxKind::L_BRACE, "expected `{` after class name") {
            self.finish();
            return;
        }
        while self.peek().is_some() && !self.at(SyntaxKind::R_BRACE) {
            let before = self.cursor;
            if self.at(SyntaxKind::CONSTRUCTOR_KW) {
                self.constructor(false);
            } else if self.at(SyntaxKind::DESTRUCTOR_KW) {
                self.destructor();
            } else if self.looks_like_method() {
                self.method();
            } else if self.at(SyntaxKind::PUB_KW)
                || self.at(SyntaxKind::IDENT)
                || self.at(SyntaxKind::VALUE_KW)
            {
                self.field();
            } else {
                self.error("expected a field, constructor, or destructor");
                self.bump();
                self.recover_to(&[
                    SyntaxKind::CONSTRUCTOR_KW,
                    SyntaxKind::DESTRUCTOR_KW,
                    SyntaxKind::PUB_KW,
                    SyntaxKind::IDENT,
                    SyntaxKind::R_BRACE,
                ]);
            }
            self.ensure_progress(before);
        }
        self.expect(SyntaxKind::R_BRACE, "expected `}` to close class");
        self.finish();
    }

    fn base_list(&mut self) {
        self.start(SyntaxKind::BASE_LIST);
        self.bump_assert(SyntaxKind::COLON);
        while self.peek().is_some() && !self.at(SyntaxKind::L_BRACE) {
            let before = self.cursor;
            self.start(SyntaxKind::BASE_SPEC);
            if matches!(
                self.peek(),
                Some(SyntaxKind::PUBLIC_KW | SyntaxKind::PROTECTED_KW | SyntaxKind::PRIVATE_KW)
            ) {
                self.bump();
            }
            self.expect(SyntaxKind::IDENT, "expected base class name");
            self.eat(SyntaxKind::COMMA);
            self.finish();
            if !self.at(SyntaxKind::L_BRACE) && !self.previous_nontrivia_was(SyntaxKind::COMMA) {
                self.error("expected `,` between base classes");
                self.recover_to(&[SyntaxKind::COMMA, SyntaxKind::L_BRACE]);
                self.eat(SyntaxKind::COMMA);
            }
            self.ensure_progress(before);
        }
        self.finish();
    }

    fn value_class(&mut self) {
        self.start(SyntaxKind::VALUE_CLASS);
        self.expect(SyntaxKind::VALUE_KW, "expected `value`");
        self.expect(SyntaxKind::CLASS_KW, "expected `class` after `value`");
        self.expect(SyntaxKind::IDENT, "expected value-class name");
        if self.at(SyntaxKind::L_ANGLE) {
            self.generic_param_list();
        }
        if !self.expect(SyntaxKind::L_BRACE, "expected `{` after value-class name") {
            self.finish();
            return;
        }
        while self.peek().is_some() && !self.at(SyntaxKind::R_BRACE) {
            let before = self.cursor;
            if self.at(SyntaxKind::CONSTRUCTOR_KW) {
                self.constructor(true);
            } else if self.at(SyntaxKind::DESTRUCTOR_KW) {
                self.destructor();
            } else if self.looks_like_method() {
                self.method();
            } else if self.at(SyntaxKind::PUB_KW)
                || self.at(SyntaxKind::IDENT)
                || self.at(SyntaxKind::VALUE_KW)
            {
                self.field();
            } else {
                self.error("expected a field, constructor, method, or structural destructor");
                self.bump();
                self.recover_to(&[
                    SyntaxKind::CONSTRUCTOR_KW,
                    SyntaxKind::DESTRUCTOR_KW,
                    SyntaxKind::PUB_KW,
                    SyntaxKind::IDENT,
                    SyntaxKind::R_BRACE,
                ]);
            }
            self.ensure_progress(before);
        }
        self.expect(SyntaxKind::R_BRACE, "expected `}` to close value class");
        self.finish();
    }

    fn generic_param_list(&mut self) {
        self.start(SyntaxKind::GENERIC_PARAM_LIST);
        let mut depth = 0usize;
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::L_ANGLE => depth += 1,
                SyntaxKind::R_ANGLE => {
                    depth -= 1;
                    self.bump();
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                _ => {}
            }
            self.bump();
        }
        if depth != 0 {
            self.error("unclosed value-class generic parameter list");
        }
        if self.peek_text() == Some("where") {
            while self.peek().is_some() && !self.at(SyntaxKind::L_BRACE) {
                self.bump();
            }
        }
        self.finish();
    }

    fn field(&mut self) {
        self.start(SyntaxKind::FIELD_DEF);
        self.eat(SyntaxKind::PUB_KW);
        self.expect_contextual_identifier("expected field name");
        self.expect(SyntaxKind::COLON, "expected `:` after field name");
        self.type_ref(&[SyntaxKind::COMMA, SyntaxKind::R_BRACE]);
        self.eat(SyntaxKind::COMMA);
        self.finish();
    }

    fn constructor(&mut self, value_class: bool) {
        self.start(SyntaxKind::CONSTRUCTOR_DEF);
        self.bump_assert(SyntaxKind::CONSTRUCTOR_KW);
        self.param_list();
        self.expect(SyntaxKind::L_BRACE, "expected `{` before constructor body");
        if self.at(SyntaxKind::NEW_KW) {
            self.new_expr();
        } else {
            self.error("constructor must contain `new { ... }`");
            self.recover_to(&[SyntaxKind::NEW_KW, SyntaxKind::R_BRACE]);
            if self.at(SyntaxKind::NEW_KW) {
                self.new_expr();
            }
        }
        if !value_class && self.at(SyntaxKind::INIT_KW) {
            self.lifecycle_block(SyntaxKind::INIT_BLOCK, SyntaxKind::INIT_KW);
        }
        self.expect(SyntaxKind::R_BRACE, "expected `}` after constructor body");
        self.finish();
    }

    fn destructor(&mut self) {
        self.start(SyntaxKind::DESTRUCTOR_DEF);
        self.bump_assert(SyntaxKind::DESTRUCTOR_KW);
        if !self.expect(SyntaxKind::L_BRACE, "expected `{` after `destructor`") {
            self.finish();
            return;
        }
        if self.at(SyntaxKind::DEINIT_KW) {
            self.lifecycle_block(SyntaxKind::DEINIT_BLOCK, SyntaxKind::DEINIT_KW);
        }
        if self.at(SyntaxKind::DROP_KW) {
            self.lifecycle_block(SyntaxKind::DROP_BLOCK, SyntaxKind::DROP_KW);
        }
        self.expect(SyntaxKind::R_BRACE, "expected `}` after destructor body");
        self.finish();
    }

    fn lifecycle_block(&mut self, node: SyntaxKind, keyword: SyntaxKind) {
        self.start(node);
        self.bump_assert(keyword);
        self.start(SyntaxKind::BLOCK);
        if !self.expect(SyntaxKind::L_BRACE, "expected lifecycle block") {
            self.finish();
            self.finish();
            return;
        }
        let mut depth = 1usize;
        while self.peek().is_some() && depth > 0 {
            match self.peek().unwrap() {
                SyntaxKind::L_BRACE => {
                    depth += 1;
                    self.bump();
                }
                SyntaxKind::R_BRACE => {
                    depth -= 1;
                    self.bump();
                }
                _ => self.bump(),
            }
        }
        if depth != 0 {
            self.error("unclosed lifecycle block");
        }
        self.finish();
        self.finish();
    }

    fn looks_like_method(&self) -> bool {
        self.tokens[self.cursor..]
            .iter()
            .filter(|token| !token.kind.is_trivia())
            .take(5)
            .map(|token| token.kind)
            .take_while(|kind| {
                matches!(
                    kind,
                    SyntaxKind::PUB_KW
                        | SyntaxKind::VIRTUAL_KW
                        | SyntaxKind::OVERRIDE_KW
                        | SyntaxKind::FINAL_KW
                        | SyntaxKind::FN_KW
                )
            })
            .any(|kind| kind == SyntaxKind::FN_KW)
    }

    fn method(&mut self) {
        self.start(SyntaxKind::METHOD_DEF);
        while matches!(
            self.peek(),
            Some(
                SyntaxKind::PUB_KW
                    | SyntaxKind::VIRTUAL_KW
                    | SyntaxKind::OVERRIDE_KW
                    | SyntaxKind::FINAL_KW
            )
        ) {
            self.bump();
        }
        self.start(SyntaxKind::METHOD_SIGNATURE);
        self.expect(SyntaxKind::FN_KW, "expected `fn`");
        self.expect_contextual_identifier("expected method name");
        let mut delimiters = Vec::new();
        while let Some(kind) = self.peek() {
            if delimiters.is_empty() && matches!(kind, SyntaxKind::L_BRACE | SyntaxKind::SEMICOLON)
            {
                break;
            }
            match kind {
                SyntaxKind::L_PAREN => delimiters.push(SyntaxKind::R_PAREN),
                SyntaxKind::L_BRACKET => delimiters.push(SyntaxKind::R_BRACKET),
                SyntaxKind::L_ANGLE => delimiters.push(SyntaxKind::R_ANGLE),
                close if delimiters.last() == Some(&close) => {
                    delimiters.pop();
                }
                _ => {}
            }
            self.bump();
        }
        if !delimiters.is_empty() {
            self.error("unclosed delimiter in method signature");
        }
        self.finish();
        if self.at(SyntaxKind::SEMICOLON) {
            self.bump();
        } else if self.at(SyntaxKind::L_BRACE) {
            self.plain_block();
        } else {
            self.error("expected method body or `;`");
        }
        self.finish();
    }

    fn plain_block(&mut self) {
        self.start(SyntaxKind::BLOCK);
        self.bump_assert(SyntaxKind::L_BRACE);
        let mut depth = 1usize;
        while self.peek().is_some() && depth > 0 {
            match self.peek().unwrap() {
                SyntaxKind::L_BRACE => {
                    depth += 1;
                    self.bump();
                }
                SyntaxKind::R_BRACE => {
                    depth -= 1;
                    self.bump();
                }
                _ => self.bump(),
            }
        }
        if depth != 0 {
            self.error("unclosed method body");
        }
        self.finish();
    }

    fn param_list(&mut self) {
        self.start(SyntaxKind::PARAM_LIST);
        if !self.expect(SyntaxKind::L_PAREN, "expected constructor parameter list") {
            self.finish();
            return;
        }
        while self.peek().is_some() && !self.at(SyntaxKind::R_PAREN) {
            let before = self.cursor;
            self.start(SyntaxKind::PARAM);
            self.expect_contextual_identifier("expected parameter name");
            self.expect(SyntaxKind::COLON, "expected `:` after parameter name");
            self.type_ref(&[SyntaxKind::COMMA, SyntaxKind::R_PAREN]);
            self.eat(SyntaxKind::COMMA);
            self.finish();
            self.ensure_progress(before);
        }
        self.expect(SyntaxKind::R_PAREN, "expected `)` after parameters");
        self.finish();
    }

    fn type_ref(&mut self, stop: &[SyntaxKind]) {
        self.start(SyntaxKind::TYPE_REF);
        let mut delimiters = Vec::new();
        let mut consumed = false;
        while let Some(kind) = self.peek() {
            if delimiters.is_empty() && stop.contains(&kind) {
                break;
            }
            match kind {
                SyntaxKind::L_ANGLE => delimiters.push(SyntaxKind::R_ANGLE),
                SyntaxKind::L_PAREN => delimiters.push(SyntaxKind::R_PAREN),
                SyntaxKind::L_BRACKET => delimiters.push(SyntaxKind::R_BRACKET),
                close if delimiters.last() == Some(&close) => {
                    delimiters.pop();
                }
                SyntaxKind::IDENT
                | SyntaxKind::COLON_COLON
                | SyntaxKind::COMMA
                | SyntaxKind::R_ANGLE
                | SyntaxKind::R_PAREN
                | SyntaxKind::PUNCT
                | SyntaxKind::SEMICOLON
                | SyntaxKind::R_BRACKET
                | SyntaxKind::NUMBER => {}
                _ => {
                    self.error("unsupported type syntax; expected a Rust type path");
                    self.bump();
                    continue;
                }
            }
            consumed = true;
            self.bump();
        }
        if !consumed {
            self.error("expected a type");
        }
        if !delimiters.is_empty() {
            self.error("unclosed delimiter in type");
        }
        self.finish();
    }

    fn new_expr(&mut self) {
        self.start(SyntaxKind::NEW_EXPR);
        self.bump_assert(SyntaxKind::NEW_KW);
        if !self.expect(SyntaxKind::L_BRACE, "expected `{` after `new`") {
            self.finish();
            return;
        }
        while self.peek().is_some() && !self.at(SyntaxKind::R_BRACE) {
            let before = self.cursor;
            if self.at(SyntaxKind::BASE_KW) {
                self.base_initializer();
            } else {
                self.start(SyntaxKind::NEW_FIELD);
                self.expect_contextual_identifier("expected initialized field name");
                if self.eat(SyntaxKind::COLON) {
                    self.expression();
                }
                self.eat(SyntaxKind::COMMA);
                self.finish();
            }
            if !self.at(SyntaxKind::R_BRACE) && !self.previous_nontrivia_was(SyntaxKind::COMMA) {
                self.error("expected `,` between field initializers");
                self.recover_to(&[SyntaxKind::COMMA, SyntaxKind::R_BRACE]);
                self.eat(SyntaxKind::COMMA);
            }
            self.ensure_progress(before);
        }
        self.expect(SyntaxKind::R_BRACE, "expected `}` after `new` fields");
        self.finish();
    }

    fn base_initializer(&mut self) {
        self.start(SyntaxKind::BASE_INIT);
        self.bump_assert(SyntaxKind::BASE_KW);
        self.expect(SyntaxKind::IDENT, "expected base class name after `base`");
        if self.expect(SyntaxKind::L_PAREN, "expected `(` after base class name") {
            if !self.at(SyntaxKind::R_PAREN) {
                self.expression_until_r_paren();
            }
            self.expect(SyntaxKind::R_PAREN, "expected `)` after base arguments");
        }
        self.eat(SyntaxKind::COMMA);
        self.finish();
    }

    fn expression_until_r_paren(&mut self) {
        self.start(SyntaxKind::EXPR);
        let mut stack = Vec::new();
        while let Some(kind) = self.peek() {
            if stack.is_empty() && kind == SyntaxKind::R_PAREN {
                break;
            }
            match kind {
                SyntaxKind::L_PAREN => stack.push(SyntaxKind::R_PAREN),
                SyntaxKind::L_BRACKET => stack.push(SyntaxKind::R_BRACKET),
                SyntaxKind::L_BRACE => stack.push(SyntaxKind::R_BRACE),
                close if stack.last() == Some(&close) => {
                    stack.pop();
                }
                _ => {}
            }
            self.bump();
        }
        if !stack.is_empty() {
            self.error("unclosed delimiter in base initializer");
        }
        self.finish();
    }

    fn expression(&mut self) {
        self.start(SyntaxKind::EXPR);
        let mut stack = Vec::new();
        let mut consumed = false;
        while let Some(kind) = self.peek() {
            if stack.is_empty() && matches!(kind, SyntaxKind::COMMA | SyntaxKind::R_BRACE) {
                break;
            }
            match kind {
                SyntaxKind::L_PAREN => stack.push(SyntaxKind::R_PAREN),
                SyntaxKind::L_BRACKET => stack.push(SyntaxKind::R_BRACKET),
                SyntaxKind::L_BRACE => stack.push(SyntaxKind::R_BRACE),
                close if stack.last() == Some(&close) => {
                    stack.pop();
                }
                _ => {}
            }
            consumed = true;
            self.bump();
        }
        if !consumed {
            self.error("expected an initializer expression");
        }
        if !stack.is_empty() {
            self.error("unclosed delimiter in initializer expression");
        }
        self.finish();
    }

    fn start(&mut self, kind: SyntaxKind) {
        self.events.push(Event::Start(kind));
    }

    fn finish(&mut self) {
        self.events.push(Event::Finish);
    }

    fn peek(&self) -> Option<SyntaxKind> {
        self.tokens[self.cursor..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .map(|token| token.kind)
    }

    fn peek_text(&self) -> Option<&str> {
        self.tokens[self.cursor..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .map(|token| &self.source[token.span.start..token.span.end])
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.peek() == Some(kind)
    }

    fn eat_trivia(&mut self) {
        while self
            .tokens
            .get(self.cursor)
            .is_some_and(|token| token.kind.is_trivia())
        {
            self.events.push(Event::Token(self.cursor));
            self.cursor += 1;
        }
    }

    fn bump(&mut self) {
        self.eat_trivia();
        if self.cursor < self.tokens.len() {
            self.events.push(Event::Token(self.cursor));
            self.cursor += 1;
        }
    }

    fn bump_assert(&mut self, kind: SyntaxKind) {
        debug_assert!(self.at(kind));
        self.bump();
    }

    fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: SyntaxKind, message: &str) -> bool {
        if self.eat(kind) {
            true
        } else {
            self.error(message);
            false
        }
    }

    fn expect_contextual_identifier(&mut self, message: &str) -> bool {
        if matches!(self.peek(), Some(SyntaxKind::IDENT | SyntaxKind::VALUE_KW)) {
            self.bump();
            true
        } else {
            self.error(message);
            false
        }
    }

    fn error(&mut self, message: &str) {
        let span = self.tokens[self.cursor..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .map_or(Span::new(self.source.len(), self.source.len()), |token| {
                token.span
            });
        self.diagnostics.push(Diagnostic::syntax(message, span));
    }

    fn recover_to(&mut self, kinds: &[SyntaxKind]) {
        while self.peek().is_some_and(|kind| !kinds.contains(&kind)) {
            self.bump();
        }
    }

    fn previous_nontrivia_was(&self, kind: SyntaxKind) -> bool {
        self.tokens[..self.cursor]
            .iter()
            .rev()
            .find(|token| !token.kind.is_trivia())
            .is_some_and(|token| token.kind == kind)
    }

    fn ensure_progress(&mut self, previous_cursor: usize) {
        if self.cursor == previous_cursor && self.peek().is_some() {
            self.error("parser could not recover from this token");
            self.bump();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_tree_is_lossless() {
        let source = "// point\nvalue class Point { x: f64, constructor(x: f64) { new { x } } }";
        let parsed = parse(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.syntax.text().to_string(), source);
    }

    #[test]
    fn recovers_at_the_next_item() {
        let source = "nonsense value class Good { x: i32, constructor(x: i32) { new { x } } }";
        let parsed = parse(source);
        assert!(!parsed.diagnostics.is_empty());
        assert!(
            parsed
                .syntax
                .text()
                .to_string()
                .contains("value class Good")
        );
    }
}
