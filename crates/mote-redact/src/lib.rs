//! Best-effort secret redaction run on every event before storage.
//!
//! Patterns and secret keys are carried over from Overseer's `redact.rs` unchanged, so the
//! same tokens are masked in both daemons.

use regex::Regex;
use std::sync::OnceLock;

fn patterns() -> &'static [Regex] {
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| {
        [
            r"sk-[A-Za-z0-9_\-]{16,}",
            r"sk-ant-[A-Za-z0-9_\-]{16,}",
            r"eyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}",
            r"(?i)bearer\s+[A-Za-z0-9._\-]{16,}",
            r#"(?i)"(access_token|refresh_token|id_token|api_key|apikey|password|secret)"\s*:\s*"[^"]*""#,
            r"gh[pousr]_[A-Za-z0-9]{20,}",
            r"xox[abprs]-[A-Za-z0-9\-]{10,}",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

/// Replace every known secret pattern in `text` with `[redacted]`. Keyed secrets keep their
/// key name and mask only the value.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for re in patterns() {
        if re.is_match(&out) {
            out = re
                .replace_all(&out, |caps: &regex::Captures| {
                    if let Some(key) = caps.get(1) {
                        format!("\"{}\": \"[redacted]\"", key.as_str())
                    } else {
                        "[redacted]".to_string()
                    }
                })
                .to_string();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn redacts_common_secrets() {
        assert_eq!(redact("key sk-abcdefghijklmnopqrstuv end"), "key [redacted] end");
        assert_eq!(redact(r#"{"access_token": "abc.def"}"#), r#"{"access_token": "[redacted]"}"#);
        assert!(redact("Authorization: Bearer abcdefghijklmnopqrstuvwxyz").contains("[redacted]"));
        assert_eq!(redact("hello world"), "hello world");
    }

    #[test]
    fn redacts_github_and_slack_tokens() {
        assert_eq!(redact("ghp_0123456789abcdefghijklmnopqrstuvwx"), "[redacted]");
        assert!(redact("token xoxb-1234567890-abcdefghij").contains("[redacted]"));
    }

    #[test]
    fn redacts_jwt() {
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghij";
        assert_eq!(redact(jwt), "[redacted]");
    }
}
