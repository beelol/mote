//! Benchmarks: `moted bench talk|router|memory`. Each runs 20 iterations against the live local
//! models, reports p50 and p95, and appends one JSON line to `<data dir>/bench.jsonl`. See
//! docs/rfcs/v0.md section 17. The latency evidence for V0-21, V0-26 and V0-36 comes from here.

use crate::lanes;
use mote_context::ContextStore;
use mote_localmodel::Ollama;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::time::Instant;

/// Default run count.
pub const RUNS: usize = 20;

/// A benchmark result: p50/p95 of the primary metric, plus any extra metrics.
#[derive(Debug, Clone)]
pub struct BenchResult {
    pub name: String,
    pub runs: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub extra: Value,
}

impl BenchResult {
    pub fn to_line(&self) -> Value {
        json!({
            "name": self.name,
            "runs": self.runs,
            "p50_ms": self.p50_ms,
            "p95_ms": self.p95_ms,
            "extra": self.extra,
        })
    }
}

fn percentile(samples: &[f64], p: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rank = (p / 100.0) * (s.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        s[lo]
    } else {
        s[lo] + (rank - lo as f64) * (s[hi] - s[lo])
    }
}

/// Append one benchmark line to `bench.jsonl`.
pub fn append_bench_line(path: &Path, result: &BenchResult) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", result.to_line())
}

/// Benchmark talk-lane first-token latency.
pub fn bench_talk(client: &Ollama, model: &str, num_ctx: u32, runs: usize) -> BenchResult {
    let mut firsts = Vec::new();
    for _ in 0..runs {
        let messages = json!([
            { "role": "system", "content": "Reply in one short sentence." },
            { "role": "user", "content": "say hello" }
        ]);
        let opts = mote_localmodel::ChatOptions::new(num_ctx).no_think();
        let start = Instant::now();
        let mut first: Option<f64> = None;
        let _ = client.chat_stream(model, messages, &opts, |chunk| {
            if first.is_none() && !chunk.content.is_empty() {
                first = Some(start.elapsed().as_secs_f64() * 1000.0);
            }
        });
        if let Some(ms) = first {
            firsts.push(ms);
        }
    }
    BenchResult {
        name: "talk".into(),
        runs: firsts.len(),
        p50_ms: percentile(&firsts, 50.0),
        p95_ms: percentile(&firsts, 95.0),
        extra: json!({ "metric": "first_token_ms", "model": model }),
    }
}

/// Benchmark router latency (full structured verdict).
pub fn bench_router(client: &Ollama, model: &str, num_ctx: u32, runs: usize) -> BenchResult {
    let mut times = Vec::new();
    for _ in 0..runs {
        let start = Instant::now();
        let _ = lanes::route(client, model, num_ctx, "hey there");
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    BenchResult {
        name: "router".into(),
        runs: times.len(),
        p50_ms: percentile(&times, 50.0),
        p95_ms: percentile(&times, 95.0),
        extra: json!({ "metric": "verdict_ms", "model": model }),
    }
}

/// Benchmark memory write and retrieve latency.
pub fn bench_memory(store: &ContextStore, runs: usize) -> BenchResult {
    let mut writes = Vec::new();
    let mut retrieves = Vec::new();
    for i in 0..runs {
        let start = Instant::now();
        store.memory_write("event", &format!("benchmark memory node number {i}"), None).ok();
        writes.push(start.elapsed().as_secs_f64() * 1000.0);

        let start = Instant::now();
        let _ = store.memory_search("benchmark", 8);
        retrieves.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    BenchResult {
        name: "memory".into(),
        runs,
        p50_ms: percentile(&retrieves, 50.0),
        p95_ms: percentile(&retrieves, 95.0),
        extra: json!({
            "metric": "retrieve_ms",
            "write_p50_ms": percentile(&writes, 50.0),
            "write_p95_ms": percentile(&writes, 95.0),
        }),
    }
}

/// Run a named benchmark with live dependencies and append the result line.
pub fn run_named(name: &str, data_dir: &Path, client: &Ollama, config: &crate::config::ConfigStore) {
    let bench_path = data_dir.join("bench.jsonl");
    let result = match name {
        "talk" => {
            let model = config.get_json("models.talk").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            bench_talk(client, &model, 32768, RUNS)
        }
        "router" => {
            let model = config.get_json("models.router").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            bench_router(client, &model, 32768, RUNS)
        }
        "memory" => {
            let store = ContextStore::open(&mote_paths::db_path()).expect("open store");
            bench_memory(&store, RUNS)
        }
        other => {
            eprintln!("moted bench: unknown benchmark '{other}' (talk|router|memory)");
            return;
        }
    };
    let _ = append_bench_line(&bench_path, &result);
    println!(
        "bench {}: p50 {:.1} ms, p95 {:.1} ms over {} runs",
        result.name, result.p50_ms, result.p95_ms, result.runs
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_testkit::ScriptedServer;

    #[test]
    fn three_benches_each_write_one_line() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(vec![
            json!({"message":{"role":"assistant","content":"hello"},"done":false}).to_string(),
            json!({"message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"hi\"}"},"done":true}).to_string(),
        ]);
        let client = Ollama::new(srv.base_url());
        let store = ContextStore::open_in_memory().unwrap();

        let dir = std::env::temp_dir().join(format!("mote-bench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bench.jsonl");

        append_bench_line(&path, &bench_talk(&client, "qwen3:4b", 32768, 3)).unwrap();
        append_bench_line(&path, &bench_router(&client, "qwen3:0.6b", 32768, 3)).unwrap();
        append_bench_line(&path, &bench_memory(&store, 3)).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 3, "one line per bench");
        let names: Vec<String> = lines
            .iter()
            .map(|l| serde_json::from_str::<Value>(l).unwrap()["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["talk", "router", "memory"]);
        for l in &lines {
            let v: Value = serde_json::from_str(l).unwrap();
            assert!(v["p50_ms"].is_number());
            assert!(v["p95_ms"].is_number());
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
