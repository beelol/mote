use crate::{now_ms, Daemon};
use mote_gateway::{GatewayConfig, GatewayServer};
use mote_ipc::{Actor, Dispatch, RpcError};
use mote_protocol::{
    ApprovalDecision, ChatMessage, CommittedSurface, Cursor, HostSnapshot, MessageRole,
    MessageStatus, ThreadSnapshot, TypedInteraction,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

pub fn prepare_identity(data: &std::path::Path) -> Result<mote_gateway::HostIdentity, String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let identity = mote_gateway::HostIdentity::load_or_create(data).map_err(|e| e.to_string())?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(data.join("mote.db"))
        .map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.uid() != unsafe { libc::geteuid() } {
        return Err("unsafe database file".into());
    }
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    mote_gateway::DeviceStore::open(&data.join("mote.db"), identity.host_id())
        .map_err(|e| e.to_string())?;
    Ok(identity)
}

pub fn bind(daemon: Arc<Daemon>, config: GatewayConfig) -> Result<Arc<GatewayServer>, String> {
    let server =
        Arc::new(GatewayServer::bind_loopback(config, daemon.clone()).map_err(|e| e.to_string())?);
    if server.host_id() != &daemon.host_id {
        return Err("host identity changed".into());
    }
    *daemon.gateway.lock().unwrap() = Some(Arc::downgrade(&server));
    Ok(server)
}

pub fn local_control(daemon: &Daemon, method: &str, params: Value) -> Result<Value, RpcError> {
    let server = daemon
        .gateway
        .lock()
        .unwrap()
        .as_ref()
        .and_then(std::sync::Weak::upgrade)
        .ok_or_else(|| RpcError::internal("gateway unavailable"))?;
    match method {
        "gateway.pair" => Ok(
            json!({"code":server.open_pairing().map_err(|e| RpcError::internal(e.to_string()))?}),
        ),
        "gateway.pending" => Ok(
            json!({"devices":server.pending_devices().into_iter().map(|device| json!({"client_id":device.client_id,"name":device.name})).collect::<Vec<_>>()}),
        ),
        "gateway.confirm" => {
            let id = params["client_id"]
                .as_str()
                .ok_or_else(|| RpcError::invalid_params("client_id required"))?;
            let approved = params["approved"]
                .as_bool()
                .ok_or_else(|| RpcError::invalid_params("approved required"))?;
            server
                .confirm_pairing(id, approved)
                .map_err(|e| RpcError::invalid_params(e.to_string()))?;
            Ok(json!({"ok":true}))
        }
        "gateway.revoke" => {
            let key: [u8; 32] = mote_gateway::noise::unhex(
                params["public_key"]
                    .as_str()
                    .ok_or_else(|| RpcError::invalid_params("public_key required"))?,
            )
            .map_err(|_| RpcError::invalid_params("invalid key"))?
            .try_into()
            .map_err(|_| RpcError::invalid_params("invalid key length"))?;
            Ok(
                json!({"revoked":server.revoke(&key).map_err(|e| RpcError::internal(e.to_string()))?}),
            )
        }
        _ => Err(RpcError::invalid_params("unknown gateway control")),
    }
}

