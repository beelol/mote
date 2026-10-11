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
    Click {
        reference: NodeRef,
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserCommitIntent {
    pub origin: String,
    pub url: String,
    pub node_ref: NodeRef,
    pub request_fingerprint: String,
    pub page_state_hash: String,
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
        install_boundary(&page, policy.clone(), evidence.clone(), references.clone()).await?;
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
                    execute(&page, &policy, &references, &evidence, command),
                )
                .await
                .map_err(|_| anyhow!("browser command timed out"))
                .and_then(|result| result);
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
) -> Result<()> {
    let mut requests = page.event_listener::<fetch::EventRequestPaused>().await?;
    let target = page.clone();
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
    tokio::spawn(async move {
        while let Some(event) = navigation.next().await {
            if event.frame.parent_id.is_none() {
                frame_references.lock().unwrap().begin_document();
            }
        }
    });
    let mut history = page
        .event_listener::<page::EventNavigatedWithinDocument>()
        .await?;
    tokio::spawn(async move {
        while history.next().await.is_some() {
            references.lock().unwrap().begin_document();
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
                if tokenizer.encode_ordinary(&candidate).len() > token_cap.min(8192) {
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
            if truncated {
                text.push_str("[snapshot token cap reached]");
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
                },
            })
        }
        BrowserCommand::Screenshot => Ok(BrowserResult::Screenshot(
            page.screenshot(chromiumoxide::page::ScreenshotParams::default())
                .await?,
        )),
        BrowserCommand::Evidence => Ok(BrowserResult::Evidence(evidence.lock().unwrap().clone())),
        BrowserCommand::Navigate { .. } => unreachable!(),
    }
}

async fn call_node(page: &Page, backend: i64, function: &str) -> Result<String> {
    let backend = dom::BackendNodeId::new(backend);
    let node = page
        .execute(
            dom::ResolveNodeParams::builder()
                .backend_node_id(backend)
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
