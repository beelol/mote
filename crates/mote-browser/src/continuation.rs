use crate::{
    worker::call_node, BrowserCommitIntent, BrowserEffect, BrowserPolicy, BrowserResult, NodeRef,
    SnapshotMap,
};
use anyhow::{anyhow, bail, Result};
use chromiumoxide::{
    cdp::browser_protocol::{accessibility, fetch, network},
    Page,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Default)]
pub(crate) struct Boundary {
    pub generation: u64,
    pub pending: HashMap<String, Retained>,
    pub released: HashSet<fetch::RequestId>,
    pub finished: HashMap<fetch::RequestId, bool>,
}

impl Boundary {
    pub fn retain(&mut self, request: Retained) -> bool {
        if request.generation != self.generation || self.pending.len() >= 16 {
            return false;
        }
        self.pending.insert(request.fingerprint.clone(), request);
        true
    }

    fn invalidate(&mut self) -> Vec<fetch::RequestId> {
        self.generation += 1;
        let mut requests: HashSet<_> = self
            .pending
            .drain()
            .map(|(_, request)| request.id)
            .collect();
        requests.extend(self.released.drain());
        requests.into_iter().collect()
    }

    fn begin_release(&mut self, key: &str, now: i64) -> Result<(fetch::RequestId, u64)> {
        let request = self
            .pending
            .get(key)
            .ok_or_else(|| anyhow!("request already consumed"))?;
        if request.generation != self.generation
            || request.expires_at <= now
            || self.released.contains(&request.id)
        {
            bail!("request expired or document changed");
        }
        self.released.insert(request.id.clone());
        Ok((request.id.clone(), request.generation))
    }
}

pub(crate) struct Retained {
    generation: u64,
    id: fetch::RequestId,
    pub fingerprint: String,
    request_url: String,
    method: String,
    body: Option<String>,
    source_url: String,
    expires_at: i64,
    binding: Option<BrowserCommitIntent>,
    node_description: Option<String>,
}

impl Retained {
    pub async fn capture(
        page: &Page,
        event: &fetch::EventRequestPaused,
        generation: u64,
    ) -> Result<Self> {
        let body = if event.request.has_post_data == Some(true) {
            let id = event
                .network_id
                .clone()
                .ok_or_else(|| anyhow!("request body unavailable"))?;
            Some(
                page.execute(network::GetRequestPostDataParams::new(id))
                    .await?
                    .result
                    .post_data,
            )
        } else {
            None
        };
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                event.request_id.clone(),
                &event.request,
                &body
            ))?)
        );
        Ok(Self {
            generation,
            id: event.request_id.clone(),
            fingerprint,
            request_url: event.request.url.clone(),
            method: event.request.method.clone(),
            body,
            source_url: page
                .url()
                .await?
                .ok_or_else(|| anyhow!("page has no URL"))?,
            expires_at: chrono::Utc::now().timestamp_millis() + 120_000,
            binding: None,
            node_description: None,
        })
    }
}

pub(crate) async fn cancel(page: &Page, state: &Arc<Mutex<Boundary>>, key: &str) {
    let request = state.lock().unwrap().pending.remove(key);
    if let Some(request) = request {
        let _ = page
            .execute(fetch::FailRequestParams::new(
                request.id,
                network::ErrorReason::Aborted,
            ))
            .await;
    }
}

pub(crate) async fn cancel_all(page: &Page, state: &Arc<Mutex<Boundary>>) {
    let requests = state.lock().unwrap().invalidate();
    for id in requests {
        let _ = page
            .execute(fetch::FailRequestParams::new(
                id,
                network::ErrorReason::Aborted,
            ))
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_during_capture_rejects_late_request_insertion() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                for navigation in ["full_document", "same_document"] {
                    let state = Arc::new(Mutex::new(Boundary::default()));
                    let generation = state.lock().unwrap().generation;
                    let (resume, suspended) = tokio::sync::oneshot::channel();
                    let captured_state = state.clone();
                    let capture = tokio::spawn(async move {
                        suspended.await.unwrap();
                        let request = Retained {
                            generation,
                            id: fetch::RequestId::new("old"),
                            fingerprint: "old-request".into(),
                            request_url: "http://127.0.0.1/effect".into(),
                            method: "POST".into(),
                            body: Some("old-body".into()),
                            source_url: "http://127.0.0.1/new-page".into(),
                            expires_at: i64::MAX,
                            binding: None,
                            node_description: None,
                        };
                        captured_state.lock().unwrap().retain(request)
                    });
                    state.lock().unwrap().invalidate();
                    resume.send(()).unwrap();
                    assert!(
                        !capture.await.unwrap(),
                        "{navigation} allowed a late capture"
                    );
                    assert!(state.lock().unwrap().pending.is_empty());
                }
            });
    }

    #[test]
    fn navigation_keeps_consumed_request_cancellable_before_continuation() {
        let mut state = Boundary::default();
        let id = fetch::RequestId::new("exact-request");
        state.pending.insert(
            "key".into(),
            Retained {
                generation: 0,
                id: id.clone(),
                fingerprint: "key".into(),
                request_url: String::new(),
                method: "POST".into(),
                body: None,
                source_url: String::new(),
                expires_at: i64::MAX,
                binding: None,
                node_description: None,
            },
        );
        state.begin_release("key", 0).unwrap();
        assert!(state.begin_release("key", 0).is_err());
        assert_eq!(state.invalidate(), vec![id]);
        assert!(state.pending.is_empty());
        assert!(state.released.is_empty());
    }
}

