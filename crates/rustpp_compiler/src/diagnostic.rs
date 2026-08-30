use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagnosticCode(pub &'static str);

impl DiagnosticCode {
    pub const LEXICAL: Self = Self("RPP1001");
    pub const SYNTAX: Self = Self("RPP2001");
    pub const SEMANTIC: Self = Self("RPP3001");
    pub const ABI_CONFIGURATION: Self = Self("RPP4001");
    pub const INTERNAL: Self = Self("RPP9001");
    pub const IO: Self = Self("RPP9002");
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub message: String,
    pub span: Option<Span>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self::with_span(DiagnosticCode::SEMANTIC, message, span)
    }

    pub fn lexical(message: impl Into<String>, span: Span) -> Self {
        Self::with_span(DiagnosticCode::LEXICAL, message, span)
    }

    pub fn syntax(message: impl Into<String>, span: Span) -> Self {
        Self::with_span(DiagnosticCode::SYNTAX, message, span)
    }

    pub fn abi_configuration(message: impl Into<String>) -> Self {
        Self::without_span(DiagnosticCode::ABI_CONFIGURATION, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::without_span(DiagnosticCode::INTERNAL, message)
    }

    fn with_span(code: DiagnosticCode, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::without_span(DiagnosticCode::IO, message)
    }

    fn without_span(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            span: None,
        }
    }

    pub fn render(&self, source_name: &str, source: &str) -> String {
        let Some(span) = self.span else {
            return format!("error[{}]: {}", self.code.0, self.message);
        };
        let before = &source[..span.start.min(source.len())];
        let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = before
            .rsplit_once('\n')
            .map_or(before.len(), |(_, tail)| tail.len())
            + 1;
        format!(
            "{source_name}:{line}:{column}: error[{}]: {}",
            self.code.0, self.message
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] {}", self.code.0, self.message)
    }
}
