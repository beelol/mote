use crate::ast::{Alignment, Wrapping};
use crate::resolve::{ActionTarget, Element, Property, ResolvedDeclaration};
use crate::{Diagnostic, DiagnosticCode, ResolvedSurface, Severity};
use mote_ui::{Node, MAX_BYTES, MAX_DEPTH, MAX_NODES};
use serde_json::Value;
use std::io::{self, Write};

type Checked<T> = Result<T, Diagnostic>;

fn diagnostic(node: &ResolvedDeclaration, code: DiagnosticCode, message: &str) -> Diagnostic {
    Diagnostic {
        code,
        severity: Severity::Error,
        span: node.span,
        declaration_path: std::iter::once("screen".into())
            .chain(node.identity.path.iter().map(usize::to_string))
            .collect(),
        message: message.into(),
        hint: Some("Use the supported v0 subset or a versioned consumer contract".into()),
    }
}

fn unsupported(node: &ResolvedDeclaration) -> Diagnostic {
    diagnostic(
        node,
        DiagnosticCode::LowerUnsupported,
        "The v0 consumer cannot represent this declaration losslessly",
    )
}

fn check_support(node: &ResolvedDeclaration, repeated: bool) -> Checked<()> {
    use Element::*;
    let properties_supported = node.properties.iter().all(|property| match node.element {
        Col => matches!(property, Property::Alignment(Alignment::Start)),
        Row => matches!(
            property,
            Property::Alignment(Alignment::Start) | Property::Wrapping(Wrapping::Wrap)
        ),
        Field | Toggle | Select | Button => matches!(property, Property::Label(_)),
        Column => matches!(property, Property::Label(_) | Property::ColumnKind(_)),
        _ => false,
    });
    let element_supported = match node.element {
        Screen | Text | Col | Row | Item | Column => true,
        List | Table => node.children.is_empty(),
        Field | Toggle | Select => {
            !repeated
                && node
                    .field
                    .as_ref()
                    .is_some_and(|field| field.rows.is_empty())
        }
        Button => node.action.as_ref().is_some_and(|action| {
            action.target == ActionTarget::Host && action.arguments.is_empty() && !action.disabled
        }),
        Progress => node.values.contains_key("value"),
        _ => false,
    };
    if node.fill || !properties_supported || !element_supported {
        return Err(unsupported(node));
    }
    for child in &node.children {
        check_support(child, repeated)?;
    }
    for child in node
        .rows
        .iter()
        .flat_map(|row| &row.children)
        .chain(&node.empty_templates)
    {
        check_support(child, true)?;
    }
    Ok(())
}

fn label(node: &ResolvedDeclaration) -> Checked<&str> {
    node.properties
        .iter()
        .find_map(|property| match property {
            Property::Label(label) => Some(label.as_str()),
            _ => None,
        })
        .ok_or_else(|| unsupported(node))
}

fn raw_value<'a>(node: &'a ResolvedDeclaration, key: &str) -> Checked<&'a Value> {
    node.values
        .get(key)
        .and_then(|value| value.value.as_ref())
        .ok_or_else(|| unsupported(node))
}

fn display<'a>(node: &'a ResolvedDeclaration, key: &str) -> Checked<&'a str> {
    node.values
        .get(key)
        .and_then(|value| value.display.as_deref())
        .ok_or_else(|| unsupported(node))
}

#[derive(Default)]
struct Budget {
    bytes: usize,
    nodes: usize,
}

