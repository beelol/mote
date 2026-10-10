use std::collections::{HashMap, HashSet};

use crate::{ast::*, lex, Diagnostic, DiagnosticCode, Severity, Span, Token, TokenKind};

type ParseResult<T> = Result<T, Diagnostic>;

pub fn parse_surface(source: &str) -> Result<Surface, Vec<Diagnostic>> {
    let mut parser = Parser::new(source)?;
    let result = (|| {
        if !parser.is_word("screen") {
            return Err(parser.expected("one screen declaration"));
        }
        let screen = parser.declaration(None, false, 1, 0)?;
        parser.expect(TokenKind::Eof, "end of input")?;
        Ok(Surface { screen })
    })();
    result.map_err(|error| vec![error])
}

/// Parses declarations inserted into a screen body. References remain unresolved.
pub fn parse_fragment(source: &str) -> Result<Vec<Declaration>, Vec<Diagnostic>> {
    let mut parser = Parser::new(source)?;
    parser
        .declarations("screen", false, 1, TokenKind::Eof)
        .map_err(|error| vec![error])
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
    nodes: usize,
    path: Vec<String>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Result<Self, Vec<Diagnostic>> {
        Ok(Self {
            source,
            tokens: lex(source)?,
            cursor: 0,
            nodes: 0,
            path: Vec::new(),
        })
    }

    fn token(&self) -> Token {
        self.tokens[self.cursor]
    }

    fn text(&self) -> &'a str {
        let span = self.token().span;
        &self.source[span.start..span.end]
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.token().kind == kind
    }

    fn is_word(&self, word: &str) -> bool {
        self.at(TokenKind::Identifier) && self.text() == word
    }

    fn take(&mut self, kind: TokenKind) -> Option<Token> {
        if self.at(kind) {
            let token = self.token();
            if kind != TokenKind::Eof {
                self.cursor += 1;
            }
            Some(token)
        } else {
            None
        }
    }

    fn word(&mut self, word: &str) -> Option<Span> {
        if self.is_word(word) {
            let span = self.token().span;
            self.cursor += 1;
            Some(span)
        } else {
            None
        }
    }

    fn expect(&mut self, kind: TokenKind, description: &str) -> ParseResult<Token> {
        self.take(kind).ok_or_else(|| self.expected(description))
    }

    fn expect_word(&mut self, word: &str) -> ParseResult<Span> {
        self.word(word).ok_or_else(|| self.expected(word))
    }

    fn error(&self, code: DiagnosticCode, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code,
            severity: Severity::Error,
            span,
            declaration_path: self.path.clone(),
            message: message.into(),
            hint: None,
        }
    }

    fn expected(&self, description: &str) -> Diagnostic {
        self.error(
            if self.at(TokenKind::Eof) {
                DiagnosticCode::UnexpectedEof
            } else {
                DiagnosticCode::UnexpectedToken
            },
            self.token().span,
            format!("expected {description}"),
        )
    }

    fn identifier(&mut self) -> ParseResult<Spanned<String>> {
        let token = self.expect(TokenKind::Identifier, "an identifier")?;
        Ok(Spanned {
            value: self.source[token.span.start..token.span.end].to_owned(),
            span: token.span,
        })
    }

    fn string(&mut self, required: bool) -> ParseResult<StringLiteral> {
        let token = self.expect(TokenKind::String, "a string literal")?;
        let value: String = serde_json::from_str(&self.source[token.span.start..token.span.end])
            .map_err(|_| {
                self.error(
                    DiagnosticCode::InvalidLiteral,
                    token.span,
                    "invalid string literal",
                )
            })?;
        if required && value.is_empty() {
            return Err(self.error(
                DiagnosticCode::InvalidLiteral,
                token.span,
                "required labels cannot be empty",
            ));
        }
        Ok(Spanned {
            value,
            span: token.span,
        })
    }

    fn optional_title(&mut self) -> ParseResult<Option<StringLiteral>> {
        if self.at(TokenKind::String) {
            self.string(false).map(Some)
        } else {
            Ok(None)
        }
    }

    fn number(&mut self) -> ParseResult<NumberLiteral> {
        let token = self.expect(TokenKind::Number, "a numeric literal")?;
        let source = self.source[token.span.start..token.span.end].to_owned();
        let value = source.parse().map_err(|_| {
            self.error(DiagnosticCode::InvalidLiteral, token.span, "invalid number")
        })?;
        Ok(NumberLiteral {
            source,
            value,
            span: token.span,
        })
    }

    fn optional_number(&mut self, name: &str) -> ParseResult<Option<NumberLiteral>> {
        if self.word(name).is_some() {
            self.number().map(Some)
        } else {
            Ok(None)
        }
    }

    fn path(&mut self) -> ParseResult<Path> {
        let first = self.identifier()?;
        let mut span = first.span;
        let mut segments = vec![first.value];
        while self.take(TokenKind::Dot).is_some() {
            let part = self.identifier()?;
            span.end = part.span.end;
            segments.push(part.value);
        }
        Ok(Path { segments, span })
    }

    fn reference(&mut self) -> ParseResult<Reference> {
        let dot = self.take(TokenKind::Dot);
        let path = self.path()?;
        let span = Span::new(
            dot.map_or(path.span.start, |token| token.span.start),
            path.span.end,
        );
        Ok(Reference {
            path,
            relative: dot.is_some(),
            span,
        })
    }

    fn optional_reference(&mut self, name: &str) -> ParseResult<Option<Reference>> {
        if self.word(name).is_some() {
            self.reference().map(Some)
        } else {
            Ok(None)
        }
    }

    fn display(&mut self) -> ParseResult<DisplayOperand> {
        if self.at(TokenKind::String) {
            self.string(false).map(DisplayOperand::Literal)
        } else {
            self.reference().map(DisplayOperand::Reference)
        }
    }

    fn literal_options(&mut self) -> ParseResult<Vec<StringLiteral>> {
        self.expect(TokenKind::LBracket, "a nonempty string option list")?;
        let mut options = Vec::new();
        let mut seen = HashSet::new();
        loop {
            let option = self.string(true)?;
            if !seen.insert(option.value.clone()) {
                return Err(self.error(
                    DiagnosticCode::InvalidLiteral,
                    option.span,
                    "options and stages must be unique",
                ));
            }
            options.push(option);
            if self.take(TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(TokenKind::RBracket, "the closing option bracket")?;
        Ok(options)
    }

    fn options(&mut self) -> ParseResult<Options> {
        if self.at(TokenKind::LBracket) {
            self.literal_options().map(Options::Literal)
        } else {
            self.reference().map(Options::Reference)
        }
    }

    fn choice<T: Copy>(
        &mut self,
        choices: &[(&str, T)],
        description: &str,
    ) -> ParseResult<Spanned<T>> {
        self.optional_choice(choices)
            .ok_or_else(|| self.expected(description))
    }

    fn optional_choice<T: Copy>(&mut self, choices: &[(&str, T)]) -> Option<Spanned<T>> {
        for &(name, value) in choices {
            if let Some(span) = self.word(name) {
                return Some(Spanned { value, span });
            }
        }
        None
    }

    fn icon(&mut self) -> ParseResult<Spanned<Icon>> {
        let name = self.string(true)?;
        let value = match name.value.as_str() {
            "add" => Icon::Add,
            "back" => Icon::Back,
            "bell" => Icon::Bell,
            "calendar" => Icon::Calendar,
            "check" => Icon::Check,
            "clock" => Icon::Clock,
            "close" => Icon::Close,
            "copy" => Icon::Copy,
            "delete" => Icon::Delete,
            "down" => Icon::Down,
            "edit" => Icon::Edit,
            "error" => Icon::Error,
            "file" => Icon::File,
            "folder" => Icon::Folder,
            "image" => Icon::Image,
            "info" => Icon::Info,
            "left" => Icon::Left,
            "mail" => Icon::Mail,
            "more" => Icon::More,
            "open" => Icon::Open,
            "right" => Icon::Right,
            "search" => Icon::Search,
            "settings" => Icon::Settings,
            "up" => Icon::Up,
            "user" => Icon::User,
            "warning" => Icon::Warning,
            _ => {
                return Err(self.error(
                    DiagnosticCode::InvalidLiteral,
                    name.span,
                    "unknown icon name",
                ))
            }
        };
        Ok(Spanned {
            value,
            span: name.span,
        })
    }

    fn control(&mut self) -> ParseResult<LabeledControl> {
        Ok(LabeledControl {
            label: self.string(true)?,
            field: self.path()?,
        })
    }

    fn action(&mut self) -> ParseResult<Action> {
        let start = self.expect(TokenKind::Arrow, "an action arrow")?.span.start;
        let name = self.identifier()?;
        let mut arguments = Vec::new();
        let mut seen = HashSet::new();
        if self.take(TokenKind::LBrace).is_some() {
            loop {
                let name = self.identifier()?;
                if !seen.insert(name.value.clone()) {
                    return Err(self.error(
                        DiagnosticCode::DuplicateArgument,
                        name.span,
                        "action argument names must be unique",
                    ));
                }
                let shorthand = self.take(TokenKind::Colon).is_none();
                let reference = if shorthand {
                    Reference {
                        path: Path {
                            segments: vec![name.value.clone()],
                            span: name.span,
                        },
                        relative: false,
                        span: name.span,
                    }
                } else {
                    self.reference()?
                };
                let span = Span::new(name.span.start, reference.span.end);
                arguments.push(ActionArgument {
                    name,
                    reference,
                    shorthand,
                    span,
                });
                if self.take(TokenKind::Comma).is_none() {
                    break;
                }
            }
            self.expect(TokenKind::RBrace, "the closing action brace")?;
        }
        Ok(Action {
            name,
            arguments,
            span: Span::new(start, self.tokens[self.cursor - 1].span.end),
        })
    }

    fn fill(&mut self, parent: Option<&str>) -> ParseResult<Option<Span>> {
        let fill = self.word("fill");
        if let Some(span) = fill {
            if !matches!(parent, Some("row" | "col")) {
                return Err(self.error(
                    DiagnosticCode::InvalidNesting,
                    span,
                    "fill requires a direct row or col parent",
                ));
            }
        }
        Ok(fill)
    }

    fn declarations(
        &mut self,
        parent: &str,
        in_form: bool,
        depth: usize,
        terminator: TokenKind,
    ) -> ParseResult<Vec<Declaration>> {
        let mut children = Vec::new();
        let mut indices = HashMap::new();
        while !self.at(terminator) {
            if self.at(TokenKind::Eof) {
                return Err(self.expected("a closing block brace"));
            }
            let index = indices.entry(self.text().to_owned()).or_insert(0);
            children.push(self.declaration(Some(parent), in_form, depth, *index)?);
            *index += 1;
        }
        self.validate_children(parent, &children)?;
        Ok(children)
    }

    fn block(&mut self, name: &str, in_form: bool, depth: usize) -> ParseResult<Vec<Declaration>> {
        self.expect(TokenKind::LBrace, "a declaration block")?;
        let children = self.declarations(
            name,
            in_form || name == "form",
            depth + 1,
            TokenKind::RBrace,
        )?;
        self.expect(TokenKind::RBrace, "a closing block brace")?;
        Ok(children)
    }

    fn declaration(
        &mut self,
        parent: Option<&str>,
        in_form: bool,
        depth: usize,
        index: usize,
    ) -> ParseResult<Declaration> {
        self.path.push(format!("{}[{index}]", self.text()));
        if depth > 12 {
            return Err(self.error(
                DiagnosticCode::DepthLimit,
                self.token().span,
                "declaration depth exceeds 12",
            ));
        }
        if self.nodes >= 2000 {
            return Err(self.error(
                DiagnosticCode::NodeLimit,
                self.token().span,
                "declaration count exceeds 2000",
            ));
        }
        self.nodes += 1;
        let name = self.identifier()?;
        let word = name.value.as_str();
        self.validate_placement(word, parent, in_form, name.span)?;
        let mut fill = None;
        let mut fill_consumed = false;
        use DeclarationKind::*;
        let kind = match word {
            "screen" | "section" | "form" => {
                let title = self.optional_title()?;
                fill = self.fill(parent)?;
                fill_consumed = true;
                let children = self.block(word, in_form, depth)?;
                let container = Container { title, children };
                match word {
                    "screen" => Screen(container),
                    "section" => Section(container),
                    _ => Form(container),
                }
            }
            "card" => {
                let title = self.optional_title()?;
                let collapsible = self.word("collapsible");
                fill = self.fill(parent)?;
                fill_consumed = true;
                Card {
                    title,
                    collapsible,
                    children: self.block(word, in_form, depth)?,
                }
            }
            "col" | "row" => {
                let wrapping = if word == "row" {
                    self.optional_choice(&[("wrap", Wrapping::Wrap), ("nowrap", Wrapping::NoWrap)])
                } else {
                    None
                };
                let alignment = self.optional_choice(&[
                    ("start", Alignment::Start),
                    ("center", Alignment::Center),
                    ("end", Alignment::End),
                ]);
                let distribution = self.optional_choice(&[
                    ("between", Distribution::Between),
                    ("around", Distribution::Around),
                ]);
                fill = self.fill(parent)?;
                fill_consumed = true;
                let children = self.block(word, in_form, depth)?;
                if word == "col" {
                    Col {
                        alignment,
                        distribution,
                        children,
                    }
                } else {
                    Row {
                        wrapping,
                        alignment,
                        distribution,
                        children,
                    }
                }
            }
            "tabs" | "header" | "footer" | "item" | "tile" => {
                fill = self.fill(parent)?;
                fill_consumed = true;
                let children = self.block(word, in_form, depth)?;
                match word {
                    "tabs" => Tabs(children),
                    "header" => Header(children),
                    "footer" => Footer(children),
                    "item" => Item(children),
                    _ => Tile(children),
                }
            }
            "tab" | "dialog" | "sheet" | "tooltip" => {
                let title = self.string(word != "tooltip")?;
                fill = self.fill(parent)?;
                fill_consumed = true;
                let children = self.block(word, in_form, depth)?;
                match word {
                    "tab" => Tab { title, children },
                    "dialog" => Dialog { title, children },
                    "sheet" => Sheet { title, children },
                    _ => Tooltip {
                        text: title,
                        children,
                    },
                }
            }
            "title" => Title(self.display()?),
            "text" => Text(self.display()?),
            "caption" => Caption(self.display()?),
            "code" => Code(self.display()?),
            "badge" => Badge(self.display()?),
            "tag" => Tag(self.display()?),
            "status" => Status {
                text: self.display()?,
                live: self.word("live"),
            },
            "label" => Label(self.string(true)?),
            "md" => Md(self.reference()?),
            "link" => {
                let text = self.display()?;
                self.expect_word("to")?;
                Link {
                    text,
                    to: self.reference()?,
                }
            }
            "stat" => Stat {
                label: self.string(true)?,
                value: self.reference()?,
                delta: self.optional_reference("delta")?,
            },
            "kv" => Kv {
                label: self.string(true)?,
                value: self.reference()?,
            },
            "icon" => Icon(self.icon()?),
            "image" | "avatar" => {
                let source = self.reference()?;
                self.expect_word("alt")?;
                let alt = self.display()?;
                if word == "image" {
                    Image { source, alt }
                } else {
                    Avatar { source, alt }
                }
            }
            "timestamp" => Timestamp {
                value: self.reference()?,
                relative: self.word("relative"),
            },
            "divider" => Divider,
            "typing" => Typing,
            "field" => {
                let control = self.control()?;
                let kind = if self.word("kind").is_some() {
                    Some(self.choice(
                        &[
                            ("text", FieldKind::Text),
                            ("email", FieldKind::Email),
                            ("url", FieldKind::Url),
                            ("number", FieldKind::Number),
                            ("phone", FieldKind::Phone),
                            ("password", FieldKind::Password),
                        ],
                        "a field kind",
                    )?)
                } else {
                    None
                };
                let min = self.optional_number("min")?;
                let max = self.optional_number("max")?;
                self.numeric_bounds(min.as_ref(), max.as_ref(), None)?;
                if !kind
                    .as_ref()
                    .is_some_and(|kind| kind.value == FieldKind::Number)
                {
                    for bound in [min.as_ref(), max.as_ref()].into_iter().flatten() {
                        if bound.value < 0.0 || bound.value.fract() != 0.0 {
                            return Err(self.error(
                                DiagnosticCode::InvalidLiteral,
                                bound.span,
                                "text bounds must be nonnegative integers",
                            ));
                        }
                    }
                }
                let pattern = if self.word("pattern").is_some() {
                    Some(self.string(false)?)
                } else {
                    None
                };
                Field {
                    control,
                    kind,
                    min,
                    max,
                    pattern,
                }
            }
            "area" => Area(self.control()?),
            "toggle" => Toggle(self.control()?),
            "check" => Check(self.control()?),
            "time" => Time(self.control()?),
            "search" => Search(self.control()?),
            "choice" | "select" | "multiselect" | "segmented" => {
                let control = self.control()?;
                self.expect_word("options")?;
                if word == "segmented" {
                    Segmented {
                        control,
                        options: self.literal_options()?,
                    }
                } else {
                    let control = OptionControl {
                        control,
                        options: self.options()?,
                    };
                    match word {
                        "choice" => Choice(control),
                        "select" => Select(control),
                        _ => Multiselect(control),
                    }
                }
            }
            "slider" | "stepper" => {
                let control = self.control()?;
                let (min, max) = if word == "slider" {
                    self.expect_word("min")?;
                    let min = self.number()?;
                    self.expect_word("max")?;
                    (Some(min), Some(self.number()?))
                } else {
                    (self.optional_number("min")?, self.optional_number("max")?)
                };
                let step = self.optional_number("step")?;
                self.numeric_bounds(min.as_ref(), max.as_ref(), step.as_ref())?;
                let control = NumericControl {
                    control,
                    min,
                    max,
                    step,
                };
                if word == "slider" {
                    Slider(control)
                } else {
                    Stepper(control)
                }
            }
            "date" => Date {
                control: self.control()?,
                min: self.optional_reference("min")?,
                max: self.optional_reference("max")?,
            },
            "button" => {
                let label = self.string(true)?;
                let intent = self.optional_choice(&[
                    ("primary", ButtonIntent::Primary),
                    ("danger", ButtonIntent::Danger),
                ]);
                let confirm = self.word("confirm");
                let disabled = self.word("disabled");
                fill = self.fill(parent)?;
                fill_consumed = true;
                Button {
                    label,
                    intent,
                    confirm,
                    disabled,
                    action: self.action()?,
                }
            }
            "list" | "table" | "grid" | "chat" => {
                self.expect_word("from")?;
                let source = self.reference()?;
                let key = if self.word("key").is_some() {
                    Some(self.path()?)
                } else {
                    None
                };
                let selection = if word != "chat" && self.word("select").is_some() {
                    Some(self.choice(
                        &[("one", Selection::One), ("many", Selection::Many)],
                        "one or many",
                    )?)
                } else {
                    None
                };
                fill = self.fill(parent)?;
                fill_consumed = true;
                let children = self.block(word, in_form, depth)?;
                if word == "chat" {
                    Chat {
                        source,
                        key,
                        children,
                    }
                } else {
                    let collection = Collection {
                        source,
                        key,
                        selection,
                        children,
                    };
                    match word {
                        "list" => List(collection),
                        "table" => Table(collection),
                        _ => Grid(collection),
                    }
                }
            }
            "column" => {
                let label = self.string(true)?;
                if !self.at(TokenKind::Dot) {
                    return Err(self.expected("a relative column path"));
                }
                let value = self.reference()?;
                let kind = if self.word("kind").is_some() {
                    Some(self.choice(
                        &[
                            ("text", ColumnKind::Text),
                            ("number", ColumnKind::Number),
                            ("money", ColumnKind::Money),
                            ("date", ColumnKind::Date),
                            ("status", ColumnKind::Status),
                        ],
                        "a column kind",
                    )?)
                } else {
                    None
                };
                Column { label, value, kind }
            }
            "progress" => {
                let value = self.optional_reference("value")?;
                let label = if self.word("label").is_some() {
                    Some(self.string(false)?)
                } else {
                    None
                };
                Progress { value, label }
            }
            "steps" => {
                self.expect_word("stages")?;
                let stages = self.literal_options()?;
                self.expect_word("current")?;
                Steps {
                    stages,
                    current: self.reference()?,
                }
            }
            "banner" => {
                let text = self.display()?;
                let tone = {
                    self.expect_word("tone")?;
                    self.choice(
                        &[
                            ("info", Tone::Info),
                            ("warn", Tone::Warn),
                            ("error", Tone::Error),
                        ],
                        "a banner tone",
                    )?
                };
                fill = self.fill(parent)?;
                fill_consumed = true;
                let action = if self.at(TokenKind::Arrow) {
                    Some(self.action()?)
                } else {
                    None
                };
                Banner { text, tone, action }
            }
            "empty" => Empty(self.string(false)?),
            "error" => {
                let text = self.string(false)?;
                fill = self.fill(parent)?;
                fill_consumed = true;
                let retry = if self.at(TokenKind::Arrow) {
                    let action = self.action()?;
                    if action.name.value != "Retry" || !action.arguments.is_empty() {
                        return Err(self.error(
                            DiagnosticCode::UnexpectedToken,
                            action.span,
                            "error overrides accept only -> Retry",
                        ));
                    }
                    Some(action)
                } else {
                    None
                };
                Error { text, retry }
            }
            "message" => {
                self.expect_word("role")?;
                let role = if self
                    .tokens
                    .get(self.cursor + 1)
                    .is_some_and(|token| token.kind == TokenKind::Dot)
                {
                    MessageRole::Reference(self.reference()?)
                } else if let Some(role) = self.optional_choice(&[
                    ("me", Role::Me),
                    ("them", Role::Them),
                    ("system", Role::System),
                ]) {
                    MessageRole::Literal(role)
                } else {
                    MessageRole::Reference(self.reference()?)
                };
                self.expect_word("body")?;
                let body = self.reference()?;
                fill = self.fill(parent)?;
                fill_consumed = true;
                let children = if self.at(TokenKind::LBrace) {
                    Some(self.block(word, in_form, depth)?)
                } else {
                    None
                };
                Message {
                    role,
                    body,
                    children,
                }
            }
            "compose" => {
                let field = self.path()?;
                self.expect_word("send")?;
                fill = self.fill(parent)?;
                fill_consumed = true;
                Compose {
                    field,
                    action: self.action()?,
                }
            }
            "menu" => {
                self.expect_word("trigger")?;
                let trigger = if self.word("icon").is_some() {
                    MenuTrigger::Icon(self.icon()?)
                } else {
                    MenuTrigger::Label(self.string(true)?)
                };
                fill = self.fill(parent)?;
                fill_consumed = true;
                Menu {
                    trigger,
                    children: self.block(word, in_form, depth)?,
                }
            }
            _ => {
                return Err(self.error(
                    DiagnosticCode::UnknownDeclaration,
                    name.span,
                    "unknown declaration or misplaced modifier",
                ))
            }
        };
        if !fill_consumed {
            fill = self.fill(parent)?;
        }
        self.path.pop();
        Ok(Declaration {
            kind,
            fill,
            span: Span::new(name.span.start, self.tokens[self.cursor - 1].span.end),
        })
    }

    fn numeric_bounds(
        &self,
        min: Option<&NumberLiteral>,
        max: Option<&NumberLiteral>,
        step: Option<&NumberLiteral>,
    ) -> ParseResult<()> {
        if let (Some(min), Some(max)) = (min, max) {
            if min.value > max.value {
                return Err(self.error(
                    DiagnosticCode::InvalidLiteral,
                    max.span,
                    "max cannot be less than min",
                ));
            }
        }
        if let Some(step) = step {
            if step.value <= 0.0 {
                return Err(self.error(
                    DiagnosticCode::InvalidLiteral,
                    step.span,
                    "step must be positive",
                ));
            }
        }
        Ok(())
    }

    fn validate_placement(
        &self,
        name: &str,
        parent: Option<&str>,
        in_form: bool,
        span: Span,
    ) -> ParseResult<()> {
        let allowed = match name {
            "screen" => parent.is_none(),
            "header" | "footer" => parent == Some("screen"),
            "tab" => parent == Some("tabs"),
            "item" => parent == Some("list"),
            "tile" => parent == Some("grid"),
            "column" => parent == Some("table"),
            "message" => parent == Some("chat"),
            "empty" | "error" => matches!(parent, Some("list" | "grid" | "table" | "chat")),
            "form" => !in_form,
            _ => true,
        };
        if !allowed {
            return Err(self.error(
                DiagnosticCode::InvalidNesting,
                span,
                format!("{name} is not allowed in this block"),
            ));
        }
        Ok(())
    }

    fn validate_children(&self, parent: &str, children: &[Declaration]) -> ParseResult<()> {
        use DeclarationKind::*;
        let valid = match parent {
            "screen" => {
                children
                    .iter()
                    .filter(|c| matches!(c.kind, Header(_)))
                    .count()
                    <= 1
                    && children
                        .iter()
                        .filter(|c| matches!(c.kind, Footer(_)))
                        .count()
                        <= 1
            }
            "tabs" => !children.is_empty() && children.iter().all(|c| matches!(c.kind, Tab { .. })),
            "menu" => {
                !children.is_empty() && children.iter().all(|c| matches!(c.kind, Button { .. }))
            }
            "tooltip" => children.len() == 1,
            "form" => primary_buttons(children) == 1,
            "list" | "grid" | "table" | "chat" => {
                let templates = children
                    .iter()
                    .take_while(|child| {
                        matches!(
                            (parent, &child.kind),
                            ("list", Item(_))
                                | ("grid", Tile(_))
                                | ("table", Column { .. })
                                | ("chat", Message { .. })
                        )
                    })
                    .count();
                let count_valid = if parent == "table" {
                    templates >= 1
                } else {
                    templates == 1
                };
                let mut rest = &children[templates..];
                if rest
                    .first()
                    .is_some_and(|child| matches!(child.kind, Empty(_)))
                {
                    rest = &rest[1..];
                }
                if rest
                    .first()
                    .is_some_and(|child| matches!(child.kind, Error { .. }))
                {
                    rest = &rest[1..];
                }
                count_valid && rest.is_empty()
            }
            _ => true,
        };
        if !valid {
            return Err(self.error(
                DiagnosticCode::InvalidNesting,
                self.token().span,
                format!("invalid children for {parent}"),
            ));
        }
        Ok(())
    }
}

fn primary_buttons(children: &[Declaration]) -> usize {
    children.iter().map(|child| {
        usize::from(matches!(&child.kind, DeclarationKind::Button { intent: Some(intent), .. } if intent.value == ButtonIntent::Primary))
            + primary_buttons(child.children())
    }).sum()
}
