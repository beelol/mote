//! The `moted mcp` server: the four think-lane tools over stdio as JSON-RPC. Tools are
//! `memory_search`, `memory_write`, `file_read`, `file_list`. See docs/rfcs/v0.md section 8.

use mote_context::ContextStore;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The MCP tool server.
pub struct McpServer {
    store: Arc<Mutex<ContextStore>>,
    root: PathBuf,
}

fn content(text: String, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

impl McpServer {
    pub fn new(store: Arc<Mutex<ContextStore>>, root: PathBuf) -> Self {
        Self { store, root }
    }

    fn tools_list(&self) -> Value {
        json!({ "tools": [
            { "name": "memory_search", "description": "Search memory nodes by text.",
              "inputSchema": { "type": "object", "properties": { "query": { "type": "string" },
                "limit": { "type": "integer" } }, "required": ["query"] } },
            { "name": "memory_write", "description": "Write a memory node.",
              "inputSchema": { "type": "object", "properties": { "text": { "type": "string" },
                "kind": { "type": "string" } }, "required": ["text"] } },
            { "name": "file_read", "description": "Read a file.",
              "inputSchema": { "type": "object", "properties": { "path": { "type": "string" } },
                "required": ["path"] } },
            { "name": "file_list", "description": "List a directory.",
              "inputSchema": { "type": "object", "properties": { "path": { "type": "string" } },
                "required": ["path"] } }
        ]})
    }

    fn resolve(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            p
        } else {
            self.root.join(p)
        }
    }

    fn tools_call(&self, params: &Value) -> Result<Value, Value> {
        let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(Value::Null);
        match name {
            "memory_search" => {
                let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
                let limit = args.get("limit").and_then(|l| l.as_u64()).unwrap_or(8) as usize;
                match self.store.lock().unwrap().memory_search(query, limit) {
                    Ok(hits) => Ok(content(serde_json::to_string(&hits).unwrap(), false)),
                    Err(e) => Ok(content(format!("memory_search failed: {e}"), true)),
                }
            }
            "memory_write" => {
                let text = args.get("text").and_then(|t| t.as_str()).unwrap_or("");
                let kind = args.get("kind").and_then(|k| k.as_str()).unwrap_or("event");
                match self.store.lock().unwrap().memory_write(kind, text, None) {
                    Ok(id) => Ok(content(json!({ "id": id }).to_string(), false)),
                    Err(e) => Ok(content(format!("memory_write failed: {e}"), true)),
                }
            }
            "file_read" => {
                let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("");
                match std::fs::read_to_string(self.resolve(path)) {
                    Ok(text) => Ok(content(text, false)),
                    Err(e) => Ok(content(format!("file_read failed: {e}"), true)),
                }
            }
            "file_list" => {
                let path = args.get("path").and_then(|p| p.as_str()).unwrap_or("");
                match std::fs::read_dir(self.resolve(path)) {
                    Ok(rd) => {
                        let names: Vec<String> = rd
                            .filter_map(|e| e.ok())
                            .map(|e| e.file_name().to_string_lossy().into_owned())
                            .collect();
                        Ok(content(serde_json::to_string(&names).unwrap(), false))
                    }
                    Err(e) => Ok(content(format!("file_list failed: {e}"), true)),
                }
            }
            other => Err(json!({ "code": -32601, "message": format!("unknown tool: {other}") })),
        }
    }

    /// Handle one JSON-RPC message. Returns a response, or None for a notification.
    pub fn handle(&self, req: &Value) -> Option<Value> {
        let id = req.get("id").cloned()?; // notifications carry no id and get no reply
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let result = match method {
            "initialize" => Ok(json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": { "name": "moted", "version": "0" },
                "capabilities": { "tools": {} }
            })),
            "tools/list" => Ok(self.tools_list()),
            "tools/call" => self.tools_call(req.get("params").unwrap_or(&Value::Null)),
            _ => Err(json!({ "code": -32601, "message": "method not found" })),
        };
        Some(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
        })
    }

    /// Serve JSON-RPC over the given reader/writer (stdio in production).
    pub fn serve(&self, reader: impl BufRead, mut writer: impl Write) -> std::io::Result<()> {
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let Ok(req) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(resp) = self.handle(&req) {
                writeln!(writer, "{resp}")?;
                writer.flush()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_testkit::FakeHarness;

    #[test]
    fn think_turn_calls_memory_search_through_mcp() {
        let store = Arc::new(Mutex::new(ContextStore::open_in_memory().unwrap()));
        store.lock().unwrap().memory_write("event", "the sky is blue today", None).unwrap();
        let server = McpServer::new(store, std::env::temp_dir());

        // The fake harness performs the MCP handshake then calls memory_search.
        let resp = FakeHarness::call_tool(
            |req| server.handle(&req).unwrap(),
            "memory_search",
            json!({ "query": "sky" }),
        );
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("the sky is blue today"), "memory_search returned: {text}");
        assert_eq!(resp["result"]["isError"], false);
    }

    #[test]
    fn unknown_tool_is_an_error() {
        let store = Arc::new(Mutex::new(ContextStore::open_in_memory().unwrap()));
        let server = McpServer::new(store, std::env::temp_dir());
        let resp = FakeHarness::call_tool(
            |req| server.handle(&req).unwrap(),
            "no_such_tool",
            json!({}),
        );
        assert!(resp.get("error").is_some());
    }
}