pub fn dispatch(
    daemon: &Daemon,
    actor: &Actor,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    let Actor::PairedDevice { host_id, device_id } = actor else {
        return daemon.call(method, params);
    };
    if host_id != &daemon.host_id {
        return Err(RpcError::new("unauthorized", "wrong host"));
    }
    match method {
        "hello" | "ping" => daemon.call(method, params),
        "turn" => {
            let params = serde_json::from_str(&mote_redact::redact(&params.to_string()))
                .map_err(|e| RpcError::internal(e.to_string()))?;
            daemon.call(method, params)
        }
        "status" => {
            let mut result = daemon.call(method, params)?;
            result.as_object_mut().unwrap().remove("socket");
            result["host_id"] = json!(daemon.host_id);
            Ok(result)
        }
        "snapshot" => {
            let seq = daemon
                .log
                .max_seq()
                .map_err(|e| RpcError::internal(e.to_string()))?;
            let mut surfaces = BTreeMap::new();
            let mut threads: BTreeMap<String, ThreadSnapshot> = BTreeMap::new();
            for event in daemon
                .log
                .recent_since(0, 5000)
                .map_err(|e| RpcError::internal(e.to_string()))?
                .into_iter()
                .rev()
            {
                if event.seq > seq {
                    continue;
                }
                if matches!(
                    event.kind.as_str(),
                    "turn.user" | "talk.delta" | "think.text"
                ) {
                    if let Some(text) = event.payload["text"].as_str() {
                        threads
                            .entry(event.scope_id.clone())
                            .or_insert_with(|| ThreadSnapshot {
                                thread: event.scope_id.clone(),
                                messages: Vec::new(),
                            })
                            .messages
                            .push(ChatMessage {
                                id: event.seq.to_string(),
                                role: if event.kind == "turn.user" {
                                    MessageRole::User
                                } else {
                                    MessageRole::Assistant
                                },
                                text: text.into(),
                                status: MessageStatus::Delivered,
                            });
                    }
                }
                if event.kind == "ui.surface" {
                    let tree = event
                        .payload
                        .get("matrix_surface")
                        .or_else(|| event.payload.get("surface"));
                    if let Some(tree) = tree.and_then(Value::as_object) {
                        surfaces.insert(
                            event.scope_id.clone(),
                            CommittedSurface {
                                thread: event.scope_id,
                                surface_seq: event.seq,
                                surface_contract: 1,
                                tree: tree.clone(),
                            },
                        );
                    }
                }
            }
            let pending_approvals = daemon
                .watchers
                .lock()
                .unwrap()
                .remote_approvals(host_id, device_id, now_ms())
                .map_err(|e| RpcError::internal(e.to_string()))?;
            let snapshot = HostSnapshot {
                host_id: host_id.clone(),
                cursor: Cursor::new(host_id.clone(), seq)
                    .map_err(|e| RpcError::internal(e.to_string()))?,
                threads: threads.into_values().collect(),
                surfaces: surfaces.into_values().collect(),
                pending_approvals,
            };
            serde_json::to_value(snapshot).map_err(|e| RpcError::internal(e.to_string()))
        }
        "interaction" => {
            let interaction: TypedInteraction = serde_json::from_value(params)
                .map_err(|_| RpcError::invalid_params("invalid typed interaction"))?;
            if interaction.host_id != *host_id
                || interaction.event.starts_with("approve:")
                || interaction.event.starts_with("deny:")
            {
                return Err(RpcError::new(
                    "unauthorized",
                    "interaction requires its host and typed action",
                ));
            }
            daemon.call("ui.event", json!({"thread":interaction.thread,"surface_seq":interaction.surface_seq,"surface_contract":interaction.surface_contract,"node_path":interaction.node_path,"event":interaction.event,"args":interaction.args}))
        }
        "approval.decision" => {
            let decision: ApprovalDecision = serde_json::from_value(params)
                .map_err(|_| RpcError::invalid_params("invalid typed approval"))?;
            let changed = daemon
                .watchers
                .lock()
                .unwrap()
                .remote_decide(host_id, device_id, &decision, now_ms())
                .map_err(|e| RpcError::internal(e.to_string()))?;
            if !changed {
                return Err(RpcError::invalid_params(
                    "approval binding stale or invalid",
                ));
            }
            daemon.log.append(&decision.thread, "approval.decided", &json!({"approval_id":decision.approval_id,"decision":decision.decision,"action_hash":decision.action_hash})).map_err(|e| RpcError::internal(e.to_string()))?;
            Ok(json!({"ok":true,"changed":true}))
        }
        _ => Err(RpcError::new("unauthorized", "method is local-only")),
    }
}
