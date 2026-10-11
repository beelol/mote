use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BrowserEvidence {
    pub entries: Vec<HarEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarEntry {
    pub started_at: String,
    pub response_code: Option<i64>,
    pub url: String,
    pub method: String,
    pub headers: Value,
    pub body: Option<String>,
    pub status: String,
}

impl BrowserEvidence {
    pub fn record(
        &mut self,
        url: &str,
        method: &str,
        mut headers: Value,
        body: Option<&str>,
        status: &str,
    ) {
        if let Some(headers) = headers.as_object_mut() {
            for (key, value) in headers.iter_mut() {
                *value = if matches!(
                    key.to_ascii_lowercase().as_str(),
                    "cookie" | "set-cookie" | "authorization" | "proxy-authorization"
                ) {
                    json!("[redacted]")
                } else {
                    json!(mote_redact::redact(
                        value.as_str().unwrap_or("[non-text header]")
                    ))
                };
            }
        }
        let url = url::Url::parse(url)
            .map(|mut url| {
                url.set_query(None);
                url.set_fragment(None);
                url.set_username("").ok();
                url.set_password(None).ok();
                url.to_string()
            })
            .unwrap_or_else(|_| "[invalid URL]".into());
        self.entries.push(HarEntry {
            started_at: chrono::Utc::now().to_rfc3339(),
            response_code: None,
            url: mote_redact::redact(&url),
            method: method.into(),
            headers,
            body: body.map(|_| "[redacted]".into()),
            status: status.into(),
        });
    }

    pub fn har(&self) -> Value {
        let entries = self.entries.iter().map(|entry| {
            let headers = entry.headers.as_object().map(|headers| headers.iter().map(|(name, value)| json!({"name":name,"value":value.as_str().unwrap_or("[redacted]")})).collect::<Vec<_>>()).unwrap_or_default();
            let mut request = json!({"method":entry.method,"url":entry.url,"httpVersion":"HTTP/1.1","headers":headers,"cookies":[],"queryString":[],"headersSize":-1,"bodySize":-1});
            if let Some(body) = &entry.body { request["postData"] = json!({"mimeType":"application/octet-stream","text":body}); }
            json!({"startedDateTime":entry.started_at,"time":0,"request":request,"response":{"status":entry.response_code.unwrap_or(0),"statusText":"","httpVersion":"HTTP/1.1","headers":[],"cookies":[],"content":{"size":0,"mimeType":"application/octet-stream"},"redirectURL":"","headersSize":-1,"bodySize":-1},"cache":{},"timings":{"send":0,"wait":0,"receive":0},"_moteEffect":entry.status})
        }).collect::<Vec<_>>();
        json!({"log":{"version":"1.2","creator":{"name":"mote-browser","version":"0"},"entries":entries}})
    }
}
