#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticCode {
    IncompleteString,
    InvalidString,
    InvalidEscape,
    InvalidNumber,
    InvalidCharacter,
    CommentNotAllowed,
    SourceTooLong,
    UnexpectedToken,
    UnexpectedEof,
    UnknownDeclaration,
    InvalidNesting,
    InvalidLiteral,
    DuplicateArgument,
    DepthLimit,
    NodeLimit,
    InvalidManifest,
    InvalidSnapshot,
    ResolvePath,
    ResolveType,
    ResolveScope,
    ResolveField,
    ResolveKey,
    ResolveAction,
    ResolveConstraint,
    ResolveLimit,
    LowerUnsupported,
    LowerLimit,
}

impl DiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IncompleteString => "E_LEX_INCOMPLETE_STRING",
            Self::InvalidString => "E_LEX_STRING",
            Self::InvalidEscape => "E_LEX_ESCAPE",
            Self::InvalidNumber => "E_LEX_NUMBER",
            Self::InvalidCharacter => "E_LEX_CHARACTER",
            Self::CommentNotAllowed => "E_LEX_COMMENT",
            Self::SourceTooLong => "E_SOURCE_TOO_LONG",
            Self::UnexpectedToken => "E_PARSE_TOKEN",
            Self::UnexpectedEof => "E_PARSE_INCOMPLETE",
            Self::UnknownDeclaration => "E_PARSE_DECLARATION",
            Self::InvalidNesting => "E_PARSE_NESTING",
            Self::InvalidLiteral => "E_PARSE_LITERAL",
            Self::DuplicateArgument => "E_PARSE_DUPLICATE_ARGUMENT",
            Self::DepthLimit => "E_PARSE_DEPTH",
            Self::NodeLimit => "E_PARSE_NODES",
            Self::InvalidManifest => "E_MANIFEST",
            Self::InvalidSnapshot => "E_SNAPSHOT",
            Self::ResolvePath => "E_RESOLVE_PATH",
            Self::ResolveType => "E_RESOLVE_TYPE",
            Self::ResolveScope => "E_RESOLVE_SCOPE",
            Self::ResolveField => "E_RESOLVE_FIELD",
            Self::ResolveKey => "E_RESOLVE_KEY",
            Self::ResolveAction => "E_RESOLVE_ACTION",
            Self::ResolveConstraint => "E_RESOLVE_CONSTRAINT",
            Self::ResolveLimit => "E_RESOLVE_LIMIT",
            Self::LowerUnsupported => "E_LOWER_UNSUPPORTED",
            Self::LowerLimit => "E_LOWER_LIMIT",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub span: Span,
    pub declaration_path: Vec<String>,
    pub message: String,
    pub hint: Option<String>,
}