async fn page_hash(page: &Page, url: &str) -> Result<String> {
    let ax = page
        .execute(accessibility::GetFullAxTreeParams::default())
        .await?
        .result;
    Ok(format!(
        "{:x}",
        Sha256::digest([url.as_bytes(), &serde_json::to_vec(&ax)?].concat())
    ))
}

const DESCRIBE: &str = "function(){if(!this.isConnected||this.ownerDocument!==document)throw Error('detached node');if(this instanceof HTMLAnchorElement)return JSON.stringify({url:this.href,method:'GET',body:null});const form=this.form;if(!form||!(this instanceof HTMLButtonElement||this instanceof HTMLInputElement)||this.type!=='submit'||form.enctype!=='application/x-www-form-urlencoded')throw Error('only native URL-encoded forms and links are supported');const fields=[];for(const field of form.elements){if(!field.name||field.disabled)continue;if(field.type==='file')throw Error('file upload unsupported');if(['submit','button','reset','image'].includes(field.type)&&field!==this)continue;if(['checkbox','radio'].includes(field.type)&&!field.checked)continue;if(field instanceof HTMLSelectElement){for(const option of field.selectedOptions)fields.push([field.name,option.value]);}else fields.push([field.name,field.value]);}let url=new URL(this.hasAttribute('formaction')?this.formAction:form.action);const method=(this.hasAttribute('formmethod')?this.formMethod:form.method).toUpperCase();const body=new URLSearchParams(fields).toString();if(method==='GET')url.search=body;else if(method!=='POST')throw Error('unsupported form method');return JSON.stringify({url:url.href,method,body:method==='GET'?null:body})}";

#[derive(Deserialize)]
struct NativeRequest {
    url: String,
    method: String,
    body: Option<String>,
}

