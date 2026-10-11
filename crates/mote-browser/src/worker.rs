use crate::continuation::{Boundary, Retained};
use crate::{
    BrowserEffect, BrowserEvidence, BrowserNode, BrowserPolicy, BrowserSnapshot, LaunchConfig,
    NodeRef, SnapshotMap,
};
use anyhow::{anyhow, bail, Result};
use base64::Engine;
use chromiumoxide::cdp::browser_protocol::{
    accessibility, browser, dom, fetch, network, page, target,
};
use chromiumoxide::cdp::js_protocol::runtime;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum BrowserCommand {
    Navigate {
        url: String,
    },
    Snapshot {
        interactive_only: bool,
        token_cap: usize,
    },
    Read {
        reference: NodeRef,
    },
    Fill {
        reference: NodeRef,
        value: String,
    },
    Select {
        reference: NodeRef,
        value: String,
    },
    Scroll {
        delta: i64,
    },
    Back,
    Fetch {
        url: String,
        method: String,
        body: Option<String>,
    },
    Click {
        reference: NodeRef,
    },
    Prepare {
        reference: NodeRef,
    },
    Release {
        intent: BrowserCommitIntent,
    },
    Pending,
    Inspect {
        intent: BrowserCommitIntent,
    },
    Cancel {
        fingerprint: String,
    },
    Screenshot,
    Evidence,
}

#[derive(Clone, Debug)]
pub enum BrowserResult {
    Navigated,
    Snapshot(BrowserSnapshot),
    Text(String),
    Filled,
    CommitRequired { intent: BrowserCommitIntent },
    Screenshot(Vec<u8>),
    Evidence(BrowserEvidence),
    Pending(Vec<BrowserCommitIntent>),
    Released,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserCommitIntent {
    pub origin: String,
    pub url: String,
    pub node_ref: NodeRef,
    pub request_fingerprint: String,
    pub page_state_hash: String,
    #[serde(default)]
    pub request_url: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub expires_at: i64,
}

type Reply = mpsc::Sender<Result<BrowserResult>>;
type Work = (Option<BrowserCommand>, Reply);

pub struct BrowserHost {
    sender: Option<tokio::sync::mpsc::UnboundedSender<Work>>,
    thread: Option<std::thread::JoinHandle<()>>,
    arguments: Vec<String>,
}

#[derive(Clone)]
pub struct BrowserSession {
    sender: tokio::sync::mpsc::UnboundedSender<Work>,
}

impl BrowserHost {
    pub fn launch(config: LaunchConfig, policy: BrowserPolicy) -> Result<Self> {
        let version = std::process::Command::new(&config.executable)
            .arg("--version")
            .output()?;
        if !version.status.success() {
            bail!("Chrome version check failed");
        }
        config.verify_version(&String::from_utf8(version.stdout)?)?;
        for ancestor in config.profile.ancestors() {
            if std::fs::symlink_metadata(ancestor)
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
                && ![std::path::Path::new("/tmp"), std::path::Path::new("/var")].contains(&ancestor)
            {
                bail!("browser profile ancestry cannot contain symlinks");
            }
        }
        mote_paths::ensure_private_dir(&config.profile)?;
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let (ready, init) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("mote-browser".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match runtime {
                    Ok(runtime) => runtime.block_on(run(config, policy, receiver, ready)),
                    Err(error) => {
                        let _ = ready.send(Err(error.into()));
                    }
                }
            })?;
        let arguments = init.recv_timeout(Duration::from_secs(30))??;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
            arguments,
        })
    }

    pub fn session(&self) -> BrowserSession {
        BrowserSession {
            sender: self.sender.as_ref().expect("active browser").clone(),
        }
    }
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
}

