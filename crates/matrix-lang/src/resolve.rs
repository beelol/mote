use crate::ast::*;
use crate::decimal::Decimal;
use crate::manifest::*;
use crate::types::{DisplayFormat, Type};
use crate::{Diagnostic, DiagnosticCode as DiagnosticKind, Severity, Span};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

const MAX_NODES: usize = 2_000;
const MAX_BYTES: usize = 256 * 1024;
const BUILTINS: [&str; 6] = ["Open", "Copy", "Back", "Dismiss", "Retry", "Cancel"];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowKey {
    String(String),
    Integer(String),
    Index(usize),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RowIdentity {
    pub collection_path: Vec<usize>,
    pub collection_element: Element,
    pub key: RowKey,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeIdentity {
    pub path: Vec<usize>,
    pub element: Element,
    pub rows: Vec<RowIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FieldIdentity {
    pub path: String,
    pub rows: Vec<RowIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Element {
    Screen,
    Section,
    Card,
    Col,
    Row,
    Tabs,
    Tab,
    Header,
    Footer,
    Title,
    Text,
    Caption,
    Code,
    Badge,
    Tag,
    Status,
    Label,
    Md,
    Link,
    Stat,
    Kv,
    Icon,
    Image,
    Avatar,
    Timestamp,
    Divider,
    Typing,
    Field,
    Area,
    Toggle,
    Check,
    Time,
    Search,
    Choice,
    Select,
    Multiselect,
    Segmented,
    Slider,
    Stepper,
    Date,
    Button,
    Form,
    List,
    Table,
    Grid,
    Item,
    Tile,
    Column,
    Progress,
    Steps,
    Banner,
    Empty,
    Error,
    Chat,
    Message,
    Compose,
    Dialog,
    Sheet,
    Menu,
    Tooltip,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Property {
    Title(String),
    Label(String),
    Text(String),
    Collapsible,
    Alignment(Alignment),
    Distribution(Distribution),
    Wrapping(Wrapping),
    Live,
    Icon(Icon),
    Relative,
    FieldKind(FieldKind),
    Min(String),
    Max(String),
    Step(String),
    Pattern(String),
    Intent(ButtonIntent),
    Confirm,
    Disabled,
    Key(Vec<String>),
    Selection(Selection),
    ColumnKind(ColumnKind),
    Tone(Tone),
    Role(Role),
    Trigger(MenuTrigger),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedValue {
    pub ty: Type,
    pub value: Option<Value>,
    pub display: Option<String>,
    pub format: DisplayFormat,
    pub requires_selection: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionTarget {
    Host,
    Open,
    Copy,
    Back,
    Cancel,
    SurfaceFailures,
    Collection(NodeIdentity),
    Overlay(NodeIdentity),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedAction {
    pub name: String,
    pub arguments: BTreeMap<String, ResolvedValue>,
    pub target: ActionTarget,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedRow {
    pub identity: RowIdentity,
    pub children: Vec<ResolvedDeclaration>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedDeclaration {
    pub element: Element,
    pub identity: NodeIdentity,
    pub span: Span,
    pub fill: bool,
    pub properties: Vec<Property>,
    pub values: BTreeMap<String, ResolvedValue>,
    pub field: Option<FieldIdentity>,
    pub action: Option<ResolvedAction>,
    pub children: Vec<ResolvedDeclaration>,
    pub rows: Vec<ResolvedRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSurface {
    pub manifest_version: u32,
    pub screen: ResolvedDeclaration,
}

type Checked<T> = Result<T, Diagnostic>;

fn diagnostic(
    code: DiagnosticKind,
    span: Span,
    path: &[usize],
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        severity: Severity::Error,
        span,
        declaration_path: std::iter::once("screen".into())
            .chain(path.iter().map(usize::to_string))
            .collect(),
        message: message.into(),
        hint: None,
    }
}

fn host_error(code: DiagnosticKind, message: impl Into<String>) -> Diagnostic {
    diagnostic(code, Span::new(0, 0), &[], message)
}

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn validate_type(ty: &Type, manifest: &Manifest, stack: &mut Vec<String>, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match ty {
        Type::Enum(name) => manifest.enums.contains_key(name),
        Type::Record(name) => {
            if stack.contains(name) {
                return false;
            }
            let Some(record) = manifest.records.get(name) else {
                return false;
            };
            stack.push(name.clone());
            let valid = record
                .fields
                .values()
                .all(|b| validate_type(&b.ty, manifest, stack, depth + 1));
            stack.pop();
            valid
        }
        Type::List(item) | Type::Option(item) => validate_type(item, manifest, stack, depth + 1),
        _ => true,
    }
}

fn valid_format(binding: &BindingDefinition) -> bool {
    match &binding.display {
        DisplayFormat::Canonical => true,
        DisplayFormat::Fixed { decimals } => binding.ty.is_number() && *decimals <= 18,
        DisplayFormat::Money { symbol, decimals } => {
            binding.ty.is_number() && !symbol.is_empty() && *decimals <= 18
        }
    }
}

fn absolute_url(value: &str) -> bool {
    !value
        .bytes()
        .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
        && url::Url::parse(value).is_ok()
}

fn valid_value(ty: &Type, value: &Value, manifest: &Manifest, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    match ty {
        Type::String => value.is_string(),
        Type::Number => value.as_f64().is_some_and(f64::is_finite),
        Type::Integer => value.is_i64() || value.is_u64(),
        Type::Boolean => value.is_boolean(),
        Type::Url => value.as_str().is_some_and(absolute_url),
        Type::Asset => value.as_str().is_some_and(|v| !v.is_empty()),
        Type::Date => value.as_str().is_some_and(|v| {
            v.len() == 10 && chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").is_ok()
        }),
        Type::Time => value.as_str().is_some_and(|v| {
            chrono::NaiveTime::parse_from_str(v, "%H:%M:%S%.f").is_ok()
                || chrono::NaiveTime::parse_from_str(v, "%H:%M").is_ok()
        }),
        Type::DateTime => value
            .as_str()
            .is_some_and(|v| chrono::DateTime::parse_from_rfc3339(v).is_ok()),
        Type::Enum(name) => value
            .as_str()
            .is_some_and(|v| manifest.enums[name].members.iter().any(|m| m == v)),
        Type::Record(name) => value.as_object().is_some_and(|v| {
            let fields = &manifest.records[name].fields;
            v.len() == fields.len()
                && fields.iter().all(|(name, binding)| {
                    v.get(name)
                        .is_some_and(|value| valid_value(&binding.ty, value, manifest, depth + 1))
                })
        }),
        Type::List(item) => value
            .as_array()
            .is_some_and(|v| v.iter().all(|v| valid_value(item, v, manifest, depth + 1))),
        Type::Option(item) => value.is_null() || valid_value(item, value, manifest, depth + 1),
    }
}

fn validate_host(manifest: &Manifest, snapshot: &BindingSnapshot) -> Checked<()> {
    let fail = |message| host_error(DiagnosticKind::InvalidManifest, message);
    if manifest.version != MANIFEST_VERSION {
        return Err(fail("Unsupported manifest version"));
    }
    for (name, definition) in &manifest.enums {
        let members: BTreeSet<_> = definition.members.iter().collect();
        if !identifier(name)
            || members.is_empty()
            || members.len() != definition.members.len()
            || members.iter().any(|m| m.is_empty())
            || definition.labels.keys().any(|k| !members.contains(k))
        {
            return Err(fail("Invalid closed enum definition"));
        }
    }
    let mut bindings = Vec::new();
    for (name, record) in &manifest.records {
        if !identifier(name) || record.fields.keys().any(|name| !identifier(name)) {
            return Err(fail("Invalid record name or field"));
        }
        bindings.extend(record.fields.values());
        if !validate_type(&Type::Record(name.clone()), manifest, &mut Vec::new(), 0) {
            return Err(fail("Unknown, recursive, or overly deep record type"));
        }
    }
    let mut paths = BTreeSet::new();
    for name in manifest.values.keys().chain(manifest.fields.keys()) {
        if !name.split('.').all(identifier)
            || name.split('.').next() == Some("selected")
            || !paths.insert(name)
        {
            return Err(fail("Invalid, reserved, or ambiguous binding path"));
        }
    }
    for path in &paths {
        let mut prefix = String::new();
        let parts: Vec<_> = path.split('.').collect();
        for part in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('.');
            }
            prefix.push_str(part);
            if paths.contains(&prefix) {
                return Err(fail("Binding paths overlap a declared record root"));
            }
        }
    }
    bindings.extend(manifest.values.values());
    bindings.extend(manifest.fields.values().map(|f| &f.binding));
    if bindings
        .iter()
        .any(|b| !valid_format(b) || !validate_type(&b.ty, manifest, &mut Vec::new(), 0))
    {
        return Err(fail("Unknown type or invalid display format"));
    }
    for (name, action) in &manifest.actions {
        if !identifier(name)
            || BUILTINS.contains(&name.as_str())
            || action
                .parameters
                .iter()
                .any(|(n, ty)| !identifier(n) || !validate_type(ty, manifest, &mut Vec::new(), 0))
        {
            return Err(fail("Invalid action contract or built-in shadowing"));
        }
    }
    for field in manifest.fields.values() {
        if !valid_value(&field.binding.ty, &field.initial, manifest, 0) {
            return Err(fail("Invalid initial field value"));
        }
    }
    let fail = |message| host_error(DiagnosticKind::InvalidSnapshot, message);
    if snapshot.manifest_version != manifest.version {
        return Err(fail("Snapshot manifest version does not match"));
    }
    if snapshot.values.len() != manifest.values.len()
        || snapshot.values.iter().any(|(name, value)| {
            manifest
                .values
                .get(name)
                .is_none_or(|b| !valid_value(&b.ty, value, manifest, 0))
        })
    {
        return Err(fail(
            "Snapshot values must exactly match manifest Value types",
        ));
    }
    for (name, value) in &snapshot.fields {
        if manifest
            .fields
            .get(name)
            .is_none_or(|f| !valid_value(&f.binding.ty, value, manifest, 0))
        {
            return Err(fail("Snapshot field does not match its manifest type"));
        }
    }
    for (id, value) in &snapshot.row_fields {
        if id.rows.is_empty()
            || manifest
                .fields
                .get(&id.path)
                .is_none_or(|f| !valid_value(&f.binding.ty, value, manifest, 0))
        {
            return Err(fail("Snapshot row field does not match its manifest type"));
        }
    }
    Ok(())
}

struct SizeCounter(usize);
impl Write for SizeCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        if self.0 > MAX_BYTES {
            return Err(std::io::Error::other("Resolved value exceeds byte limit"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Context {
    row: Option<ResolvedValue>,
    selected: Option<ResolvedValue>,
    scopes: Vec<FieldScope>,
    rows: Vec<RowIdentity>,
    overlay: Option<NodeIdentity>,
    retry: Option<NodeIdentity>,
    dry: bool,
}

#[derive(Clone)]
struct FieldScope {
    owner_path: Vec<usize>,
    instance: Option<Vec<RowIdentity>>,
}

struct Resolver<'a> {
    manifest: &'a Manifest,
    snapshot: &'a BindingSnapshot,
    owners: BTreeMap<Vec<usize>, BTreeSet<String>>,
    nodes: usize,
    bytes: usize,
    seen_selections: BTreeSet<NodeIdentity>,
    seen_fields: BTreeSet<FieldIdentity>,
}

fn control_path(declaration: &Declaration) -> Option<&Path> {
    use DeclarationKind::*;
    match &declaration.kind {
        Field { control, .. }
        | Date { control, .. }
        | Segmented { control, .. }
        | Area(control)
        | Toggle(control)
        | Check(control)
        | Time(control)
        | Search(control) => Some(&control.field),
        Choice(c) | Select(c) | Multiselect(c) => Some(&c.control.field),
        Slider(c) | Stepper(c) => Some(&c.control.field),
        Compose { field, .. } => Some(field),
        _ => None,
    }
}

fn collection_parts(
    declaration: &Declaration,
) -> Option<(&Reference, Option<&Path>, Option<Selection>)> {
    match &declaration.kind {
        DeclarationKind::List(c) | DeclarationKind::Table(c) | DeclarationKind::Grid(c) => Some((
            &c.source,
            c.key.as_ref(),
            c.selection.as_ref().map(|s| s.value),
        )),
        DeclarationKind::Chat { source, key, .. } => Some((source, key.as_ref(), None)),
        _ => None,
    }
}

fn template(declaration: &Declaration) -> bool {
    matches!(
        declaration.kind,
        DeclarationKind::Item(_)
            | DeclarationKind::Tile(_)
            | DeclarationKind::Column { .. }
            | DeclarationKind::Message { .. }
    )
}

impl Resolver<'_> {
    fn collect_owners(
        &mut self,
        declaration: &Declaration,
        path: &mut Vec<usize>,
        scope: &[usize],
    ) -> Checked<()> {
        if path.len() >= 12 {
            return Err(diagnostic(
                DiagnosticKind::ResolveLimit,
                declaration.span,
                path,
                "Declaration depth exceeds 12",
            ));
        }
        if let Some(field) = control_path(declaration) {
            let name = field.segments.join(".");
            if !self.manifest.fields.contains_key(&name) {
                return Err(diagnostic(
                    DiagnosticKind::ResolvePath,
                    field.span,
                    path,
                    "Control requires a manifest Field path",
                ));
            }
            if !self.owners.entry(scope.to_vec()).or_default().insert(name) {
                return Err(diagnostic(
                    DiagnosticKind::ResolveField,
                    field.span,
                    path,
                    "Two controls own the same Field in one lexical instance",
                ));
            }
        }
        let collection = collection_parts(declaration).is_some();
        let collection_scope = path.clone();
        for (index, child) in declaration.children().iter().enumerate() {
            path.push(index);
            self.collect_owners(
                child,
                path,
                if collection && template(child) {
                    &collection_scope
                } else {
                    scope
                },
            )?;
            path.pop();
        }
        Ok(())
    }

    fn charge(&mut self, bytes: usize, span: Span, path: &[usize]) -> Checked<()> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > MAX_BYTES {
            return Err(diagnostic(
                DiagnosticKind::ResolveLimit,
                span,
                path,
                "Resolved content exceeds 256 KiB",
            ));
        }
        Ok(())
    }

    fn typed(
        &mut self,
        binding: &BindingDefinition,
        value: Option<&Value>,
        requires_selection: bool,
        span: Span,
        path: &[usize],
    ) -> Checked<ResolvedValue> {
        if let Some(value) = value {
            let mut size = SizeCounter(0);
            serde_json::to_writer(&mut size, value).map_err(|_| {
                diagnostic(
                    DiagnosticKind::ResolveLimit,
                    span,
                    path,
                    "Resolved value exceeds 256 KiB",
                )
            })?;
        }
        let display = value.and_then(|v| display_value(binding, v, self.manifest));
        if let Some(display) = &display {
            if display.len() > MAX_BYTES {
                return Err(diagnostic(
                    DiagnosticKind::ResolveLimit,
                    span,
                    path,
                    "Formatted value exceeds 256 KiB",
                ));
            }
        }
        Ok(ResolvedValue {
            ty: binding.ty.clone(),
            value: value.cloned(),
            display,
            format: binding.display.clone(),
            requires_selection,
        })
    }

    fn project(
        &mut self,
        mut binding: BindingDefinition,
        mut value: Option<&Value>,
        segments: &[String],
        required: bool,
        span: Span,
        path: &[usize],
    ) -> Checked<ResolvedValue> {
        for segment in segments {
            let Type::Record(name) = &binding.ty else {
                return Err(diagnostic(
                    DiagnosticKind::ResolveType,
                    span,
                    path,
                    "Projection requires a host record type",
                ));
            };
            binding = self.manifest.records[name]
                .fields
                .get(segment)
                .ok_or_else(|| {
                    diagnostic(
                        DiagnosticKind::ResolvePath,
                        span,
                        path,
                        "Unknown record projection",
                    )
                })?
                .clone();
            value = value.and_then(|v| v.get(segment));
        }
        self.typed(&binding, value, required, span, path)
    }

    fn read(
        &mut self,
        reference: &Reference,
        context: &Context,
        path: &[usize],
        allow_field: bool,
    ) -> Checked<ResolvedValue> {
        let span = reference.span;
        let segments = &reference.path.segments;
        if reference.relative {
            let row = context.row.as_ref().ok_or_else(|| {
                diagnostic(
                    DiagnosticKind::ResolveScope,
                    span,
                    path,
                    "Relative reference is outside a row template",
                )
            })?;
            return self.project(
                BindingDefinition {
                    ty: row.ty.clone(),
                    display: row.format.clone(),
                },
                row.value.as_ref(),
                segments,
                false,
                span,
                path,
            );
        }
        if segments.first().is_some_and(|s| s == "selected") {
            let selected = context.selected.as_ref().ok_or_else(|| {
                diagnostic(
                    DiagnosticKind::ResolveScope,
                    span,
                    path,
                    "Selection is outside its parent block",
                )
            })?;
            if segments.len() == 1 {
                return self.typed(
                    &BindingDefinition {
                        ty: selected.ty.clone(),
                        display: selected.format.clone(),
                    },
                    selected.value.as_ref(),
                    selected.requires_selection,
                    span,
                    path,
                );
            }
            let Type::Option(row) = &selected.ty else {
                return Err(diagnostic(
                    DiagnosticKind::ResolveScope,
                    span,
                    path,
                    "Cannot project across a selected list",
                ));
            };
            return self.project(
                (**row).clone().into(),
                selected.value.as_ref().filter(|v| !v.is_null()),
                &segments[1..],
                selected.requires_selection,
                span,
                path,
            );
        }
        let name = segments.join(".");
        if allow_field && self.manifest.fields.contains_key(&name) {
            return self
                .field_value(&reference.path, context, path)
                .map(|(_, value)| value);
        }
        for split in (1..=segments.len()).rev() {
            let prefix = segments[..split].join(".");
            if let Some(binding) = self.manifest.values.get(&prefix) {
                return self.project(
                    binding.clone(),
                    self.snapshot.values.get(&prefix),
                    &segments[split..],
                    false,
                    span,
                    path,
                );
            }
        }
        Err(diagnostic(
            DiagnosticKind::ResolvePath,
            span,
            path,
            "Unknown Value path",
        ))
    }

    fn field_value(
        &mut self,
        field: &Path,
        context: &Context,
        path: &[usize],
    ) -> Checked<(FieldIdentity, ResolvedValue)> {
        let name = field.segments.join(".");
        let definition = self.manifest.fields.get(&name).ok_or_else(|| {
            diagnostic(
                DiagnosticKind::ResolvePath,
                field.span,
                path,
                "Unknown Field path",
            )
        })?;
        let scope = context
            .scopes
            .iter()
            .rev()
            .find(|scope| {
                self.owners
                    .get(&scope.owner_path)
                    .is_some_and(|owners| owners.contains(&name))
            })
            .ok_or_else(|| {
                diagnostic(
                    DiagnosticKind::ResolveScope,
                    field.span,
                    path,
                    "Field is not owned in this lexical instance",
                )
            })?;
        let identity = FieldIdentity {
            path: name.clone(),
            rows: scope.instance.clone().unwrap_or_default(),
        };
        let value = match &scope.instance {
            Some(rows) if rows.is_empty() => self.snapshot.fields.get(&name),
            Some(_) => self.snapshot.row_fields.get(&identity),
            None => None,
        }
        .unwrap_or(&definition.initial);
        if !context.dry {
            self.seen_fields.insert(identity.clone());
        }
        Ok((
            identity,
            self.typed(&definition.binding, Some(value), false, field.span, path)?,
        ))
    }

    fn require(
        &self,
        value: &ResolvedValue,
        allowed: impl FnOnce(&Type) -> bool,
        span: Span,
        path: &[usize],
    ) -> Checked<()> {
        if allowed(&value.ty) {
            Ok(())
        } else {
            Err(diagnostic(
                DiagnosticKind::ResolveType,
                span,
                path,
                format!("Incompatible host type {:?}", value.ty),
            ))
        }
    }

    fn operand(
        &mut self,
        operand: &DisplayOperand,
        context: &Context,
        path: &[usize],
        string_only: bool,
    ) -> Checked<ResolvedValue> {
        match operand {
            DisplayOperand::Literal(literal) => self.typed(
                &Type::String.into(),
                Some(&Value::String(literal.value.clone())),
                false,
                literal.span,
                path,
            ),
            DisplayOperand::Reference(reference) => {
                let value = self.read(reference, context, path, false)?;
                self.require(
                    &value,
                    |ty| {
                        if string_only {
                            *ty == Type::String
                        } else {
                            ty.is_display()
                        }
                    },
                    reference.span,
                    path,
                )?;
                Ok(value)
            }
        }
    }

    fn action(
        &mut self,
        action: &Action,
        context: &Context,
        path: &[usize],
        disabled: bool,
    ) -> Checked<ResolvedAction> {
        let name = action.name.value.as_str();
        let (contract, target) = match name {
            "Open" => (
                BTreeMap::from([("url".into(), Type::Url)]),
                ActionTarget::Open,
            ),
            "Copy" => (
                BTreeMap::from([("text".into(), Type::String)]),
                ActionTarget::Copy,
            ),
            "Back" => (BTreeMap::new(), ActionTarget::Back),
            "Cancel" => (BTreeMap::new(), ActionTarget::Cancel),
            "Retry" => (
                BTreeMap::new(),
                context
                    .retry
                    .clone()
                    .map_or(ActionTarget::SurfaceFailures, ActionTarget::Collection),
            ),
            "Dismiss" => (
                BTreeMap::new(),
                ActionTarget::Overlay(context.overlay.clone().ok_or_else(|| {
                    diagnostic(
                        DiagnosticKind::ResolveScope,
                        action.span,
                        path,
                        "Dismiss requires an enclosing overlay",
                    )
                })?),
            ),
            _ => (
                self.manifest
                    .actions
                    .get(name)
                    .ok_or_else(|| {
                        diagnostic(
                            DiagnosticKind::ResolveAction,
                            action.span,
                            path,
                            "Unknown action",
                        )
                    })?
                    .parameters
                    .clone(),
                ActionTarget::Host,
            ),
        };
        if contract.len() != action.arguments.len() {
            return Err(diagnostic(
                DiagnosticKind::ResolveAction,
                action.span,
                path,
                "Action argument names must match its contract exactly",
            ));
        }
        let mut arguments = BTreeMap::new();
        let mut disabled = disabled;
        for argument in &action.arguments {
            let expected = contract.get(&argument.name.value).ok_or_else(|| {
                diagnostic(
                    DiagnosticKind::ResolveAction,
                    argument.span,
                    path,
                    "Unexpected action argument",
                )
            })?;
            let value = self.read(&argument.reference, context, path, true)?;
            self.require(&value, |ty| expected.accepts(ty), argument.span, path)?;
            disabled |= value.requires_selection;
            if arguments
                .insert(argument.name.value.clone(), value)
                .is_some()
            {
                return Err(diagnostic(
                    DiagnosticKind::ResolveAction,
                    argument.span,
                    path,
                    "Duplicate action argument",
                ));
            }
        }
        Ok(ResolvedAction {
            name: name.into(),
            arguments,
            target,
            disabled,
        })
    }

    fn selection(
        &mut self,
        declaration: &Declaration,
        path: &[usize],
        context: &Context,
    ) -> Checked<Option<ResolvedValue>> {
        let Some((source, _, Some(selection))) = collection_parts(declaration) else {
            return Ok(None);
        };
        let value = self.read(source, context, path, false)?;
        let Type::List(row) = &value.ty else {
            return Err(diagnostic(
                DiagnosticKind::ResolveType,
                source.span,
                path,
                "Collection source must be a list",
            ));
        };
        let identity = NodeIdentity {
            path: path.to_vec(),
            element: element(&declaration.kind),
            rows: context.rows.clone(),
        };
        if !context.dry {
            self.seen_selections.insert(identity.clone());
        }
        let indices = self
            .snapshot
            .selections
            .get(&identity)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let rows = value
            .value
            .as_ref()
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if !context.dry
            && (indices.iter().copied().collect::<BTreeSet<_>>().len() != indices.len()
                || indices.iter().any(|i| *i >= rows.len())
                || (selection == Selection::One && indices.len() > 1))
        {
            return Err(diagnostic(
                DiagnosticKind::InvalidSnapshot,
                declaration.span,
                path,
                "Invalid selection indices for this collection snapshot",
            ));
        }
        let (ty, value, missing) = match selection {
            Selection::One => (
                Type::Option(row.clone()),
                indices
                    .first()
                    .and_then(|i| rows.get(*i))
                    .cloned()
                    .unwrap_or(Value::Null),
                indices.is_empty(),
            ),
            Selection::Many => (
                Type::List(row.clone()),
                Value::Array(
                    indices
                        .iter()
                        .filter_map(|i| rows.get(*i).cloned())
                        .collect(),
                ),
                false,
            ),
        };
        self.typed(&ty.into(), Some(&value), missing, declaration.span, path)
            .map(Some)
    }

    fn block(
        &mut self,
        declarations: &[Declaration],
        parent: &[usize],
        context: &Context,
    ) -> Checked<Vec<ResolvedDeclaration>> {
        let mut context = context.clone();
        let mut found = false;
        for (index, declaration) in declarations.iter().enumerate() {
            let path: Vec<_> = parent.iter().copied().chain([index]).collect();
            if let Some(selected) = self.selection(declaration, &path, &context)? {
                if found {
                    return Err(diagnostic(
                        DiagnosticKind::ResolveScope,
                        declaration.span,
                        &path,
                        "A block may contain only one selectable collection",
                    ));
                }
                context.selected = Some(selected);
                found = true;
            }
        }
        declarations
            .iter()
            .enumerate()
            .map(|(index, declaration)| {
                let path: Vec<_> = parent.iter().copied().chain([index]).collect();
                self.declaration(declaration, &path, &context)
            })
            .collect()
    }

    fn control(
        &mut self,
        node: &mut ResolvedDeclaration,
        control: &LabeledControl,
        context: &Context,
        allowed: impl FnOnce(&Type) -> bool,
    ) -> Checked<()> {
        let (identity, value) = self.field_value(&control.field, context, &node.identity.path)?;
        self.require(&value, allowed, control.field.span, &node.identity.path)?;
        node.properties
            .push(Property::Label(control.label.value.clone()));
        node.field = Some(identity);
        node.values.insert("field".into(), value);
        Ok(())
    }

    fn options(
        &mut self,
        node: &mut ResolvedDeclaration,
        options: &Options,
        context: &Context,
        many: bool,
    ) -> Checked<()> {
        let field = &node.values["field"];
        let enum_type = if many {
            let Type::List(item) = &field.ty else {
                return Err(diagnostic(
                    DiagnosticKind::ResolveType,
                    node.span,
                    &node.identity.path,
                    "Multiselect requires a list of enum values",
                ));
            };
            item.as_ref()
        } else {
            &field.ty
        };
        let Type::Enum(name) = enum_type else {
            return Err(diagnostic(
                DiagnosticKind::ResolveType,
                node.span,
                &node.identity.path,
                "Options require a closed enum Field",
            ));
        };
        let expected = Type::List(Box::new(enum_type.clone()));
        let value = match options {
            Options::Literal(options) => {
                let value = Value::Array(
                    options
                        .iter()
                        .map(|s| Value::String(s.value.clone()))
                        .collect(),
                );
                if !valid_value(&expected, &value, self.manifest, 0) {
                    return Err(diagnostic(
                        DiagnosticKind::ResolveConstraint,
                        node.span,
                        &node.identity.path,
                        "Option is not in the Field's closed enum",
                    ));
                }
                self.typed(
                    &expected.clone().into(),
                    Some(&value),
                    false,
                    node.span,
                    &node.identity.path,
                )?
            }
            Options::Reference(reference) => {
                let value = self.read(reference, context, &node.identity.path, false)?;
                self.require(
                    &value,
                    |ty| *ty == expected,
                    reference.span,
                    &node.identity.path,
                )?;
                value
            }
        };
        if let Some(options) = value.value.as_ref().and_then(Value::as_array) {
            let set: BTreeSet<_> = options.iter().filter_map(Value::as_str).collect();
            if set.is_empty()
                || set.len() != options.len()
                || set
                    .iter()
                    .any(|s| !self.manifest.enums[name].members.iter().any(|m| m == s))
            {
                return Err(diagnostic(
                    DiagnosticKind::ResolveConstraint,
                    node.span,
                    &node.identity.path,
                    "Options must be nonempty unique enum members",
                ));
            }
            let current: Vec<_> = if many {
                field
                    .value
                    .as_ref()
                    .and_then(Value::as_array)
                    .map(|v| v.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default()
            } else {
                field
                    .value
                    .as_ref()
                    .and_then(Value::as_str)
                    .into_iter()
                    .collect()
            };
            if current.iter().any(|v| !set.contains(v))
                || current.iter().collect::<BTreeSet<_>>().len() != current.len()
            {
                return Err(diagnostic(
                    DiagnosticKind::ResolveConstraint,
                    node.span,
                    &node.identity.path,
                    "Field selection must occur once in the offered options",
                ));
            }
        }
        node.values.insert("options".into(), value);
        Ok(())
    }

    fn numeric_constraints(
        &self,
        node: &mut ResolvedDeclaration,
        min: Option<&NumberLiteral>,
        max: Option<&NumberLiteral>,
        step: Option<&NumberLiteral>,
        text: bool,
    ) -> Checked<()> {
        let invalid = min.zip(max).is_some_and(|(min, max)| {
            Decimal::from_source(&min.source)
                .cmp(&Decimal::from_source(&max.source))
                .is_gt()
        }) || step.is_some_and(|s| !Decimal::from_source(&s.source).is_positive())
            || (text
                && min
                    .into_iter()
                    .chain(max)
                    .any(|n| !Decimal::from_source(&n.source).is_nonnegative_integer()));
        if invalid {
            return Err(diagnostic(
                DiagnosticKind::ResolveConstraint,
                node.span,
                &node.identity.path,
                "Invalid numeric constraints",
            ));
        }
        if let Some(min) = min {
            node.properties.push(Property::Min(min.source.clone()));
        }
        if let Some(max) = max {
            node.properties.push(Property::Max(max.source.clone()));
        }
        if let Some(step) = step {
            node.properties.push(Property::Step(step.source.clone()));
        }
        Ok(())
    }

    fn collection(
        &mut self,
        node: &mut ResolvedDeclaration,
        declaration: &Declaration,
        context: &Context,
    ) -> Checked<()> {
        let (source, key, selection) = collection_parts(declaration).unwrap();
        let value = self.read(source, context, &node.identity.path, false)?;
        let Type::List(row_type) = &value.ty else {
            return Err(diagnostic(
                DiagnosticKind::ResolveType,
                source.span,
                &node.identity.path,
                "Collection source must be a list",
            ));
        };
        if let Some(key) = key {
            let key_type = self.project(
                (**row_type).clone().into(),
                None,
                &key.segments,
                false,
                key.span,
                &node.identity.path,
            )?;
            if !matches!(key_type.ty, Type::String | Type::Integer) {
                return Err(diagnostic(
                    DiagnosticKind::ResolveKey,
                    key.span,
                    &node.identity.path,
                    "Key must have a stable string or integer type",
                ));
            }
            node.properties.push(Property::Key(key.segments.clone()));
        }
        if let Some(selection) = selection {
            node.properties.push(Property::Selection(selection));
        }
        let rows = value
            .value
            .as_ref()
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let templates = declaration
            .children()
            .iter()
            .filter(|d| template(d))
            .count();
        if rows
            .len()
            .saturating_mul(templates)
            .saturating_add(self.nodes)
            > MAX_NODES
        {
            return Err(diagnostic(
                DiagnosticKind::ResolveLimit,
                declaration.span,
                &node.identity.path,
                "Expanded collection exceeds 2,000 nodes",
            ));
        }
        let mut identities = BTreeSet::new();
        let mut row_keys = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let row_key = if let Some(key) = key {
                let value = self.project(
                    (**row_type).clone().into(),
                    Some(row),
                    &key.segments,
                    false,
                    key.span,
                    &node.identity.path,
                )?;
                match value.value.as_ref() {
                    Some(Value::String(v)) => RowKey::String(v.clone()),
                    Some(Value::Number(n)) if n.is_i64() || n.is_u64() => {
                        RowKey::Integer(n.to_string())
                    }
                    _ => {
                        return Err(diagnostic(
                            DiagnosticKind::ResolveKey,
                            key.span,
                            &node.identity.path,
                            "Key must be a nonnull string or integer",
                        ))
                    }
                }
            } else {
                RowKey::Index(index)
            };
            if !identities.insert(row_key.clone()) {
                return Err(diagnostic(
                    DiagnosticKind::ResolveKey,
                    declaration.span,
                    &node.identity.path,
                    "Duplicate key in collection snapshot",
                ));
            }
            row_keys.push(row_key);
        }
        if rows.is_empty() {
            let old_nodes = self.nodes;
            let old_bytes = self.bytes;
            let mut row_context = context.clone();
            row_context.dry = true;
            row_context.row = Some(ResolvedValue {
                ty: (**row_type).clone(),
                value: None,
                display: None,
                format: DisplayFormat::Canonical,
                requires_selection: false,
            });
            row_context.scopes.push(FieldScope {
                owner_path: node.identity.path.clone(),
                instance: None,
            });
            for (index, child) in declaration
                .children()
                .iter()
                .enumerate()
                .filter(|(_, d)| template(d))
            {
                let path: Vec<_> = node.identity.path.iter().copied().chain([index]).collect();
                self.declaration(child, &path, &row_context)?;
            }
            self.nodes = old_nodes;
            self.bytes = old_bytes;
        }
        for (row, key) in rows.iter().zip(row_keys) {
            let identity = RowIdentity {
                collection_path: node.identity.path.clone(),
                collection_element: node.element,
                key,
            };
            let mut row_context = context.clone();
            row_context.rows.push(identity.clone());
            row_context.scopes.push(FieldScope {
                owner_path: node.identity.path.clone(),
                instance: (!context.dry).then(|| row_context.rows.clone()),
            });
            row_context.row = Some(ResolvedValue {
                ty: (**row_type).clone(),
                value: Some(row.clone()),
                display: None,
                format: DisplayFormat::Canonical,
                requires_selection: false,
            });
            let mut children = Vec::new();
            for (index, child) in declaration
                .children()
                .iter()
                .enumerate()
                .filter(|(_, d)| template(d))
            {
                let path: Vec<_> = node.identity.path.iter().copied().chain([index]).collect();
                children.push(self.declaration(child, &path, &row_context)?);
            }
            node.rows.push(ResolvedRow { identity, children });
        }
        let mut override_context = context.clone();
        override_context.retry = Some(node.identity.clone());
        for (index, child) in declaration
            .children()
            .iter()
            .enumerate()
            .filter(|(_, d)| !template(d))
        {
            let path: Vec<_> = node.identity.path.iter().copied().chain([index]).collect();
            node.children
                .push(self.declaration(child, &path, &override_context)?);
        }
        Ok(())
    }

    fn declaration(
        &mut self,
        declaration: &Declaration,
        path: &[usize],
        context: &Context,
    ) -> Checked<ResolvedDeclaration> {
        use DeclarationKind::*;
        self.nodes += 1;
        if self.nodes > MAX_NODES || path.len() >= 12 {
            return Err(diagnostic(
                DiagnosticKind::ResolveLimit,
                declaration.span,
                path,
                "Resolved tree exceeds depth or node limit",
            ));
        }
        self.charge(16, declaration.span, path)?;
        let mut node = ResolvedDeclaration {
            element: element(&declaration.kind),
            identity: NodeIdentity {
                path: path.to_vec(),
                element: element(&declaration.kind),
                rows: context.rows.clone(),
            },
            span: declaration.span,
            fill: declaration.fill.is_some(),
            properties: Vec::new(),
            values: BTreeMap::new(),
            field: None,
            action: None,
            children: Vec::new(),
            rows: Vec::new(),
        };
        let mut child_context = context.clone();
        match &declaration.kind {
            Screen(c) | Section(c) | Form(c) => {
                if let Some(title) = &c.title {
                    node.properties.push(Property::Title(title.value.clone()));
                }
            }
            Card {
                title, collapsible, ..
            } => {
                if let Some(title) = title {
                    node.properties.push(Property::Title(title.value.clone()));
                }
                if collapsible.is_some() {
                    node.properties.push(Property::Collapsible);
                }
            }
            Col {
                alignment,
                distribution,
                ..
            }
            | Row {
                alignment,
                distribution,
                ..
            } => {
                if let Some(alignment) = alignment {
                    node.properties.push(Property::Alignment(alignment.value));
                }
                if let Some(distribution) = distribution {
                    node.properties
                        .push(Property::Distribution(distribution.value));
                }
                if let Row {
                    wrapping: Some(wrapping),
                    ..
                } = &declaration.kind
                {
                    node.properties.push(Property::Wrapping(wrapping.value));
                }
            }
            Tab { title, .. } => node.properties.push(Property::Title(title.value.clone())),
            Title(value)
            | Text(value)
            | Caption(value)
            | Code(value)
            | Badge(value)
            | Tag(value)
            | Status { text: value, .. }
            | Banner { text: value, .. } => {
                node.values
                    .insert("text".into(), self.operand(value, context, path, false)?);
                if let Status { live: Some(_), .. } = declaration.kind {
                    node.properties.push(Property::Live);
                }
                if let Banner { tone, action, .. } = &declaration.kind {
                    node.properties.push(Property::Tone(tone.value));
                    if let Some(action) = action {
                        node.action = Some(self.action(action, context, path, false)?);
                    }
                }
            }
            Label(text) | Empty(text) => node.properties.push(Property::Text(text.value.clone())),
            Md(reference) => {
                let value = self.read(reference, context, path, false)?;
                self.require(&value, |ty| *ty == Type::String, reference.span, path)?;
                node.values.insert("text".into(), value);
            }
            Link { text, to } => {
                node.values
                    .insert("text".into(), self.operand(text, context, path, false)?);
                let value = self.read(to, context, path, false)?;
                self.require(&value, |ty| *ty == Type::Url, to.span, path)?;
                node.values.insert("to".into(), value);
            }
            Stat { label, value, .. } | Kv { label, value } => {
                node.properties.push(Property::Label(label.value.clone()));
                let resolved = self.read(value, context, path, false)?;
                self.require(
                    &resolved,
                    |ty| {
                        if matches!(declaration.kind, Kv { .. }) {
                            ty.is_display()
                        } else {
                            *ty == Type::String || ty.is_number()
                        }
                    },
                    value.span,
                    path,
                )?;
                node.values.insert("value".into(), resolved);
                if let Stat {
                    delta: Some(delta), ..
                } = &declaration.kind
                {
                    let value = self.read(delta, context, path, false)?;
                    self.require(
                        &value,
                        |ty| *ty == Type::String || ty.is_number(),
                        delta.span,
                        path,
                    )?;
                    node.values.insert("delta".into(), value);
                }
            }
            Icon(icon) => node.properties.push(Property::Icon(icon.value)),
            Image { source, alt } | Avatar { source, alt } => {
                let value = self.read(source, context, path, false)?;
                self.require(&value, |ty| *ty == Type::Asset, source.span, path)?;
                node.values.insert("source".into(), value);
                node.values
                    .insert("alt".into(), self.operand(alt, context, path, true)?);
            }
            Timestamp { value, relative } => {
                let resolved = self.read(value, context, path, false)?;
                self.require(&resolved, |ty| *ty == Type::DateTime, value.span, path)?;
                node.values.insert("value".into(), resolved);
                if relative.is_some() {
                    node.properties.push(Property::Relative);
                }
            }
            Field {
                control,
                kind,
                min,
                max,
                pattern,
            } => {
                let kind = kind.as_ref().map(|k| k.value).unwrap_or(FieldKind::Text);
                self.control(&mut node, control, context, |ty| {
                    if kind == FieldKind::Number {
                        ty.is_number()
                    } else {
                        *ty == Type::String
                    }
                })?;
                if kind != FieldKind::Text {
                    node.properties.push(Property::FieldKind(kind));
                }
                self.numeric_constraints(
                    &mut node,
                    min.as_ref(),
                    max.as_ref(),
                    None,
                    kind != FieldKind::Number,
                )?;
                if let Some(pattern) = pattern {
                    if kind == FieldKind::Number || regex::Regex::new(&pattern.value).is_err() {
                        return Err(diagnostic(
                            DiagnosticKind::ResolveConstraint,
                            pattern.span,
                            path,
                            "Pattern must be a valid Rust regex on a string Field",
                        ));
                    }
                    node.properties
                        .push(Property::Pattern(pattern.value.clone()));
                }
            }
            Area(control) | Search(control) => {
                self.control(&mut node, control, context, |ty| *ty == Type::String)?
            }
            Toggle(control) | Check(control) => {
                self.control(&mut node, control, context, |ty| *ty == Type::Boolean)?
            }
            Time(control) => self.control(&mut node, control, context, |ty| *ty == Type::Time)?,
            Choice(control) | Select(control) | Multiselect(control) => {
                self.control(&mut node, &control.control, context, |_| true)?;
                self.options(
                    &mut node,
                    &control.options,
                    context,
                    matches!(declaration.kind, Multiselect(_)),
                )?;
            }
            Segmented { control, options } => {
                self.control(&mut node, control, context, |_| true)?;
                self.options(
                    &mut node,
                    &Options::Literal(options.clone()),
                    context,
                    false,
                )?;
            }
            Slider(control) | Stepper(control) => {
                self.control(&mut node, &control.control, context, Type::is_number)?;
                self.numeric_constraints(
                    &mut node,
                    control.min.as_ref(),
                    control.max.as_ref(),
                    control.step.as_ref(),
                    false,
                )?;
            }
            Date { control, min, max } => {
                self.control(&mut node, control, context, |ty| *ty == Type::Date)?;
                for (name, reference) in [("min", min), ("max", max)] {
                    if let Some(reference) = reference {
                        let value = self.read(reference, context, path, false)?;
                        self.require(&value, |ty| *ty == Type::Date, reference.span, path)?;
                        node.values.insert(name.into(), value);
                    }
                }
                if let (Some(min), Some(max)) = (node.values.get("min"), node.values.get("max")) {
                    if min
                        .value
                        .as_ref()
                        .and_then(Value::as_str)
                        .zip(max.value.as_ref().and_then(Value::as_str))
                        .is_some_and(|(a, b)| a > b)
                    {
                        return Err(diagnostic(
                            DiagnosticKind::ResolveConstraint,
                            declaration.span,
                            path,
                            "Date min exceeds max",
                        ));
                    }
                }
            }
            Button {
                label,
                intent,
                confirm,
                disabled,
                action,
            } => {
                node.properties.push(Property::Label(label.value.clone()));
                if let Some(intent) = intent {
                    node.properties.push(Property::Intent(intent.value));
                }
                if confirm.is_some() {
                    node.properties.push(Property::Confirm);
                }
                if disabled.is_some() {
                    node.properties.push(Property::Disabled);
                }
                node.action = Some(self.action(action, context, path, disabled.is_some())?);
            }
            List(_) | Table(_) | Grid(_) | Chat { .. } => {
                self.collection(&mut node, declaration, context)?
            }
            Column { label, value, kind } => {
                node.properties.push(Property::Label(label.value.clone()));
                let resolved = self.read(value, context, path, false)?;
                let default_kind = match (&resolved.ty, &resolved.format) {
                    (Type::String, _) => Some(ColumnKind::Text),
                    (Type::Number | Type::Integer, DisplayFormat::Money { .. }) => {
                        Some(ColumnKind::Money)
                    }
                    (Type::Number | Type::Integer, _) => Some(ColumnKind::Number),
                    (Type::Date | Type::DateTime, _) => Some(ColumnKind::Date),
                    (Type::Enum(_), _) => Some(ColumnKind::Status),
                    _ => None,
                };
                if default_kind.is_none()
                    || kind.as_ref().is_some_and(|k| Some(k.value) != default_kind)
                {
                    return Err(diagnostic(
                        DiagnosticKind::ResolveType,
                        value.span,
                        path,
                        "Column kind must match the host type and formatter",
                    ));
                }
                node.properties
                    .push(Property::ColumnKind(default_kind.unwrap()));
                node.values.insert("value".into(), resolved);
            }
            Progress { value, label } => {
                if let Some(label) = label {
                    node.properties.push(Property::Label(label.value.clone()));
                }
                if let Some(reference) = value {
                    let value = self.read(reference, context, path, false)?;
                    self.require(&value, Type::is_number, reference.span, path)?;
                    if value.value.as_ref().is_some_and(|v| {
                        let number = Decimal::from_source(&v.to_string());
                        number.cmp(&Decimal::from_source("0")).is_lt()
                            || number.cmp(&Decimal::from_source("1")).is_gt()
                    }) {
                        return Err(diagnostic(
                            DiagnosticKind::ResolveConstraint,
                            reference.span,
                            path,
                            "Progress must be between zero and one",
                        ));
                    }
                    node.values.insert("value".into(), value);
                }
            }
            Steps { stages, current } => {
                let value = self.read(current, context, path, false)?;
                self.require(
                    &value,
                    |ty| matches!(ty, Type::String | Type::Enum(_)),
                    current.span,
                    path,
                )?;
                let stage_set: BTreeSet<_> = stages.iter().map(|s| s.value.as_str()).collect();
                let enum_matches = if let Type::Enum(name) = &value.ty {
                    self.manifest.enums[name]
                        .members
                        .iter()
                        .map(String::as_str)
                        .collect::<BTreeSet<_>>()
                        == stage_set
                } else {
                    true
                };
                if stage_set.is_empty()
                    || stage_set.len() != stages.len()
                    || stage_set.contains("")
                    || !enum_matches
                    || value
                        .value
                        .as_ref()
                        .and_then(Value::as_str)
                        .is_some_and(|s| !stage_set.contains(s))
                {
                    return Err(diagnostic(
                        DiagnosticKind::ResolveConstraint,
                        current.span,
                        path,
                        "Stages must be unique and match the current value and enum",
                    ));
                }
                let stage_values = Value::Array(
                    stages
                        .iter()
                        .map(|s| Value::String(s.value.clone()))
                        .collect(),
                );
                node.values.insert(
                    "stages".into(),
                    self.typed(
                        &Type::List(Box::new(Type::String)).into(),
                        Some(&stage_values),
                        false,
                        declaration.span,
                        path,
                    )?,
                );
                node.values.insert("current".into(), value);
            }
            Error { text, retry } => {
                node.properties.push(Property::Text(text.value.clone()));
                if let Some(action) = retry {
                    node.action = Some(self.action(action, context, path, false)?);
                }
            }
            Message { role, body, .. } => {
                match role {
                    MessageRole::Literal(role) => node.properties.push(Property::Role(role.value)),
                    MessageRole::Reference(reference) => {
                        let value = self.read(reference, context, path, false)?;
                        self.require(
                            &value,
                            |ty| {
                                if let Type::Enum(name) = ty {
                                    self.manifest.enums[name]
                                        .members
                                        .iter()
                                        .map(String::as_str)
                                        .collect::<BTreeSet<_>>()
                                        == BTreeSet::from(["me", "them", "system"])
                                } else {
                                    false
                                }
                            },
                            reference.span,
                            path,
                        )?;
                        node.values.insert("role".into(), value);
                    }
                }
                let value = self.read(body, context, path, false)?;
                self.require(&value, |ty| *ty == Type::String, body.span, path)?;
                node.values.insert("body".into(), value);
            }
            Compose { field, action } => {
                let (identity, value) = self.field_value(field, context, path)?;
                self.require(&value, |ty| *ty == Type::String, field.span, path)?;
                node.field = Some(identity);
                node.values.insert("field".into(), value);
                node.action = Some(self.action(action, context, path, false)?);
            }
            Dialog { title, .. } | Sheet { title, .. } => {
                node.properties.push(Property::Title(title.value.clone()));
                child_context.overlay = Some(node.identity.clone());
            }
            Menu { trigger, .. } => {
                node.properties.push(Property::Trigger(trigger.clone()));
                child_context.overlay = Some(node.identity.clone());
            }
            Tooltip { text, .. } => {
                node.properties.push(Property::Text(text.value.clone()));
                child_context.overlay = Some(node.identity.clone());
            }
            Tabs(_) | Header(_) | Footer(_) | Divider | Typing | Item(_) | Tile(_) => {}
        }
        if collection_parts(declaration).is_none() {
            node.children = self.block(declaration.children(), path, &child_context)?;
        }
        for value in node
            .values
            .values()
            .chain(node.action.iter().flat_map(|a| a.arguments.values()))
        {
            let mut raw_size = SizeCounter(0);
            let mut display_size = SizeCounter(0);
            let measured = value
                .value
                .as_ref()
                .map(|v| serde_json::to_writer(&mut raw_size, v))
                .transpose()
                .and_then(|_| {
                    value
                        .display
                        .as_ref()
                        .map(|v| serde_json::to_writer(&mut display_size, v))
                        .transpose()
                });
            if measured.is_err() {
                return Err(diagnostic(
                    DiagnosticKind::ResolveLimit,
                    declaration.span,
                    path,
                    "Resolved value exceeds 256 KiB",
                ));
            }
            self.charge(raw_size.0.max(display_size.0), declaration.span, path)?;
        }
        for property in &node.properties {
            let size = match property {
                Property::Title(s)
                | Property::Label(s)
                | Property::Text(s)
                | Property::Min(s)
                | Property::Max(s)
                | Property::Step(s)
                | Property::Pattern(s) => s.len(),
                Property::Trigger(MenuTrigger::Label(s)) => s.value.len(),
                _ => 0,
            };
            self.charge(size, declaration.span, path)?;
        }
        Ok(node)
    }
}

fn element(kind: &DeclarationKind) -> Element {
    use DeclarationKind::*;
    match kind {
        Screen(_) => Element::Screen,
        Section(_) => Element::Section,
        Card { .. } => Element::Card,
        Col { .. } => Element::Col,
        Row { .. } => Element::Row,
        Tabs(_) => Element::Tabs,
        Tab { .. } => Element::Tab,
        Header(_) => Element::Header,
        Footer(_) => Element::Footer,
        Title(_) => Element::Title,
        Text(_) => Element::Text,
        Caption(_) => Element::Caption,
        Code(_) => Element::Code,
        Badge(_) => Element::Badge,
        Tag(_) => Element::Tag,
        Status { .. } => Element::Status,
        Label(_) => Element::Label,
        Md(_) => Element::Md,
        Link { .. } => Element::Link,
        Stat { .. } => Element::Stat,
        Kv { .. } => Element::Kv,
        Icon(_) => Element::Icon,
        Image { .. } => Element::Image,
        Avatar { .. } => Element::Avatar,
        Timestamp { .. } => Element::Timestamp,
        Divider => Element::Divider,
        Typing => Element::Typing,
        Field { .. } => Element::Field,
        Area(_) => Element::Area,
        Toggle(_) => Element::Toggle,
        Check(_) => Element::Check,
        Time(_) => Element::Time,
        Search(_) => Element::Search,
        Choice(_) => Element::Choice,
        Select(_) => Element::Select,
        Multiselect(_) => Element::Multiselect,
        Segmented { .. } => Element::Segmented,
        Slider(_) => Element::Slider,
        Stepper(_) => Element::Stepper,
        Date { .. } => Element::Date,
        Button { .. } => Element::Button,
        Form(_) => Element::Form,
        List(_) => Element::List,
        Table(_) => Element::Table,
        Grid(_) => Element::Grid,
        Item(_) => Element::Item,
        Tile(_) => Element::Tile,
        Column { .. } => Element::Column,
        Progress { .. } => Element::Progress,
        Steps { .. } => Element::Steps,
        Banner { .. } => Element::Banner,
        Empty(_) => Element::Empty,
        Error { .. } => Element::Error,
        Chat { .. } => Element::Chat,
        Message { .. } => Element::Message,
        Compose { .. } => Element::Compose,
        Dialog { .. } => Element::Dialog,
        Sheet { .. } => Element::Sheet,
        Menu { .. } => Element::Menu,
        Tooltip { .. } => Element::Tooltip,
    }
}

fn display_value(
    binding: &BindingDefinition,
    value: &Value,
    manifest: &Manifest,
) -> Option<String> {
    match (&binding.ty, &binding.display) {
        (Type::Enum(name), _) => value.as_str().map(|member| {
            manifest.enums[name]
                .labels
                .get(member)
                .cloned()
                .unwrap_or_else(|| member.into())
        }),
        (Type::Number | Type::Integer, DisplayFormat::Fixed { decimals }) => {
            fixed(value, *decimals)
        }
        (Type::Number | Type::Integer, DisplayFormat::Money { symbol, decimals }) => {
            fixed(value, *decimals).map(|v| format!("{symbol}{v}"))
        }
        (Type::Number | Type::Integer, _) => Some(value.to_string()),
        (Type::String | Type::Url | Type::Asset | Type::Date | Type::Time | Type::DateTime, _) => {
            value.as_str().map(String::from)
        }
        _ => None,
    }
}

fn fixed(value: &Value, decimals: u8) -> Option<String> {
    if value.is_i64() || value.is_u64() {
        return Some(if decimals == 0 {
            value.to_string()
        } else {
            format!("{}.{}", value, "0".repeat(decimals as usize))
        });
    }
    value
        .as_f64()
        .map(|n| format!("{n:.width$}", width = decimals as usize))
}

pub fn resolve(
    surface: &Surface,
    manifest: &Manifest,
    snapshot: &BindingSnapshot,
) -> Result<ResolvedSurface, Vec<Diagnostic>> {
    let work = || -> Checked<ResolvedSurface> {
        validate_host(manifest, snapshot)?;
        let mut resolver = Resolver {
            manifest,
            snapshot,
            owners: BTreeMap::new(),
            nodes: 0,
            bytes: 0,
            seen_selections: BTreeSet::new(),
            seen_fields: BTreeSet::new(),
        };
        resolver.collect_owners(&surface.screen, &mut Vec::new(), &[])?;
        let context = Context {
            scopes: vec![FieldScope {
                owner_path: Vec::new(),
                instance: Some(Vec::new()),
            }],
            ..Context::default()
        };
        let screen = resolver.declaration(&surface.screen, &[], &context)?;
        if snapshot
            .selections
            .keys()
            .any(|id| !resolver.seen_selections.contains(id))
            || snapshot
                .row_fields
                .keys()
                .any(|id| !resolver.seen_fields.contains(id))
        {
            return Err(host_error(
                DiagnosticKind::InvalidSnapshot,
                "Snapshot contains a selection or row Field outside the resolved surface",
            ));
        }
        Ok(ResolvedSurface {
            manifest_version: manifest.version,
            screen,
        })
    };
    work().map_err(|error| vec![error])
}
