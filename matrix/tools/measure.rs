use matrix_lang::manifest::{BindingDefinition, BindingSnapshot, Manifest};
use matrix_lang::types::{DisplayFormat, Type};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;

#[allow(dead_code)]
#[path = "../../moted/src/matrix.rs"]
mod matrix;
#[allow(dead_code)]
#[path = "../../moted/src/pipeline.rs"]
mod pipeline;

fn main() -> Result<(), Box<dyn Error>> {
    let tokenizer = tiktoken_rs::o200k_base()?;
    let examples = fs::read_to_string("matrix/examples.md")?;
    let suite = examples
        .split("## Conformance suite")
        .nth(1)
        .ok_or("missing conformance suite")?
        .split("## Real world:")
        .next()
        .ok_or("missing suite end")?;
    let mut cases = Vec::new();
    for heading in suite.split("### ").skip(1) {
        let number: usize = heading
            .split('.')
            .next()
            .ok_or("missing case number")?
            .parse()?;
        let fenced = heading
            .split("```\n")
            .nth(1)
            .ok_or("missing source fence")?;
        let source = fenced.split("```").next().ok_or("missing fence end")?;
        cases.push((number, source.to_string()));
    }
    if cases.len() != 10 {
        return Err("expected ten design cases".into());
    }
    let mut bytes = 0;
    let mut tokens = 0;
    println!("design_case\tmatrix_bytes\tmatrix_tokens\tcompact_json_bytes\tjson_tokens");
    for (number, source) in cases {
        let source_tokens = tokenizer.encode_ordinary(&source).len();
        bytes += source.len();
        tokens += source_tokens;
        if number == 6 {
            let json: serde_json::Value =
                serde_json::from_str(&fs::read_to_string("crates/mote-ui/fixtures/04_list.json")?)?;
            let compact = serde_json::to_string(&json)?;
            println!(
                "{number}\t{}\t{source_tokens}\t{}\t{}",
                source.len(),
                compact.len(),
                tokenizer.encode_ordinary(&compact).len()
            );
        } else {
            println!("{number}\t{}\t{source_tokens}\tnone\tnone", source.len());
        }
    }
    println!("design_total\t{bytes}\t{tokens}\tpartial\tpartial");
    println!("v0_fixture\tmatrix_bytes\tmatrix_tokens\tcompact_json_bytes\tjson_tokens");
    let mut files = fs::read_dir("matrix/conformance/v0")?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    files.retain(|path| path.extension().and_then(|value| value.to_str()) == Some("matrix"));
    files.sort();
    if files.len() != 9 {
        return Err("expected nine exact v0 snapshot fragments".into());
    }
    let mut totals = [0; 4];
    for file in files {
        let stem = file
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("invalid fixture name")?;
        let source = fs::read_to_string(&file)?;
        let json_path = Path::new("crates/mote-ui/fixtures").join(format!("{stem}.json"));
        let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(json_path)?)?;
        let compact = serde_json::to_string(&json)?;
        let counts = [
            source.len(),
            tokenizer.encode_ordinary(&source).len(),
            compact.len(),
            tokenizer.encode_ordinary(&compact).len(),
        ];
        for (total, count) in totals.iter_mut().zip(counts) {
            *total += count;
        }
        println!(
            "{stem}\t{}\t{}\t{}\t{}",
            counts[0], counts[1], counts[2], counts[3]
        );
    }
    println!(
        "v0_total\t{}\t{}\t{}\t{}",
        totals[0], totals[1], totals[2], totals[3]
    );
    measure_host_and_transport(&tokenizer)?;
    Ok(())
}

fn type_encoding(ty: &Type) -> Value {
    match ty {
        Type::String => json!({"kind": "string"}),
        Type::Number => json!({"kind": "number"}),
        Type::Integer => json!({"kind": "integer"}),
        Type::Boolean => json!({"kind": "boolean"}),
        Type::Url => json!({"kind": "url"}),
        Type::Asset => json!({"kind": "asset"}),
        Type::Date => json!({"kind": "date"}),
        Type::Time => json!({"kind": "time"}),
        Type::DateTime => json!({"kind": "datetime"}),
        Type::Enum(name) => json!({"kind": "enum", "name": name}),
        Type::Record(name) => json!({"kind": "record", "name": name}),
        Type::List(item) => json!({"kind": "list", "item": type_encoding(item)}),
        Type::Option(item) => json!({"kind": "option", "item": type_encoding(item)}),
    }
}

fn binding_encoding(binding: &BindingDefinition) -> Value {
    let display = match &binding.display {
        DisplayFormat::Canonical => json!({"kind": "canonical"}),
        DisplayFormat::Fixed { decimals } => json!({"kind": "fixed", "decimals": decimals}),
        DisplayFormat::Money { symbol, decimals } => {
            json!({"kind": "money", "symbol": symbol, "decimals": decimals})
        }
    };
    json!({"ty": type_encoding(&binding.ty), "display": display})
}

