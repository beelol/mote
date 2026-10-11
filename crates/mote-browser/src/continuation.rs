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
    pub pending: HashMap<String, Retained>,
    pub released: HashSet<fetch::RequestId>,
    pub finished: HashMap<fetch::RequestId, bool>,
}

pub(crate) struct Retained {
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
    pub async fn capture(page: &Page, event: &fetch::EventRequestPaused) -> Result<Self> {
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
    let requests: Vec<_> = state
        .lock()
        .unwrap()
        .pending
        .drain()
        .map(|(_, request)| request.id)
        .collect();
    for id in requests {
        let _ = page
            .execute(fetch::FailRequestParams::new(
                id,
                network::ErrorReason::Aborted,
            ))
            .await;
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
    let request = state
        .pending
        .get_mut(key)
        .ok_or_else(|| anyhow!("request is no longer retained"))?;
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
        if request.binding.as_ref() != Some(intent)
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
    let request = state
        .lock()
        .unwrap()
        .pending
        .remove(&intent.request_fingerprint)
        .ok_or_else(|| anyhow!("request already consumed"))?;
    if request.expires_at <= chrono::Utc::now().timestamp_millis() {
        bail!("request expired");
    }
    state.lock().unwrap().released.insert(request.id.clone());
    page.execute(fetch::ContinueRequestParams::new(request.id.clone()))
        .await?;
    for _ in 0..500 {
        let result = state.lock().unwrap().finished.remove(&request.id);
        if let Some(success) = result {
            if success {
                return Ok(BrowserResult::Released);
            }
            bail!("approved request did not return a successful response");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    state.lock().unwrap().released.remove(&request.id);
    bail!("approved request outcome is indeterminate")
}
