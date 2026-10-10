use crate::Span;

#[derive(Clone, Debug, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

pub type StringLiteral = Spanned<String>;

#[derive(Clone, Debug, PartialEq)]
pub struct NumberLiteral {
    pub source: String,
    pub value: f64,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub segments: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    pub path: Path,
    pub relative: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DisplayOperand {
    Literal(StringLiteral),
    Reference(Reference),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Options {
    Literal(Vec<StringLiteral>),
    Reference(Reference),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionArgument {
    pub name: Spanned<String>,
    pub reference: Reference,
    pub shorthand: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub name: Spanned<String>,
    pub arguments: Vec<ActionArgument>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    Between,
    Around,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrapping {
    Wrap,
    NoWrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Email,
    Url,
    Number,
    Phone,
    Password,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnKind {
    Text,
    Number,
    Money,
    Date,
    Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonIntent {
    Primary,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    One,
    Many,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Me,
    Them,
    System,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MessageRole {
    Literal(Spanned<Role>),
    Reference(Reference),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Add,
    Back,
    Bell,
    Calendar,
    Check,
    Clock,
    Close,
    Copy,
    Delete,
    Down,
    Edit,
    Error,
    File,
    Folder,
    Image,
    Info,
    Left,
    Mail,
    More,
    Open,
    Right,
    Search,
    Settings,
    Up,
    User,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuTrigger {
    Label(StringLiteral),
    Icon(Spanned<Icon>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Surface {
    pub screen: Declaration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub fill: Option<Span>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Container {
    pub title: Option<StringLiteral>,
    pub children: Vec<Declaration>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LabeledControl {
    pub label: StringLiteral,
    pub field: Path,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OptionControl {
    pub control: LabeledControl,
    pub options: Options,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumericControl {
    pub control: LabeledControl,
    pub min: Option<NumberLiteral>,
    pub max: Option<NumberLiteral>,
    pub step: Option<NumberLiteral>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Collection {
    pub source: Reference,
    pub key: Option<Path>,
    pub selection: Option<Spanned<Selection>>,
    pub children: Vec<Declaration>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeclarationKind {
    Screen(Container),
    Section(Container),
    Card {
        title: Option<StringLiteral>,
        collapsible: Option<Span>,
        children: Vec<Declaration>,
    },
    Col {
        alignment: Option<Spanned<Alignment>>,
        distribution: Option<Spanned<Distribution>>,
        children: Vec<Declaration>,
    },
    Row {
        wrapping: Option<Spanned<Wrapping>>,
        alignment: Option<Spanned<Alignment>>,
        distribution: Option<Spanned<Distribution>>,
        children: Vec<Declaration>,
    },
    Tabs(Vec<Declaration>),
    Tab {
        title: StringLiteral,
        children: Vec<Declaration>,
    },
    Header(Vec<Declaration>),
    Footer(Vec<Declaration>),
    Title(DisplayOperand),
    Text(DisplayOperand),
    Caption(DisplayOperand),
    Code(DisplayOperand),
    Badge(DisplayOperand),
    Tag(DisplayOperand),
    Status {
        text: DisplayOperand,
        live: Option<Span>,
    },
    Label(StringLiteral),
    Md(Reference),
    Link {
        text: DisplayOperand,
        to: Reference,
    },
    Stat {
        label: StringLiteral,
        value: Reference,
        delta: Option<Reference>,
    },
    Kv {
        label: StringLiteral,
        value: Reference,
    },
    Icon(Spanned<Icon>),
    Image {
        source: Reference,
        alt: DisplayOperand,
    },
    Avatar {
        source: Reference,
        alt: DisplayOperand,
    },
    Timestamp {
        value: Reference,
        relative: Option<Span>,
    },
    Divider,
    Typing,
    Field {
        control: LabeledControl,
        kind: Option<Spanned<FieldKind>>,
        min: Option<NumberLiteral>,
        max: Option<NumberLiteral>,
        pattern: Option<StringLiteral>,
    },
    Area(LabeledControl),
    Toggle(LabeledControl),
    Check(LabeledControl),
    Time(LabeledControl),
    Search(LabeledControl),
    Choice(OptionControl),
    Select(OptionControl),
    Multiselect(OptionControl),
    Segmented {
        control: LabeledControl,
        options: Vec<StringLiteral>,
    },
    Slider(NumericControl),
    Stepper(NumericControl),
    Date {
        control: LabeledControl,
        min: Option<Reference>,
        max: Option<Reference>,
    },
    Button {
        label: StringLiteral,
        intent: Option<Spanned<ButtonIntent>>,
        confirm: Option<Span>,
        disabled: Option<Span>,
        action: Action,
    },
    Form(Container),
    List(Collection),
    Table(Collection),
    Grid(Collection),
    Item(Vec<Declaration>),
    Tile(Vec<Declaration>),
    Column {
        label: StringLiteral,
        value: Reference,
        kind: Option<Spanned<ColumnKind>>,
    },
    Progress {
        value: Option<Reference>,
        label: Option<StringLiteral>,
    },
    Steps {
        stages: Vec<StringLiteral>,
        current: Reference,
    },
    Banner {
        text: DisplayOperand,
        tone: Spanned<Tone>,
        action: Option<Action>,
    },
    Empty(StringLiteral),
    Error {
        text: StringLiteral,
        retry: Option<Action>,
    },
    Chat {
        source: Reference,
        key: Option<Path>,
        children: Vec<Declaration>,
    },
    Message {
        role: MessageRole,
        body: Reference,
        children: Option<Vec<Declaration>>,
    },
    Compose {
        field: Path,
        action: Action,
    },
    Dialog {
        title: StringLiteral,
        children: Vec<Declaration>,
    },
    Sheet {
        title: StringLiteral,
        children: Vec<Declaration>,
    },
    Menu {
        trigger: MenuTrigger,
        children: Vec<Declaration>,
    },
    Tooltip {
        text: StringLiteral,
        children: Vec<Declaration>,
    },
}

impl Declaration {
    pub fn children(&self) -> &[Declaration] {
        use DeclarationKind::*;
        match &self.kind {
            Screen(c) | Section(c) | Form(c) => &c.children,
            List(c) | Table(c) | Grid(c) => &c.children,
            Card { children, .. }
            | Col { children, .. }
            | Row { children, .. }
            | Tab { children, .. }
            | Chat { children, .. }
            | Dialog { children, .. }
            | Sheet { children, .. }
            | Menu { children, .. }
            | Tooltip { children, .. }
            | Tabs(children)
            | Header(children)
            | Footer(children)
            | Item(children)
            | Tile(children) => children,
            Message {
                children: Some(children),
                ..
            } => children,
            _ => &[],
        }
    }
}