impl Drop for BrowserHost {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let (reply, _) = mpsc::channel();
            let _ = sender.send((None, reply));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl BrowserSession {
    pub fn command(&self, command: BrowserCommand) -> Result<BrowserResult> {
        let (reply, result) = mpsc::channel();
        self.sender
            .send((Some(command), reply))
            .map_err(|_| anyhow!("browser worker closed"))?;
        result.recv_timeout(Duration::from_secs(20))?
    }
}

async fn run(
    config: LaunchConfig,
    policy: BrowserPolicy,
    mut receiver: tokio::sync::mpsc::UnboundedReceiver<Work>,
    ready: mpsc::Sender<Result<Vec<String>>>,
) {
    let launched = async {
        let mut builder = BrowserConfig::builder()
            .chrome_executable(&config.executable)
            .user_data_dir(&config.profile)
            .with_head()
            .respect_https_errors()
            .disable_default_args()
            .enable_request_intercept()
            .disable_cache()
            .request_timeout(Duration::from_secs(10));
        for arg in config
            .arguments()
            .into_iter()
            .filter(|arg| !arg.starts_with("--user-data-dir="))
        {
            let arg = arg.trim_start_matches("--");
            builder = match arg.split_once('=') {
                Some((key, value)) => builder.arg((key, value)),
                None => builder.arg(arg),
            };
        }
        let config = builder.build().map_err(|error| anyhow!(error))?;
        let (browser, mut handler) = Browser::launch(config).await?;
        let handler = tokio::spawn(async move {
            while let Some(result) = handler.next().await {
                if result.is_err() {
                    break;
                }
            }
        });
        Ok::<_, anyhow::Error>((browser, handler))
    }
    .await;
    let (mut browser, handler) = match launched {
        Ok(value) => value,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let evidence = Arc::new(Mutex::new(BrowserEvidence::default()));
    let references = Arc::new(Mutex::new(SnapshotMap::default()));
    let boundary = Arc::new(Mutex::new(Boundary::default()));
    let initialized = async {
        browser
            .execute(browser::SetDownloadBehaviorParams::new(
                browser::SetDownloadBehaviorBehavior::Deny,
            ))
            .await?;
        let arguments = browser
            .execute(browser::GetBrowserCommandLineParams::default())
            .await?
            .result
            .arguments;
        browser.start_incognito_context().await?;
        let page = browser.new_page("about:blank").await?;
        let context = browser
            .execute(
                target::GetTargetInfoParams::builder()
                    .target_id(page.target_id().clone())
                    .build(),
            )
            .await?
            .result
            .target_info
            .browser_context_id
            .ok_or_else(|| anyhow!("isolated browser context missing"))?;
        let mut downloads =
            browser::SetDownloadBehaviorParams::new(browser::SetDownloadBehaviorBehavior::Deny);
        downloads.browser_context_id = Some(context);
        browser.execute(downloads).await?;
        install_boundary(
            &page,
            policy.clone(),
            evidence.clone(),
            references.clone(),
            boundary.clone(),
        )
        .await?;
        Ok::<_, anyhow::Error>((page, arguments))
    }
    .await;
    match initialized {
        Err(error) => {
            let _ = ready.send(Err(error));
        }
        Ok((page, arguments)) => {
            let _ = ready.send(Ok(arguments));
            while let Some((command, reply)) = receiver.recv().await {
                let Some(command) = command else {
                    break;
                };
                let result = tokio::time::timeout(
                    Duration::from_secs(15),
                    execute(&page, &policy, &references, &evidence, &boundary, command),
                )
                .await
                .map_err(|_| anyhow!("browser command timed out"))
                .and_then(|result| result);
                if result.is_err() {
                    crate::continuation::cancel_all(&page, &boundary).await;
                }
                let _ = reply.send(result);
            }
        }
    }
    let _ = browser.close().await;
    handler.abort();
}

async fn install_boundary(
    page: &Page,
    policy: BrowserPolicy,
    evidence: Arc<Mutex<BrowserEvidence>>,
    references: Arc<Mutex<SnapshotMap>>,
    boundary: Arc<Mutex<Boundary>>,
) -> Result<()> {
    page.execute(
        page::AddScriptToEvaluateOnNewDocumentParams::builder()
            .source("(()=>{for(const name of ['RTCPeerConnection','webkitRTCPeerConnection','RTCIceTransport','RTCDtlsTransport','RTCSctpTransport'])Object.defineProperty(globalThis,name,{value:undefined,writable:false,configurable:false})})()")
            .run_immediately(true)
            .build()
            .map_err(|error| anyhow!(error))?,
    ).await?;
    let rtc_disabled: bool = page.evaluate("['RTCPeerConnection','webkitRTCPeerConnection','RTCIceTransport','RTCDtlsTransport','RTCSctpTransport'].every(name=>{const descriptor=Object.getOwnPropertyDescriptor(globalThis,name);return descriptor?.value===undefined&&descriptor?.writable===false&&descriptor?.configurable===false})").await?.into_value()?;
    if !rtc_disabled {
        bail!("WebRTC boundary could not be installed");
    }
    let mut requests = page.event_listener::<fetch::EventRequestPaused>().await?;
    let target = page.clone();
    let request_boundary = boundary.clone();
    tokio::spawn(async move {
        while let Some(event) = requests.next().await {
            let disposition = event
                .response_headers
                .as_ref()
                .and_then(|headers| {
                    headers
                        .iter()
                        .find(|header| header.name.eq_ignore_ascii_case("content-disposition"))
                })
                .map(|header| header.value.as_str());
            let mut effect = if event.response_status_code.is_some() {
                policy.response(&event.request.url, disposition)
            } else {
                policy.request(&event.request.url, &event.request.method)
            };
            let redirect = event
                .response_status_code
                .is_some_and(|status| (300..400).contains(&status));
            if redirect {
                let destination = event
                    .response_headers
                    .as_ref()
                    .and_then(|headers| {
                        headers
                            .iter()
                            .find(|header| header.name.eq_ignore_ascii_case("location"))
                    })
                    .and_then(|header| {
                        url::Url::parse(&event.request.url)
                            .ok()?
                            .join(&header.value)
                            .ok()
                    });
                effect = destination
                    .map(|url| policy.request(url.as_str(), "GET"))
                    .unwrap_or(BrowserEffect::Blocked);
            }
            let status = match effect {
                BrowserEffect::Read => "read",
                BrowserEffect::Commit => "commit",
                _ => "blocked",
            };
            evidence.lock().unwrap().record(
                &event.request.url,
                &event.request.method,
                serde_json::to_value(&event.request.headers).unwrap_or(json!({})),
                if event.request.has_post_data == Some(true) {
                    Some("[redacted]")
                } else {
                    None
                },
                status,
            );
            evidence
                .lock()
                .unwrap()
                .entries
                .last_mut()
                .expect("request recorded")
                .response_code = event.response_status_code;
            if effect == BrowserEffect::Commit && event.response_status_code.is_none() {
                let generation = request_boundary.lock().unwrap().generation;
                if let Ok(retained) = Retained::capture(&target, &event, generation).await {
                    let retained_key = retained.fingerprint.clone();
                    let kept = {
                        let mut state = request_boundary.lock().unwrap();
                        state.retain(retained)
                    };
                    if kept {
                        let state = request_boundary.clone();
                        let page = target.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(120)).await;
                            crate::continuation::cancel(&page, &state, &retained_key).await;
                        });
                        continue;
                    }
                }
            }
            let approved_response = event.response_status_code.is_some()
                && request_boundary
                    .lock()
                    .unwrap()
                    .released
                    .remove(&event.request_id);
            if approved_response {
                let success = !redirect
                    && event
                        .response_status_code
                        .is_some_and(|code| (200..300).contains(&code));
                let result = target
                    .execute(fetch::FailRequestParams::new(
                        event.request_id.clone(),
                        network::ErrorReason::Aborted,
                    ))
                    .await;
                request_boundary
                    .lock()
                    .unwrap()
                    .finished
                    .insert(event.request_id.clone(), success && result.is_ok());
                continue;
            }
            let result = if effect == BrowserEffect::Read
                && event.response_status_code.is_some()
                && !redirect
            {
                let origin = url::Url::parse(&event.request.url)
                    .map(|url| url.origin().ascii_serialization())
                    .unwrap_or_default();
                let mut headers = event.response_headers.clone().unwrap_or_default();
                headers.push(fetch::HeaderEntry::new("Content-Security-Policy", format!("connect-src {origin}; frame-src 'none'; worker-src 'none'; object-src 'none'; form-action {origin}; sandbox allow-scripts allow-forms allow-same-origin")));
                match target
                    .execute(fetch::GetResponseBodyParams::new(event.request_id.clone()))
                    .await
                {
                    Ok(body) => {
                        let body = if body.result.base64_encoded {
                            body.result.body
                        } else {
                            base64::engine::general_purpose::STANDARD.encode(body.result.body)
                        };
                        let mut response = fetch::FulfillRequestParams::new(
                            event.request_id.clone(),
                            event.response_status_code.unwrap_or(200),
                        );
                        response.response_headers = Some(headers);
                        response.body = Some(body.into());
                        target.execute(response).await.map(|_| ())
                    }
                    Err(error) => Err(error),
                }
            } else if effect == BrowserEffect::Read {
                target
                    .execute(fetch::ContinueRequestParams::new(event.request_id.clone()))
                    .await
                    .map(|_| ())
            } else {
                target
                    .execute(fetch::FailRequestParams::new(
                        event.request_id.clone(),
                        network::ErrorReason::BlockedByClient,
                    ))
                    .await
                    .map(|_| ())
            };
            if result.is_err() {
                let _ = target.close().await;
                break;
            }
        }
    });
    let mut navigation = page.event_listener::<page::EventFrameNavigated>().await?;
    let frame_references = references.clone();
    let frame_boundary = boundary.clone();
    let frame_page = page.clone();
    tokio::spawn(async move {
        while let Some(event) = navigation.next().await {
            if event.frame.parent_id.is_none() {
                frame_references.lock().unwrap().begin_document();
                crate::continuation::cancel_all(&frame_page, &frame_boundary).await;
            }
        }
    });
    let mut history = page
        .event_listener::<page::EventNavigatedWithinDocument>()
        .await?;
    let history_page = page.clone();
    tokio::spawn(async move {
        while history.next().await.is_some() {
            references.lock().unwrap().begin_document();
            crate::continuation::cancel_all(&history_page, &boundary).await;
        }
    });
    page.execute(
        fetch::EnableParams::builder()
            .pattern(
                fetch::RequestPattern::builder()
                    .url_pattern("*")
                    .request_stage(fetch::RequestStage::Request)
                    .build(),
            )
            .pattern(
                fetch::RequestPattern::builder()
                    .url_pattern("*")
                    .request_stage(fetch::RequestStage::Response)
                    .build(),
            )
            .build(),
    )
    .await?;
    page.execute(network::SetBypassServiceWorkerParams::new(true))
        .await?;
    Ok(())
}