pub(crate) async fn prepare(
    page: &Page,
    policy: &BrowserPolicy,
    refs: &Arc<Mutex<SnapshotMap>>,
    state: &Arc<Mutex<Boundary>>,
    reference: NodeRef,
) -> Result<BrowserResult> {
    let backend = refs.lock().unwrap().resolve(&reference)?;
    let description = call_node(page, backend, DESCRIBE).await?;
    let native: NativeRequest = serde_json::from_str(&description)?;
    if policy.request(&native.url, &native.method) != BrowserEffect::Commit {
        bail!("native action must name an approved protected request");
    }
    let old: HashSet<_> = state.lock().unwrap().pending.keys().cloned().collect();
    let script = format!("function(){{fetch({},{{method:{},body:{},credentials:'omit',redirect:'manual',mode:'same-origin',headers:{{'Content-Type':'application/x-www-form-urlencoded'}}}}).catch(()=>{{}});return 'prepared'}}", serde_json::to_string(&native.url)?, serde_json::to_string(&native.method)?, serde_json::to_string(&native.body)?);
    call_node(page, backend, &script).await?;
    for _ in 0..100 {
        let key = state
            .lock()
            .unwrap()
            .pending
            .iter()
            .find(|(key, request)| {
                !old.contains(*key)
                    && request.request_url == native.url
                    && request.method == native.method
                    && request.body == native.body
            })
            .map(|(key, _)| key.clone());
        if let Some(key) = key {
            return bind(page, refs, state, &key, reference, Some(description))
                .await
                .map(|intent| BrowserResult::CommitRequired { intent });
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    bail!("native request was not intercepted")
}

async fn bind(
    page: &Page,
    refs: &Arc<Mutex<SnapshotMap>>,
    state: &Arc<Mutex<Boundary>>,
    key: &str,
    reference: NodeRef,
    description: Option<String>,
) -> Result<BrowserCommitIntent> {
    refs.lock().unwrap().resolve(&reference)?;
    let url = page
        .url()
        .await?
        .ok_or_else(|| anyhow!("page has no URL"))?;
    let hash = page_hash(page, &url).await?;
    let mut state = state.lock().unwrap();
    let generation = state.generation;
    let request = state
        .pending
        .get_mut(key)
        .ok_or_else(|| anyhow!("request is no longer retained"))?;
    if request.generation != generation {
        bail!("request document changed");
    }
    if let Some(intent) = &request.binding {
        return Ok(intent.clone());
    }
    if request.source_url != url || request.expires_at <= chrono::Utc::now().timestamp_millis() {
        bail!("request expired or page changed");
    }
    let intent = BrowserCommitIntent {
        origin: url::Url::parse(&url)?.origin().ascii_serialization(),
        url,
        node_ref: reference,
        request_fingerprint: request.fingerprint.clone(),
        page_state_hash: hash,
        request_url: request.request_url.clone(),
        method: request.method.clone(),
        expires_at: request.expires_at,
    };
    request.binding = Some(intent.clone());
    request.node_description = description;
    Ok(intent)
}

pub(crate) async fn pending(
    page: &Page,
    refs: &Arc<Mutex<SnapshotMap>>,
    state: &Arc<Mutex<Boundary>>,
) -> Result<BrowserResult> {
    let keys: Vec<_> = state.lock().unwrap().pending.keys().cloned().collect();
    if keys.is_empty() {
        return Ok(BrowserResult::Pending(Vec::new()));
    }
    let ax = page
        .execute(accessibility::GetFullAxTreeParams::default())
        .await?
        .result;
    let backend = ax
        .nodes
        .iter()
        .find(|node| !node.ignored && node.backend_dom_node_id.is_some())
        .and_then(|node| node.backend_dom_node_id)
        .ok_or_else(|| anyhow!("current page has no accessible root"))?;
    let reference = refs.lock().unwrap().reference(*backend.inner());
    let mut intents = Vec::new();
    for key in keys {
        match bind(page, refs, state, &key, reference.clone(), None).await {
            Ok(intent) => intents.push(intent),
            Err(_) => cancel(page, state, &key).await,
        }
    }
    Ok(BrowserResult::Pending(intents))
}

pub(crate) async fn inspect(
    page: &Page,
    refs: &Arc<Mutex<SnapshotMap>>,
    state: &Arc<Mutex<Boundary>>,
    intent: &BrowserCommitIntent,
) -> Result<()> {
    let description = {
        let state = state.lock().unwrap();
        let request = state
            .pending
            .get(&intent.request_fingerprint)
            .ok_or_else(|| anyhow!("request is no longer retained"))?;
        if request.generation != state.generation
            || request.binding.as_ref() != Some(intent)
            || request.expires_at <= chrono::Utc::now().timestamp_millis()
        {
            bail!("request binding expired or changed");
        }
        request.node_description.clone()
    };
    let backend = refs.lock().unwrap().resolve(&intent.node_ref)?;
    let url = page
        .url()
        .await?
        .ok_or_else(|| anyhow!("page has no URL"))?;
    if url != intent.url || page_hash(page, &url).await? != intent.page_state_hash {
        bail!("page state changed");
    }
    let live = if description.is_some() {
        call_node(page, backend, DESCRIBE).await?
    } else {
        call_node(page, backend, "function(){if(!this.isConnected||this.ownerDocument!==document)throw Error('detached node');return 'current'}").await?
    };
    if description.is_some_and(|description| description != live) {
        bail!("native request changed");
    }
    Ok(())
}

pub(crate) async fn release(
    page: &Page,
    refs: &Arc<Mutex<SnapshotMap>>,
    state: &Arc<Mutex<Boundary>>,
    intent: BrowserCommitIntent,
) -> Result<BrowserResult> {
    inspect(page, refs, state, &intent).await?;
    let reserved = state.lock().unwrap().begin_release(
        &intent.request_fingerprint,
        chrono::Utc::now().timestamp_millis(),
    );
    let (id, generation) = match reserved {
        Ok(value) => value,
        Err(error) => {
            cancel(page, state, &intent.request_fingerprint).await;
            return Err(error);
        }
    };
    let continued = page
        .execute(fetch::ContinueRequestParams::new(id.clone()))
        .await;
    let current = {
        let mut state = state.lock().unwrap();
        state.pending.remove(&intent.request_fingerprint);
        state.generation == generation
    };
    if !current || continued.is_err() {
        state.lock().unwrap().released.remove(&id);
        let _ = page
            .execute(fetch::FailRequestParams::new(
                id.clone(),
                network::ErrorReason::Aborted,
            ))
            .await;
        bail!("approved request invalidated before continuation completed");
    }
    for _ in 0..500 {
        let result = state.lock().unwrap().finished.remove(&id);
        if let Some(success) = result {
            if success {
                return Ok(BrowserResult::Released);
            }
            bail!("approved request did not return a successful response");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    state.lock().unwrap().released.remove(&id);
    bail!("approved request outcome is indeterminate")
}
