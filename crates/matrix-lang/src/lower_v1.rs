use crate::ast::{
    Alignment as SourceAlignment, ButtonIntent as SourceButtonIntent,
    Distribution as SourceDistribution, Wrapping as SourceWrapping,
};
use crate::decimal::Decimal;
use crate::resolve::{ActionTarget, Element, Property, ResolvedDeclaration};
use crate::{Diagnostic, DiagnosticCode, ResolvedSurface, Severity};
use mote_ui::v1::{
    Alignment, ButtonIntent, Distribution, Node, Surface, Wrapping, CONTRACT_VERSION,
};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::BTreeMap;

type Checked<T> = Result<T, Diagnostic>;

#[derive(Clone, Debug, PartialEq)]
pub enum ControlContract {
    Toggle {
        field: String,
    },
    Select {
        field: String,
        options: Vec<String>,
    },
    Slider {
        field: String,
        min: f64,
        max: f64,
        step: Option<f64>,
    },
    Button {
        event: String,
        disabled: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoweredSurface {
    pub surface: Surface,
    pub controls: BTreeMap<Vec<usize>, ControlContract>,
}

fn diagnostic(node: &ResolvedDeclaration, code: DiagnosticCode, message: &str) -> Diagnostic {
    Diagnostic {
        code,
        severity: Severity::Error,
        span: node.span,
        declaration_path: std::iter::once("screen".into())
            .chain(node.identity.path.iter().map(usize::to_string))
            .collect(),
        message: message.into(),
        hint: Some("Use the frozen Matrix version 1 consumer slice".into()),
    }
}

fn unsupported(node: &ResolvedDeclaration) -> Diagnostic {
    diagnostic(
        node,
        DiagnosticCode::LowerUnsupported,
        "The Matrix version 1 consumer cannot represent this declaration losslessly",
    )
}

fn limit(node: &ResolvedDeclaration) -> Diagnostic {
    diagnostic(
        node,
        DiagnosticCode::LowerLimit,
        "The compact Matrix version 1 tree exceeds depth 12, 2,000 nodes or 256 KiB",
    )
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

fn field(node: &ResolvedDeclaration) -> Checked<&str> {
    node.field
        .as_ref()
        .filter(|field| field.rows.is_empty())
        .map(|field| field.path.as_str())
        .ok_or_else(|| unsupported(node))
}

fn number(source: &str, node: &ResolvedDeclaration) -> Checked<f64> {
    let value: f64 = source.parse().map_err(|_| unsupported(node))?;
    if !value.is_finite() {
        return Err(unsupported(node));
    }
    let serialized = serde_json::to_string(&value).map_err(|_| unsupported(node))?;
    if Decimal::from_source(source).cmp(&Decimal::from_source(&serialized)) != Ordering::Equal {
        return Err(unsupported(node));
    }
    Ok(value)
}

fn raw_number(node: &ResolvedDeclaration, key: &str) -> Checked<f64> {
    let value = raw_value(node, key)?;
    if !value.is_number() {
        return Err(unsupported(node));
    }
    number(&value.to_string(), node)
}

fn numeric_property(node: &ResolvedDeclaration, wanted: &str) -> Checked<f64> {
    node.properties
        .iter()
        .find_map(|property| match (wanted, property) {
            ("min", Property::Min(value))
            | ("max", Property::Max(value))
            | ("step", Property::Step(value)) => Some(value.as_str()),
            _ => None,
        })
        .ok_or_else(|| unsupported(node))
        .and_then(|source| number(source, node))
}

fn optional_step(node: &ResolvedDeclaration) -> Checked<Option<f64>> {
    node.properties
        .iter()
        .find_map(|property| match property {
            Property::Step(value) => Some(value.as_str()),
            _ => None,
        })
        .map(|source| number(source, node))
        .transpose()
}

fn properties_supported(node: &ResolvedDeclaration) -> bool {
    use Element::*;
    node.properties.iter().all(|property| match node.element {
        Screen => matches!(property, Property::Title(_)),
        Col => matches!(property, Property::Alignment(_) | Property::Distribution(_)),
        Row => matches!(
            property,
            Property::Alignment(_) | Property::Distribution(_) | Property::Wrapping(_)
        ),
        Header | Footer | Text => false,
        Toggle => matches!(property, Property::Label(_)),
        Select => matches!(property, Property::Label(_)),
        Slider => matches!(
            property,
            Property::Label(_) | Property::Min(_) | Property::Max(_) | Property::Step(_)
        ),
        Button => matches!(
            property,
            Property::Label(_) | Property::Intent(_) | Property::Confirm | Property::Disabled
        ),
        _ => false,
    })
}

fn check_support(node: &ResolvedDeclaration) -> Checked<()> {
    use Element::*;
    let supported = matches!(
        node.element,
        Screen | Col | Row | Header | Footer | Text | Toggle | Select | Slider | Button
    );
    let action_supported = node.element != Button
        || node.action.as_ref().is_some_and(|action| {
            action.target == ActionTarget::Host && action.arguments.is_empty()
        });
    if !supported
        || !properties_supported(node)
        || !action_supported
        || !node.rows.is_empty()
        || !node.empty_templates.is_empty()
    {
        return Err(unsupported(node));
    }
    for child in &node.children {
        check_support(child)?;
    }
    Ok(())
}

fn alignment(node: &ResolvedDeclaration) -> Option<Alignment> {
    node.properties.iter().find_map(|property| match property {
        Property::Alignment(SourceAlignment::Start) => Some(Alignment::Start),
        Property::Alignment(SourceAlignment::Center) => Some(Alignment::Center),
        Property::Alignment(SourceAlignment::End) => Some(Alignment::End),
        _ => None,
    })
}

fn distribution(node: &ResolvedDeclaration) -> Option<Distribution> {
    node.properties.iter().find_map(|property| match property {
        Property::Distribution(SourceDistribution::Between) => Some(Distribution::Between),
        Property::Distribution(SourceDistribution::Around) => Some(Distribution::Around),
        _ => None,
    })
}

fn wrapping(node: &ResolvedDeclaration) -> Option<Wrapping> {
    node.properties.iter().find_map(|property| match property {
        Property::Wrapping(SourceWrapping::Wrap) => Some(Wrapping::Wrap),
        Property::Wrapping(SourceWrapping::NoWrap) => Some(Wrapping::Nowrap),
        _ => None,
    })
}

fn intent(node: &ResolvedDeclaration) -> Option<ButtonIntent> {
    node.properties.iter().find_map(|property| match property {
        Property::Intent(SourceButtonIntent::Primary) => Some(ButtonIntent::Primary),
        Property::Intent(SourceButtonIntent::Danger) => Some(ButtonIntent::Danger),
        _ => None,
    })
}

struct Lowerer {
    controls: BTreeMap<Vec<usize>, ControlContract>,
}

impl Lowerer {
    fn children(
        &mut self,
        source: &[ResolvedDeclaration],
        path: &mut Vec<usize>,
    ) -> Checked<Vec<Node>> {
        source
            .iter()
            .enumerate()
            .map(|(index, child)| {
                path.push(index);
                let lowered = self.lower(child, path);
                path.pop();
                lowered
            })
            .collect()
    }

    fn lower(&mut self, node: &ResolvedDeclaration, path: &mut Vec<usize>) -> Checked<Node> {
        use Element::*;
        Ok(match node.element {
            Screen => Node::Screen {
                title: node.properties.iter().find_map(|property| match property {
                    Property::Title(title) => Some(title.clone()),
                    _ => None,
                }),
                children: self.children(&node.children, path)?,
            },
            Col => Node::Col {
                fill: node.fill,
                alignment: alignment(node),
                distribution: distribution(node),
                children: self.children(&node.children, path)?,
            },
            Row => Node::Row {
                fill: node.fill,
                wrapping: wrapping(node),
                alignment: alignment(node),
                distribution: distribution(node),
                children: self.children(&node.children, path)?,
            },
            Header => Node::Header {
                children: self.children(&node.children, path)?,
            },
            Footer => Node::Footer {
                children: self.children(&node.children, path)?,
            },
            Text => Node::Text {
                value: display(node, "text")?.into(),
                fill: node.fill,
            },
            Toggle => {
                let field = field(node)?.to_owned();
                let value = raw_value(node, "field")?
                    .as_bool()
                    .ok_or_else(|| unsupported(node))?;
                self.controls.insert(
                    path.clone(),
                    ControlContract::Toggle {
                        field: field.clone(),
                    },
                );
                Node::Toggle {
                    label: label(node)?.into(),
                    field,
                    value,
                    fill: node.fill,
                }
            }
            Select => {
                let field = field(node)?.to_owned();
                let value = raw_value(node, "field")?
                    .as_str()
                    .ok_or_else(|| unsupported(node))?
                    .to_owned();
                let options = raw_value(node, "options")?
                    .as_array()
                    .ok_or_else(|| unsupported(node))?
                    .iter()
                    .map(|option| {
                        option
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| unsupported(node))
                    })
                    .collect::<Checked<Vec<_>>>()?;
                self.controls.insert(
                    path.clone(),
                    ControlContract::Select {
                        field: field.clone(),
                        options: options.clone(),
                    },
                );
                Node::Select {
                    label: label(node)?.into(),
                    field,
                    options,
                    value,
                    fill: node.fill,
                }
            }
            Slider => {
                let field = field(node)?.to_owned();
                let value = raw_number(node, "field")?;
                let min = numeric_property(node, "min")?;
                let max = numeric_property(node, "max")?;
                let step = optional_step(node)?;
                let slider = Node::Slider {
                    label: label(node)?.into(),
                    field: field.clone(),
                    value,
                    min,
                    max,
                    step,
                    fill: node.fill,
                };
                let probe = Surface {
                    version: CONTRACT_VERSION,
                    root: Node::Screen {
                        title: None,
                        children: vec![Node::Row {
                            fill: false,
                            wrapping: None,
                            alignment: None,
                            distribution: None,
                            children: vec![slider.clone()],
                        }],
                    },
                };
                probe.validate().map_err(|_| unsupported(node))?;
                self.controls.insert(
                    path.clone(),
                    ControlContract::Slider {
                        field,
                        min,
                        max,
                        step,
                    },
                );
                slider
            }
            Button => {
                let action = node.action.as_ref().ok_or_else(|| unsupported(node))?;
                let disabled = node.properties.contains(&Property::Disabled);
                self.controls.insert(
                    path.clone(),
                    ControlContract::Button {
                        event: action.name.clone(),
                        disabled,
                    },
                );
                Node::Button {
                    label: label(node)?.into(),
                    event: action.name.clone(),
                    intent: intent(node),
                    confirm: node.properties.contains(&Property::Confirm),
                    disabled,
                    fill: node.fill,
                }
            }
            _ => return Err(unsupported(node)),
        })
    }
}

pub fn lower_v1(resolved: &ResolvedSurface) -> Result<LoweredSurface, Vec<Diagnostic>> {
    let work = || -> Checked<LoweredSurface> {
        check_support(&resolved.screen)?;
        let mut lowerer = Lowerer {
            controls: BTreeMap::new(),
        };
        let root = lowerer.lower(&resolved.screen, &mut Vec::new())?;
        let surface = Surface {
            version: CONTRACT_VERSION,
            root,
        };
        let encoded = serde_json::to_vec(&surface).map_err(|_| unsupported(&resolved.screen))?;
        if encoded.len() > mote_ui::v1::MAX_BYTES {
            return Err(limit(&resolved.screen));
        }
        surface
            .validate()
            .map_err(|_| unsupported(&resolved.screen))?;
        Ok(LoweredSurface {
            surface,
            controls: lowerer.controls,
        })
    };
    work().map_err(|error| vec![error])
}