async fn execute(
    page: &Page,
    policy: &BrowserPolicy,
    references: &Arc<Mutex<SnapshotMap>>,
    evidence: &Arc<Mutex<BrowserEvidence>>,
    boundary: &Arc<Mutex<Boundary>>,
    command: BrowserCommand,
) -> Result<BrowserResult> {
    if matches!(command, BrowserCommand::Evidence) {
        return Ok(BrowserResult::Evidence(evidence.lock().unwrap().clone()));
    }
    if let BrowserCommand::Navigate { url } = command {
        if policy.request(&url, "GET") != BrowserEffect::Read {
            bail!("navigation is not an approved read");
        }
        references.lock().unwrap().begin_document();
        crate::continuation::cancel_all(page, boundary).await;
        page.goto(url).await?;
        return Ok(BrowserResult::Navigated);
    }
    let url = page
        .url()
        .await?
        .ok_or_else(|| anyhow!("page has no URL"))?;
    if policy.request(&url, "GET") != BrowserEffect::Read {
        bail!("current page is not approved");
    }
    match command {
        BrowserCommand::Select { reference, value } => {
            let backend = references.lock().unwrap().resolve(&reference)?;
            call_node(page,backend,&format!("function(){{if(!(this instanceof HTMLSelectElement))throw Error('not a select');const value={};if(!Array.from(this.options).some(option=>option.value===value))throw Error('unknown option');Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(this,value);this.dispatchEvent(new Event('input',{{bubbles:true}}));this.dispatchEvent(new Event('change',{{bubbles:true}}));return 'selected'}}",serde_json::to_string(&value)?)).await?;
            Ok(BrowserResult::Filled)
        }
        BrowserCommand::Scroll { delta } => {
            let backend = page
                .execute(dom::GetDocumentParams::default())
                .await?
                .result
                .root
                .backend_node_id;
            call_node(
                page,
                *backend.inner(),
                &format!(
                    "function(){{window.scrollBy(0,{});return 'scrolled'}}",
                    delta.clamp(-10000, 10000)
                ),
            )
            .await?;
            Ok(BrowserResult::Filled)
        }
        BrowserCommand::Back => {
            let history = page
                .execute(page::GetNavigationHistoryParams::default())
                .await?
                .result;
            let entry = history
                .current_index
                .checked_sub(1)
                .and_then(|index| history.entries.get(index as usize))
                .ok_or_else(|| anyhow!("no earlier page"))?;
            if policy.request(&entry.url, "GET") != BrowserEffect::Read {
                bail!("history destination is not an approved read");
            }
            references.lock().unwrap().begin_document();
            crate::continuation::cancel_all(page, boundary).await;
            let mut navigation = page.event_listener::<page::EventFrameNavigated>().await?;
            let mut history_changes = page
                .event_listener::<page::EventNavigatedWithinDocument>()
                .await?;
            page.execute(page::NavigateToHistoryEntryParams::new(entry.id))
                .await?;
            tokio::time::timeout(Duration::from_secs(5), async {
                tokio::select! {
                    _ = navigation.next() => (),
                    _ = history_changes.next() => (),
                }
            })
            .await?;
            Ok(BrowserResult::Navigated)
        }
        BrowserCommand::Fetch { url, method, body } => {
            let effect = policy.request(&url, &method);
            if !matches!(effect, BrowserEffect::Read | BrowserEffect::Commit) {
                bail!("request is outside approved origins");
            }
            let backend = page
                .execute(dom::GetDocumentParams::default())
                .await?
                .result
                .root
                .backend_node_id;
            if effect == BrowserEffect::Commit {
                let reference = references.lock().unwrap().reference(*backend.inner());
                return crate::continuation::prepare_request(
                    page,
                    policy,
                    references,
                    boundary,
                    reference,
                    crate::continuation::NativeRequest { url, method, body },
                    None,
                )
                .await;
            }
            if body.is_some() {
                bail!("read requests cannot include a body");
            }
            let value = call_node(page,*backend.inner(),&format!("async function(){{const response=await fetch({},{{credentials:'omit',redirect:'error',mode:'same-origin'}});return (await response.text()).slice(0,32768)}}",serde_json::to_string(&url)?)).await?;
            Ok(BrowserResult::Text(format!(
                "<untrusted_browser_page>\n{}\n</untrusted_browser_page>",
                mote_redact::redact(&value)
            )))
        }
        BrowserCommand::Snapshot {
            interactive_only,
            token_cap,
        } => {
            let ax = page
                .execute(accessibility::GetFullAxTreeParams::default())
                .await?
                .result;
            let canonical = serde_json::to_vec(&ax)?;
            let page_state_hash = format!(
                "{:x}",
                Sha256::digest([url.as_bytes(), &canonical].concat())
            );
            let tokenizer = tiktoken_rs::o200k_base()?;
            let mut text = String::new();
            let mut truncated = false;
            let mut nodes = Vec::new();
            let cap = token_cap.min(8192);
            let notice = "[snapshot token cap reached]";
            for node in ax.nodes {
                if node.ignored {
                    continue;
                }
                let role = node
                    .role
                    .and_then(|value| value.value)
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_default();
                if interactive_only
                    && ![
                        "textbox",
                        "button",
                        "link",
                        "checkbox",
                        "combobox",
                        "listbox",
                        "radio",
                        "slider",
                        "spinbutton",
                    ]
                    .contains(&role.as_str())
                {
                    continue;
                }
                let Some(backend) = node.backend_dom_node_id else {
                    continue;
                };
                let reference = references.lock().unwrap().reference(*backend.inner());
                let name = node
                    .name
                    .and_then(|value| value.value)
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_default();
                let value = node
                    .value
                    .and_then(|value| value.value)
                    .and_then(|value| value.as_str().map(mote_redact::redact));
                let node = BrowserNode {
                    reference,
                    role,
                    name: mote_redact::redact(&name),
                    value,
                };
                let line = format!(
                    "{} {} {} {}\n",
                    node.reference.0,
                    node.role,
                    serde_json::to_string(&node.name)?,
                    serde_json::to_string(&node.value)?
                );
                let candidate = format!("{text}{line}");
                if tokenizer
                    .encode_ordinary(&format!("{candidate}{notice}"))
                    .len()
                    > cap
                {
                    truncated = true;
                    break;
                }
                text = candidate;
                nodes.push(node);
            }
            references.lock().unwrap().retain_snapshot(
                &nodes
                    .iter()
                    .map(|node| node.reference.clone())
                    .collect::<Vec<_>>(),
            );
            if truncated && tokenizer.encode_ordinary(&format!("{text}{notice}")).len() <= cap {
                text.push_str(notice);
            }
            Ok(BrowserResult::Snapshot(BrowserSnapshot {
                url,
                nodes,
                text,
                truncated,
                page_state_hash,
            }))
        }
        BrowserCommand::Read { reference } => {
            let backend = references.lock().unwrap().resolve(&reference)?;
            let value = call_node(page, backend, "function(){return this.textContent}").await?;
            Ok(BrowserResult::Text(format!(
                "<untrusted_browser_page>\n{}\n</untrusted_browser_page>",
                mote_redact::redact(&value)
            )))
        }
        BrowserCommand::Fill { reference, value } => {
            let backend = references.lock().unwrap().resolve(&reference)?;
            let value = serde_json::to_string(&value)?;
            call_node(page, backend, &format!("function(){{if(!(this instanceof HTMLInputElement || this instanceof HTMLTextAreaElement))throw Error('not a fillable field');const setter=Object.getOwnPropertyDescriptor(this instanceof HTMLInputElement?HTMLInputElement.prototype:HTMLTextAreaElement.prototype,'value').set;setter.call(this,{value});this.dispatchEvent(new Event('input',{{bubbles:true}}));this.dispatchEvent(new Event('change',{{bubbles:true}}));return 'filled'}}")).await?;
            Ok(BrowserResult::Filled)
        }
        BrowserCommand::Click { reference } => {
            let backend = references.lock().unwrap().resolve(&reference)?;
            let request = call_node(page, backend, "function(){if(!this.isConnected||this.ownerDocument!==document)throw Error('detached node');const form=this.form;return JSON.stringify({tag:this.tagName,type:this.type||'',href:this.href||'',method:form?.method||'',action:form?.action||'',fields:form?Array.from(form.elements,field=>({name:field.name,type:field.type,value:field.value,checked:field.checked})):[]})}").await?;
            let ax = page
                .execute(accessibility::GetFullAxTreeParams::default())
                .await?
                .result;
            let canonical = serde_json::to_vec(&ax)?;
            Ok(BrowserResult::CommitRequired {
                intent: BrowserCommitIntent {
                    origin: url::Url::parse(&url)?.origin().ascii_serialization(),
                    page_state_hash: format!(
                        "{:x}",
                        Sha256::digest([url.as_bytes(), &canonical].concat())
                    ),
                    request_fingerprint: format!(
                        "{:x}",
                        Sha256::digest([reference.0.as_bytes(), request.as_bytes()].concat())
                    ),
                    url,
                    node_ref: reference,
                    request_url: String::new(),
                    method: String::new(),
                    expires_at: 0,
                },
            })
        }
        BrowserCommand::Screenshot => Ok(BrowserResult::Screenshot(
            base64::engine::general_purpose::STANDARD.decode(
                page.execute(page::CaptureScreenshotParams::default())
                    .await?
                    .result
                    .data,
            )?,
        )),
        BrowserCommand::Prepare { reference } => {
            crate::continuation::prepare(page, policy, references, boundary, reference).await
        }
        BrowserCommand::Pending => crate::continuation::pending(page, references, boundary).await,
        BrowserCommand::Inspect { intent } => {
            crate::continuation::inspect(page, references, boundary, &intent).await?;
            Ok(BrowserResult::CommitRequired { intent })
        }
        BrowserCommand::Release { intent } => {
            crate::continuation::release(page, references, boundary, intent).await
        }
        BrowserCommand::Cancel { fingerprint } => {
            crate::continuation::cancel(page, boundary, &fingerprint).await;
            Ok(BrowserResult::Released)
        }
        BrowserCommand::Evidence => Ok(BrowserResult::Evidence(evidence.lock().unwrap().clone())),
        BrowserCommand::Navigate { .. } => unreachable!(),
    }
}

pub(crate) async fn call_node(page: &Page, backend: i64, function: &str) -> Result<String> {
    let frame = page
        .execute(page::GetFrameTreeParams::default())
        .await?
        .result
        .frame_tree
        .frame
        .id;
    let context = page
        .execute(
            page::CreateIsolatedWorldParams::builder()
                .frame_id(frame)
                .world_name("mote-browser-actions")
                .build()
                .map_err(|error| anyhow!(error))?,
        )
        .await?
        .result
        .execution_context_id;
    let backend = dom::BackendNodeId::new(backend);
    let node = page
        .execute(
            dom::ResolveNodeParams::builder()
                .backend_node_id(backend)
                .execution_context_id(context)
                .build(),
        )
        .await?
        .result
        .object;
    let object = node
        .object_id
        .ok_or_else(|| anyhow!("node is no longer in the current document"))?;
    let result = page
        .execute(
            runtime::CallFunctionOnParams::builder()
                .object_id(object)
                .function_declaration(function)
                .return_by_value(true)
                .await_promise(true)
                .build()
                .map_err(|error| anyhow!(error))?,
        )
        .await?
        .result;
    if result.exception_details.is_some() {
        bail!("browser node action rejected");
    }
    Ok(result
        .result
        .value
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default())
}
