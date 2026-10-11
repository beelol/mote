use mote_ui::{v1, Node};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum Surface {
    V0(Node),
    V1 {
        surface: v1::Surface,
        fallback: Node,
    },
    Gallery {
        legacy: Node,
        matrix: v1::Surface,
    },
}

impl Surface {
    pub fn display_v0(&self) -> &Node {
        match self {
            Self::V0(node)
            | Self::V1 { fallback: node, .. }
            | Self::Gallery { legacy: node, .. } => node,
        }
    }

    pub fn matrix_v1(&self) -> Option<&v1::Surface> {
        match self {
            Self::V0(_) => None,
            Self::V1 { surface, .. } => Some(surface),
            Self::Gallery { matrix, .. } => Some(matrix),
        }
    }
}

pub fn decode(payload: &Value) -> Option<Surface> {
    let fallback_bytes = serde_json::to_vec(payload.get("surface")?).ok()?;
    let fallback = mote_ui::validate(&fallback_bytes).ok()?;
    if payload.get("matrix_surface").is_none()
        || payload.get("surface_id").and_then(Value::as_str) != Some("matrix")
    {
        return Some(Surface::V0(fallback));
    }

    let envelope_bytes = serde_json::to_vec(payload).ok()?;
    match v1::validate_envelope(&envelope_bytes) {
        Ok(envelope) => Some(Surface::V1 {
            surface: envelope.matrix_surface,
            fallback: envelope.surface,
        }),
        Err(_) => Some(Surface::V0(fallback)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fallback() -> serde_json::Value {
        serde_json::json!({"type":"text", "value":"Fallback dashboard"})
    }

    fn matrix_surface() -> serde_json::Value {
        serde_json::json!({
            "version": 1,
            "root": {
                "type": "screen",
                "children": [{"type":"text", "value":"Matrix dashboard"}]
            }
        })
    }

    fn dual_payload() -> serde_json::Value {
        serde_json::json!({
            "surface_id": "matrix",
            "surface": fallback(),
            "matrix_surface": matrix_surface()
        })
    }

    #[test]
    fn valid_dual_payload_retains_both_independently_valid_contracts() {
        let payload = dual_payload();
        let legacy = mote_ui::validate(&serde_json::to_vec(&payload["surface"]).unwrap()).unwrap();
        let matrix =
            mote_ui::v1::validate(&serde_json::to_vec(&payload["matrix_surface"]).unwrap())
                .unwrap();

        let decoded = decode(&payload).unwrap();

        assert_eq!(decoded.display_v0(), &legacy);
        assert_eq!(decoded.matrix_v1(), Some(&matrix));
        assert!(matches!(decoded, Surface::V1 { .. }));
    }

    #[test]
    fn missing_unknown_or_malformed_matrix_surface_uses_valid_fallback() {
        let expected = mote_ui::validate(&serde_json::to_vec(&fallback()).unwrap()).unwrap();
        let payloads = [
            serde_json::json!({"surface_id":"matrix", "surface":fallback()}),
            serde_json::json!({
                "surface_id":"matrix",
                "surface":fallback(),
                "matrix_surface":{"version":2, "root":{"type":"screen","children":[]}}
            }),
            serde_json::json!({
                "surface_id":"matrix",
                "surface":fallback(),
                "matrix_surface":{"version":1, "root":{"type":"unknown"}}
            }),
            serde_json::json!({
                "surface_id":"other",
                "surface":fallback(),
                "matrix_surface":matrix_surface()
            }),
        ];

        for payload in payloads {
            let decoded = decode(&payload).unwrap();
            assert_eq!(decoded, Surface::V0(expected.clone()));
        }
    }

    #[test]
    fn invalid_fallback_rejects_even_a_valid_matrix_surface() {
        let payload = serde_json::json!({
            "surface_id":"matrix",
            "surface":{"type":"unknown"},
            "matrix_surface":matrix_surface()
        });

        assert!(decode(&payload).is_none());
    }
}