fn manifest_encoding(manifest: &Manifest) -> Value {
    let records: BTreeMap<_, _> = manifest
        .records
        .iter()
        .map(|(name, record)| {
            let fields: BTreeMap<_, _> = record
                .fields
                .iter()
                .map(|(name, binding)| (name, binding_encoding(binding)))
                .collect();
            (name, json!({"fields": fields}))
        })
        .collect();
    let enums: BTreeMap<_, _> = manifest
        .enums
        .iter()
        .map(|(name, definition)| {
            (
                name,
                json!({"members": definition.members, "labels": definition.labels}),
            )
        })
        .collect();
    let values: BTreeMap<_, _> = manifest
        .values
        .iter()
        .map(|(name, binding)| (name, binding_encoding(binding)))
        .collect();
    let fields: BTreeMap<_, _> = manifest
        .fields
        .iter()
        .map(|(name, field)| {
            (
                name,
                json!({"binding": binding_encoding(&field.binding), "initial": field.initial}),
            )
        })
        .collect();
    let actions: BTreeMap<_, _> = manifest
        .actions
        .iter()
        .map(|(name, action)| {
            let parameters: BTreeMap<_, _> = action
                .parameters
                .iter()
                .map(|(name, ty)| (name, type_encoding(ty)))
                .collect();
            (name, json!({"parameters": parameters}))
        })
        .collect();
    json!({"version": manifest.version, "records": records, "enums": enums,
        "values": values, "fields": fields, "actions": actions})
}

fn snapshot_encoding(snapshot: &BindingSnapshot) -> Result<Value, Box<dyn Error>> {
    if !snapshot.row_fields.is_empty() || !snapshot.selections.is_empty() {
        return Err("dashboard measurement expects no row fields or selections".into());
    }
    Ok(json!({"manifest_version": snapshot.manifest_version,
        "values": snapshot.values, "fields": snapshot.fields,
        "row_fields": {}, "selections": {}}))
}

fn measure_host_and_transport(tokenizer: &tiktoken_rs::CoreBPE) -> Result<(), Box<dyn Error>> {
    let config = BTreeMap::from([
        ("models.local_only".into(), json!(true)),
        ("models.talk".into(), json!("qwen3:4b")),
        ("models.router".into(), json!("qwen3:0.6b")),
        ("models.think".into(), json!("qwen3-coder:30b-64k")),
        ("budgets.talk_first_token".into(), json!(300)),
        ("budgets.router".into(), json!(150)),
        ("budgets.think_first_token".into(), json!(3000)),
        ("packs.talk".into(), json!(600)),
        ("packs.think".into(), json!(8000)),
    ]);
    let log = mote_eventlog::EventLog::open_in_memory()?;
    let store = mote_context::ContextStore::open_in_memory()?;
    let pipeline = pipeline::build_snapshot(&log, &store, config, 0)?;
    let bindings = matrix::host_bindings(&pipeline.config, &pipeline);
    let source = fs::read_to_string("matrix/examples/mote-dashboard.matrix")?;
    let surface = matrix::compile(&source, &bindings)
        .map_err(|errors| format!("dashboard compile failed: {errors:?}"))?;
    let node = mote_ui::validate(&serde_json::to_vec(&surface)?)
        .map_err(|error| format!("dashboard validation failed: {}", error.0))?;
    let surface = serde_json::to_value(node)?;
    let mut event = log.append(
        "main",
        "ui.surface",
        &json!({"surface": surface, "surface_id": "matrix"}),
    )?;
    event.ts = 0;
    if event.seq != 1 {
        return Err("dashboard measurement expects the first committed event".into());
    }
    println!("host_and_transport\tencoding\tbytes\ttokens");
    for (name, encoding, text) in [
        ("dashboard_source", "matrix_source", source.clone()),
        (
            "dashboard_consumer_json",
            "compact_consumer_json",
            serde_json::to_string(&surface)?,
        ),
        (
            "host_manifest",
            "canonical_measurement_json",
            serde_json::to_string(&manifest_encoding(&bindings.manifest))?,
        ),
        (
            "binding_snapshot",
            "canonical_measurement_json",
            serde_json::to_string(&snapshot_encoding(&bindings.snapshot)?)?,
        ),
        (
            "matrix_render_request",
            "ipc_json_line",
            format!(
                "{}\n",
                json!({"id": 1, "method": "matrix.render", "params": {"thread": "main", "source": source}})
            ),
        ),
        (
            "matrix_render_response",
            "ipc_json_line",
            format!(
                "{}\n",
                json!({"id": 1, "result": {"ok": true, "valid": true}})
            ),
        ),
        (
            "ui_surface_event",
            "ipc_json_line",
            format!("{}\n", json!({"event": event})),
        ),
    ] {
        println!(
            "{name}\t{encoding}\t{}\t{}",
            text.len(),
            tokenizer.encode_ordinary(&text).len()
        );
    }
    Ok(())
}
