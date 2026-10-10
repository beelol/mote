use crate::ast::*;

const MAX_SOURCE_BYTES: usize = 256 * 1024;

pub fn format_surface(surface: &Surface) -> String {
    let mut output = String::new();
    write_declaration(&surface.screen, 0, &mut output);
    finish_format(output)
}

pub fn format_fragment(declarations: &[Declaration]) -> String {
    let mut output = String::new();
    for declaration in declarations {
        write_declaration(declaration, 0, &mut output);
    }
    finish_format(output)
}

fn finish_format(pretty: String) -> String {
    if pretty.len() <= MAX_SOURCE_BYTES {
        pretty
    } else {
        compact(&pretty)
    }
}

fn compact(pretty: &str) -> String {
    let mut output = String::with_capacity(pretty.len());
    let mut in_string = false;
    let mut escaped = false;
    let mut pending_space = false;
    let mut previous = None;
    for character in pretty.chars() {
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character.is_ascii_whitespace() {
            pending_space = true;
            continue;
        } else {
            if pending_space && previous.is_some_and(word_character) && word_character(character) {
                output.push(' ');
            }
            output.push(character);
            pending_space = false;
            if character == '"' {
                in_string = true;
            }
        }
        previous = Some(character);
    }
    output
}

fn word_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn string(value: &StringLiteral) -> String {
    serde_json::to_string(&value.value).expect("a parsed string is JSON serializable")
}

fn path(value: &Path) -> String {
    value.segments.join(".")
}

fn reference(value: &Reference) -> String {
    format!(
        "{}{}",
        if value.relative { "." } else { "" },
        path(&value.path)
    )
}

fn display(value: &DisplayOperand) -> String {
    match value {
        DisplayOperand::Literal(value) => string(value),
        DisplayOperand::Reference(value) => reference(value),
    }
}

fn options(value: &Options) -> String {
    match value {
        Options::Literal(values) => literal_options(values),
        Options::Reference(value) => reference(value),
    }
}

fn literal_options(values: &[StringLiteral]) -> String {
    format!(
        "[{}]",
        values.iter().map(string).collect::<Vec<_>>().join(", ")
    )
}

fn word<T: std::fmt::Debug>(value: T) -> String {
    format!("{value:?}").to_ascii_lowercase()
}

fn action(value: &Action) -> String {
    let mut output = format!(" -> {}", value.name.value);
    if !value.arguments.is_empty() {
        output.push('{');
        for (index, argument) in value.arguments.iter().enumerate() {
            if index != 0 {
                output.push_str(", ");
            }
            output.push_str(&argument.name.value);
            if !argument.shorthand {
                output.push_str(": ");
                output.push_str(&reference(&argument.reference));
            }
        }
        output.push('}');
    }
    output
}

fn control(value: &LabeledControl) -> String {
    format!("{} {}", string(&value.label), path(&value.field))
}

fn numeric(value: &NumericControl, required_bounds: bool) -> String {
    let mut output = control(&value.control);
    if let Some(min) = &value.min {
        output.push_str(&format!(" min {}", min.source));
    } else if required_bounds {
        unreachable!("parsed sliders have a minimum")
    }
    if let Some(max) = &value.max {
        output.push_str(&format!(" max {}", max.source));
    } else if required_bounds {
        unreachable!("parsed sliders have a maximum")
    }
    if let Some(step) = &value.step {
        output.push_str(&format!(" step {}", step.source));
    }
    output
}

fn collection(name: &str, value: &Collection) -> String {
    let mut output = format!("{name} from {}", reference(&value.source));
    if let Some(key) = &value.key {
        output.push_str(&format!(" key {}", path(key)));
    }
    if let Some(selection) = &value.selection {
        output.push_str(&format!(" select {}", word(selection.value)));
    }
    output
}

