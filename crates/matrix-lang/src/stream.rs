use crate::{lex, parse_surface, Diagnostic, DiagnosticCode, Severity, Span, Surface, TokenKind};

const MAX_SOURCE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum CandidateStatus {
    Incomplete,
    Rejected(Vec<Diagnostic>),
    Complete(Box<Surface>),
}

#[derive(Default)]
pub struct CandidateStream {
    buffer: Vec<u8>,
    rejected: Option<Vec<Diagnostic>>,
}

impl CandidateStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &[u8]) -> CandidateStatus {
        if let Some(errors) = &self.rejected {
            return CandidateStatus::Rejected(errors.clone());
        }
        let status = self.push_chunk(chunk);
        if let CandidateStatus::Rejected(errors) = &status {
            self.rejected = Some(errors.clone());
        }
        status
    }

    fn push_chunk(&mut self, chunk: &[u8]) -> CandidateStatus {
        if chunk.len() > MAX_SOURCE_BYTES - self.buffer.len() {
            return CandidateStatus::Rejected(vec![Diagnostic {
                code: DiagnosticCode::SourceTooLong,
                severity: Severity::Error,
                span: Span::new(0, self.buffer.len().saturating_add(chunk.len())),
                declaration_path: Vec::new(),
                message: "Matrix source exceeds 256 KiB".to_owned(),
                hint: None,
            }]);
        }
        self.buffer.extend_from_slice(chunk);
        let source = match std::str::from_utf8(&self.buffer) {
            Ok(source) => source,
            Err(error) if error.error_len().is_none() => return CandidateStatus::Incomplete,
            Err(error) => {
                return CandidateStatus::Rejected(vec![Diagnostic {
                    code: DiagnosticCode::InvalidCharacter,
                    severity: Severity::Error,
                    span: Span::new(
                        error.valid_up_to(),
                        error.valid_up_to() + error.error_len().unwrap_or(1),
                    ),
                    declaration_path: Vec::new(),
                    message: "source is not valid UTF-8".to_owned(),
                    hint: None,
                }]);
            }
        };
        match parse_surface(source) {
            Ok(surface) => CandidateStatus::Complete(Box::new(surface)),
            Err(errors) if errors.iter().all(|error| is_incomplete(error, source)) => {
                CandidateStatus::Incomplete
            }
            Err(errors) => CandidateStatus::Rejected(errors),
        }
    }
}

fn is_incomplete(error: &Diagnostic, source: &str) -> bool {
    match error.code {
        DiagnosticCode::UnexpectedEof | DiagnosticCode::IncompleteString => true,
        DiagnosticCode::InvalidNumber if error.span.end == source.len() => {
            source.ends_with(['-', '.', 'e', 'E', '+'])
        }
        DiagnosticCode::InvalidLiteral if error.span.end == source.len() => {
            source.as_bytes().last().is_some_and(u8::is_ascii_digit)
                && !source[error.span.start..error.span.end].starts_with('-')
        }
        DiagnosticCode::UnknownDeclaration
        | DiagnosticCode::UnexpectedToken
        | DiagnosticCode::InvalidNesting
            if error.span.end == source.len() =>
        {
            let last = lex(source)
                .ok()
                .and_then(|tokens| tokens.into_iter().rev().nth(1));
            let token = match last {
                Some(token)
                    if token.kind == TokenKind::Identifier && token.span.end == source.len() =>
                {
                    &source[token.span.start..token.span.end]
                }
                _ => return false,
            };
            const WORDS: &[&str] = &[
                "screen",
                "section",
                "card",
                "col",
                "row",
                "tabs",
                "tab",
                "header",
                "footer",
                "title",
                "text",
                "caption",
                "code",
                "badge",
                "tag",
                "status",
                "label",
                "md",
                "link",
                "stat",
                "kv",
                "icon",
                "image",
                "avatar",
                "timestamp",
                "divider",
                "typing",
                "field",
                "area",
                "toggle",
                "check",
                "time",
                "search",
                "choice",
                "select",
                "multiselect",
                "segmented",
                "slider",
                "stepper",
                "date",
                "button",
                "form",
                "list",
                "table",
                "grid",
                "item",
                "tile",
                "column",
                "progress",
                "steps",
                "banner",
                "empty",
                "error",
                "chat",
                "message",
                "compose",
                "dialog",
                "sheet",
                "menu",
                "tooltip",
                "from",
                "key",
                "one",
                "many",
                "role",
                "body",
                "trigger",
                "send",
                "to",
                "min",
                "max",
                "step",
                "kind",
                "pattern",
                "options",
                "stages",
                "current",
                "tone",
                "value",
                "delta",
                "alt",
                "label",
                "relative",
                "live",
                "fill",
                "collapsible",
                "wrap",
                "nowrap",
                "start",
                "center",
                "end",
                "between",
                "around",
                "primary",
                "danger",
                "confirm",
                "disabled",
                "info",
                "warn",
                "me",
                "them",
                "system",
                "money",
                "number",
                "password",
                "phone",
                "email",
                "url",
                "status",
                "text",
                "date",
                "Retry",
            ];
            !token.is_empty()
                && WORDS.iter().any(|word| {
                    word.starts_with(token)
                        && (error.code != DiagnosticCode::InvalidNesting
                            || word.len() > token.len())
                })
        }
        _ => false,
    }
}