impl Write for Budget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.bytes) {
            return Err(io::Error::other("Compact v0 tree exceeds 256 KiB"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Budget {
    fn charge(&mut self, bytes: usize, node: &ResolvedDeclaration) -> Checked<()> {
        if bytes > MAX_BYTES.saturating_sub(self.bytes) {
            return Err(self.limit(node));
        }
        self.bytes += bytes;
        Ok(())
    }

    fn string(&mut self, text: &str, node: &ResolvedDeclaration) -> Checked<String> {
        serde_json::to_writer(&mut *self, text).map_err(|_| self.limit(node))?;
        Ok(text.into())
    }

    fn limit(&self, node: &ResolvedDeclaration) -> Diagnostic {
        let mut error = diagnostic(
            node,
            DiagnosticCode::LowerLimit,
            "The compact v0 tree exceeds depth 12, 2,000 nodes or 256 KiB",
        );
        error.hint = Some("Reduce the surface or the collection snapshot".into());
        error
    }

    fn children(
        &mut self,
        children: &[ResolvedDeclaration],
        parent: &ResolvedDeclaration,
        depth: usize,
    ) -> Checked<Vec<Node>> {
        self.charge(children.len().saturating_sub(1), parent)?;
        children
            .iter()
            .map(|child| self.lower(child, depth))
            .collect()
    }

    fn lower(&mut self, node: &ResolvedDeclaration, depth: usize) -> Checked<Node> {
        use Element::*;
        if matches!(node.element, Screen | Item) && node.children.len() == 1 {
            return self.lower(&node.children[0], depth);
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES || depth > MAX_DEPTH {
            return Err(self.limit(node));
        }
        Ok(match node.element {
            Text => {
                self.charge(r#"{"type":"text","value":}"#.len(), node)?;
                Node::Text {
                    value: self.string(display(node, "text")?, node)?,
                }
            }
            Screen | Col | Item | Row => {
                let row = node.element == Row;
                self.charge(
                    if row {
                        r#"{"type":"row","children":[]}"#.len()
                    } else {
                        r#"{"type":"stack","children":[]}"#.len()
                    },
                    node,
                )?;
                let children = self.children(&node.children, node, depth + 1)?;
                if row {
                    Node::Row { children }
                } else {
                    Node::Stack { children }
                }
            }
            List => {
                self.charge(
                    r#"{"type":"list","children":[]}"#.len() + node.rows.len().saturating_sub(1),
                    node,
                )?;
                let children = node
                    .rows
                    .iter()
                    .map(|row| {
                        if row.children.len() != 1 {
                            return Err(unsupported(node));
                        }
                        self.lower(&row.children[0], depth + 1)
                    })
                    .collect::<Checked<_>>()?;
                Node::List { children }
            }
            Table => {
                let columns = node
                    .rows
                    .first()
                    .map(|row| row.children.as_slice())
                    .unwrap_or(&node.empty_templates);
                self.charge(
                    r#"{"type":"table","columns":[],"rows":[]}"#.len()
                        + columns.len().saturating_sub(1)
                        + node.rows.len().saturating_sub(1),
                    node,
                )?;
                let columns = columns
                    .iter()
                    .map(|column| self.string(label(column)?, column))
                    .collect::<Checked<Vec<_>>>()?;
                let rows = node
                    .rows
                    .iter()
                    .map(|row| {
                        self.charge(2 + row.children.len().saturating_sub(1), node)?;
                        row.children
                            .iter()
                            .map(|column| self.string(display(column, "value")?, column))
                            .collect::<Checked<Vec<_>>>()
                    })
                    .collect::<Checked<Vec<_>>>()?;
                Node::Table { columns, rows }
            }
            Field | Toggle | Select => {
                let prefix = match node.element {
                    Field => r#"{"type":"field","label":,"name":,"value":}"#,
                    Toggle => r#"{"type":"toggle","label":,"name":,"value":}"#,
                    _ => r#"{"type":"select","label":,"name":,"options":[],"value":}"#,
                };
                self.charge(prefix.len(), node)?;
                let label = self.string(label(node)?, node)?;
                let field = node.field.as_ref().ok_or_else(|| unsupported(node))?;
                let name = self.string(&field.path, node)?;
                let current = raw_value(node, "field")?;
                if node.element == Toggle {
                    let value = current.as_bool().ok_or_else(|| unsupported(node))?;
                    self.charge(if value { 4 } else { 5 }, node)?;
                    Node::Toggle { label, name, value }
                } else {
                    let value =
                        self.string(current.as_str().ok_or_else(|| unsupported(node))?, node)?;
                    if node.element == Field {
                        Node::Field { label, name, value }
                    } else {
                        let raw = raw_value(node, "options")?
                            .as_array()
                            .ok_or_else(|| unsupported(node))?;
                        self.charge(raw.len().saturating_sub(1), node)?;
                        let options = raw
                            .iter()
                            .map(|option| {
                                self.string(option.as_str().ok_or_else(|| unsupported(node))?, node)
                            })
                            .collect::<Checked<_>>()?;
                        Node::Select {
                            label,
                            name,
                            options,
                            value,
                        }
                    }
                }
            }
            Button => {
                self.charge(r#"{"type":"action","label":,"event":}"#.len(), node)?;
                let label = self.string(label(node)?, node)?;
                let event = self.string(
                    &node.action.as_ref().ok_or_else(|| unsupported(node))?.name,
                    node,
                )?;
                Node::Action { label, event }
            }
            Progress => {
                self.charge(r#"{"type":"progress","value":}"#.len(), node)?;
                let value = raw_value(node, "value")?
                    .as_f64()
                    .ok_or_else(|| unsupported(node))? as f32;
                serde_json::to_writer(&mut *self, &value).map_err(|_| self.limit(node))?;
                Node::Progress { value }
            }
            _ => return Err(unsupported(node)),
        })
    }
}

pub fn lower_v0(surface: &ResolvedSurface) -> Result<Node, Vec<Diagnostic>> {
    let work = || {
        check_support(&surface.screen, false)?;
        Budget::default().lower(&surface.screen, 1)
    };
    work().map_err(|error| vec![error])
}
