pub mod diagnostic;
pub mod lexer;

pub use diagnostic::{Diagnostic, DiagnosticCode, Severity, Span};
pub use lexer::{lex, Token, TokenKind};