fn write_declaration(declaration: &Declaration, depth: usize, output: &mut String) {
    use DeclarationKind::*;
    let (mut head, children, action): (String, Option<&[Declaration]>, Option<&Action>) =
        match &declaration.kind {
            Screen(value) => (container("screen", value), Some(&value.children), None),
            Section(value) => (container("section", value), Some(&value.children), None),
            Form(value) => (container("form", value), Some(&value.children), None),
            Card {
                title,
                collapsible,
                children,
            } => {
                let mut head = "card".to_owned();
                if let Some(title) = title {
                    head.push_str(&format!(" {}", string(title)));
                }
                if collapsible.is_some() {
                    head.push_str(" collapsible");
                }
                (head, Some(children), None)
            }
            Col {
                alignment,
                distribution,
                children,
            } => {
                let mut head = "col".to_owned();
                if let Some(value) = alignment {
                    head.push_str(&format!(" {}", word(value.value)));
                }
                if let Some(value) = distribution {
                    head.push_str(&format!(" {}", word(value.value)));
                }
                (head, Some(children), None)
            }
            Row {
                wrapping,
                alignment,
                distribution,
                children,
            } => {
                let mut head = "row".to_owned();
                if let Some(value) = wrapping {
                    head.push_str(&format!(" {}", word(value.value)));
                }
                if let Some(value) = alignment {
                    head.push_str(&format!(" {}", word(value.value)));
                }
                if let Some(value) = distribution {
                    head.push_str(&format!(" {}", word(value.value)));
                }
                (head, Some(children), None)
            }
            Tabs(children) => ("tabs".to_owned(), Some(children), None),
            Header(children) => ("header".to_owned(), Some(children), None),
            Footer(children) => ("footer".to_owned(), Some(children), None),
            Item(children) => ("item".to_owned(), Some(children), None),
            Tile(children) => ("tile".to_owned(), Some(children), None),
            Tab { title, children } => (format!("tab {}", string(title)), Some(children), None),
            Dialog { title, children } => {
                (format!("dialog {}", string(title)), Some(children), None)
            }
            Sheet { title, children } => (format!("sheet {}", string(title)), Some(children), None),
            Tooltip { text, children } => {
                (format!("tooltip {}", string(text)), Some(children), None)
            }
            Title(value) => (format!("title {}", display(value)), None, None),
            Text(value) => (format!("text {}", display(value)), None, None),
            Caption(value) => (format!("caption {}", display(value)), None, None),
            Code(value) => (format!("code {}", display(value)), None, None),
            Badge(value) => (format!("badge {}", display(value)), None, None),
            Tag(value) => (format!("tag {}", display(value)), None, None),
            Status { text, live } => (
                format!(
                    "status {}{}",
                    display(text),
                    if live.is_some() { " live" } else { "" }
                ),
                None,
                None,
            ),
            Label(value) => (format!("label {}", string(value)), None, None),
            Md(value) => (format!("md {}", reference(value)), None, None),
            Link { text, to } => (
                format!("link {} to {}", display(text), reference(to)),
                None,
                None,
            ),
            Stat {
                label,
                value,
                delta,
            } => {
                let mut head = format!("stat {} {}", string(label), reference(value));
                if let Some(delta) = delta {
                    head.push_str(&format!(" delta {}", reference(delta)));
                }
                (head, None, None)
            }
            Kv { label, value } => (
                format!("kv {} {}", string(label), reference(value)),
                None,
                None,
            ),
            Icon(value) => (format!("icon \"{}\"", word(value.value)), None, None),
            Image { source, alt } => (
                format!("image {} alt {}", reference(source), display(alt)),
                None,
                None,
            ),
            Avatar { source, alt } => (
                format!("avatar {} alt {}", reference(source), display(alt)),
                None,
                None,
            ),
            Timestamp { value, relative } => (
                format!(
                    "timestamp {}{}",
                    reference(value),
                    if relative.is_some() { " relative" } else { "" }
                ),
                None,
                None,
            ),
            Divider => ("divider".to_owned(), None, None),
            Typing => ("typing".to_owned(), None, None),
            Field {
                control: value,
                kind,
                min,
                max,
                pattern,
            } => {
                let mut head = format!("field {}", control(value));
                if let Some(kind) = kind {
                    head.push_str(&format!(" kind {}", word(kind.value)));
                }
                if let Some(min) = min {
                    head.push_str(&format!(" min {}", min.source));
                }
                if let Some(max) = max {
                    head.push_str(&format!(" max {}", max.source));
                }
                if let Some(pattern) = pattern {
                    head.push_str(&format!(" pattern {}", string(pattern)));
                }
                (head, None, None)
            }
            Area(value) => (format!("area {}", control(value)), None, None),
            Toggle(value) => (format!("toggle {}", control(value)), None, None),
            Check(value) => (format!("check {}", control(value)), None, None),
            Time(value) => (format!("time {}", control(value)), None, None),
            Search(value) => (format!("search {}", control(value)), None, None),
            Choice(value) => (
                format!(
                    "choice {} options {}",
                    control(&value.control),
                    options(&value.options)
                ),
                None,
                None,
            ),
            Select(value) => (
                format!(
                    "select {} options {}",
                    control(&value.control),
                    options(&value.options)
                ),
                None,
                None,
            ),
            Multiselect(value) => (
                format!(
                    "multiselect {} options {}",
                    control(&value.control),
                    options(&value.options)
                ),
                None,
                None,
            ),
            Segmented {
                control: value,
                options,
            } => (
                format!(
                    "segmented {} options {}",
                    control(value),
                    literal_options(options)
                ),
                None,
                None,
            ),
            Slider(value) => (format!("slider {}", numeric(value, true)), None, None),
            Stepper(value) => (format!("stepper {}", numeric(value, false)), None, None),
            Date {
                control: value,
                min,
                max,
            } => {
                let mut head = format!("date {}", control(value));
                if let Some(min) = min {
                    head.push_str(&format!(" min {}", reference(min)));
                }
                if let Some(max) = max {
                    head.push_str(&format!(" max {}", reference(max)));
                }
                (head, None, None)
            }
            Button {
                label,
                intent,
                confirm,
                disabled,
                action,
            } => {
                let mut head = format!("button {}", string(label));
                if let Some(intent) = intent {
                    head.push_str(&format!(" {}", word(intent.value)));
                }
                if confirm.is_some() {
                    head.push_str(" confirm");
                }
                if disabled.is_some() {
                    head.push_str(" disabled");
                }
                (head, None, Some(action))
            }
            List(value) => (collection("list", value), Some(&value.children), None),
            Table(value) => (collection("table", value), Some(&value.children), None),
            Grid(value) => (collection("grid", value), Some(&value.children), None),
            Column { label, value, kind } => {
                let mut head = format!("column {} {}", string(label), reference(value));
                if let Some(kind) = kind {
                    head.push_str(&format!(" kind {}", word(kind.value)));
                }
                (head, None, None)
            }
            Progress { value, label } => {
                let mut head = "progress".to_owned();
                if let Some(value) = value {
                    head.push_str(&format!(" value {}", reference(value)));
                }
                if let Some(label) = label {
                    head.push_str(&format!(" label {}", string(label)));
                }
                (head, None, None)
            }
            Steps { stages, current } => (
                format!(
                    "steps stages {} current {}",
                    literal_options(stages),
                    reference(current)
                ),
                None,
                None,
            ),
            Banner { text, tone, action } => (
                format!("banner {} tone {}", display(text), word(tone.value)),
                None,
                action.as_ref(),
            ),
            Empty(value) => (format!("empty {}", string(value)), None, None),
            Error { text, retry } => (format!("error {}", string(text)), None, retry.as_ref()),
            Chat {
                source,
                key,
                children,
            } => {
                let mut head = format!("chat from {}", reference(source));
                if let Some(key) = key {
                    head.push_str(&format!(" key {}", path(key)));
                }
                (head, Some(children), None)
            }
            Message {
                role,
                body,
                children,
            } => {
                let role = match role {
                    MessageRole::Literal(value) => word(value.value),
                    MessageRole::Reference(value) => reference(value),
                };
                (
                    format!("message role {role} body {}", reference(body)),
                    children.as_deref(),
                    None,
                )
            }
            Compose { field, action } => {
                (format!("compose {} send", path(field)), None, Some(action))
            }
            Menu { trigger, children } => {
                let trigger = match trigger {
                    MenuTrigger::Label(value) => string(value),
                    MenuTrigger::Icon(value) => format!("icon \"{}\"", word(value.value)),
                };
                (format!("menu trigger {trigger}"), Some(children), None)
            }
        };
    if declaration.fill.is_some() {
        head.push_str(" fill");
    }
    if let Some(action) = action {
        head.push_str(&self::action(action));
    }
    output.push_str(&"  ".repeat(depth));
    output.push_str(&head);
    if let Some(children) = children {
        if children.is_empty() {
            output.push_str(" {}\n");
        } else {
            output.push_str(" {\n");
            for child in children {
                write_declaration(child, depth + 1, output);
            }
            output.push_str(&"  ".repeat(depth));
            output.push_str("}\n");
        }
    } else {
        output.push('\n');
    }
}

fn container(name: &str, value: &Container) -> String {
    match &value.title {
        Some(title) => format!("{name} {}", string(title)),
        None => name.to_owned(),
    }
}
