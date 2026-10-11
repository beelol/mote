use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct BrowserFixture {
    addr: SocketAddr,
    counts: Arc<Mutex<HashMap<String, usize>>>,
    running: Arc<AtomicBool>,
}

impl BrowserFixture {
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let counts = Arc::new(Mutex::new(HashMap::new()));
        let running = Arc::new(AtomicBool::new(true));
        let state = counts.clone();
        let run = running.clone();
        std::thread::spawn(move || {
            for connection in listener.incoming() {
                if !run.load(Ordering::Acquire) {
                    break;
                }
                if let Ok(connection) = connection {
                    let state = state.clone();
                    std::thread::spawn(move || {
                        let _ = serve(connection, state);
                    });
                }
            }
        });
        Ok(Self {
            addr,
            counts,
            running,
        })
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url())
    }
    pub fn count(&self, effect: &str) -> usize {
        *self.counts.lock().unwrap().get(effect).unwrap_or(&0)
    }
}

impl Drop for BrowserFixture {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        let _ = TcpStream::connect(self.addr);
    }
}

fn serve(mut stream: TcpStream, counts: Arc<Mutex<HashMap<String, usize>>>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let path = line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let mut content_length = 0;
    let mut seeded_cookie = false;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some(length) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = length.trim().parse::<usize>().unwrap_or(0).min(65536);
        }
        if header.to_ascii_lowercase().starts_with("cookie:")
            && header.contains("fixture_session=seed")
        {
            seeded_cookie = true;
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    let mut status = "200 OK";
    let mut extra = String::new();
    let mut content_type = "text/html";
    let response = if let Some(effect) = path.strip_prefix("/effect/") {
        *counts.lock().unwrap().entry(effect.into()).or_default() += 1;
        if effect == "download" {
            extra = "Content-Disposition: attachment; filename=fixture.txt\r\n".into();
        }
        "fixture effect".into()
    } else if let Some(effect) = path.strip_prefix("/trigger/") {
        let secondary = match effect {
            "popup" => Some("window.open('/effect/popup')"),
            "frame" => Some("let frame=document.createElement('iframe');frame.src='/effect/frame';document.body.append(frame)"),
            "worker" => Some("try{new Worker('/effect/worker')}catch(e){}"),
            "serviceworker" => Some("navigator.serviceWorker.register('/effect/serviceworker').catch(()=>{})"),
            _ => None,
        };
        if let Some(script) = secondary {
            let response = format!("<!doctype html><title>Secondary context</title><body><script>{script}</script></body>");
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len())?;
            return stream.flush();
        }
        if effect == "websocket" {
            let response = "<!doctype html><title>Websocket</title><script>new WebSocket('ws://'+location.host+'/effect/websocket')</script>";
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len())?;
            return stream.flush();
        }
        let method = if matches!(effect, "download" | "oauth") {
            "GET"
        } else {
            "POST"
        };
        format!("<!doctype html><title>Trigger</title><h1>Trigger {effect}</h1><script>fetch('/effect/{effect}',{{method:'{method}',headers:{{'Authorization':'Bearer fixture-secret'}},body:{}}}).catch(()=>{{}})</script>", if method == "GET" { "undefined" } else { "'password=fixture-secret'" })
    } else if path == "/seed-state" {
        "<!doctype html><title>Seed state</title><h1>Controlled profile seed</h1>".into()
    } else if path == "/seed-worker.js" {
        content_type = "application/javascript";
        "self.addEventListener('install',()=>self.skipWaiting());self.addEventListener('activate',event=>event.waitUntil(self.clients.claim()));self.addEventListener('fetch',event=>{if(new URL(event.request.url).pathname==='/inherited-state')event.respondWith(new Response('<h1>inherited worker state</h1>',{headers:{'Content-Type':'text/html'}}))})".into()
    } else if path == "/inherited-state" {
        format!("<!doctype html><title>State isolation</title><h1>fresh HTTP state</h1><p>cookie {}</p>", if seeded_cookie { "present" } else { "absent" })
    } else if path == "/same-document" {
        "<!doctype html><title>History</title><label>History<input oninput=\"history.pushState({},'', '/navigation')\"></label>".into()
    } else if path == "/redirect" {
        status = "302 Found";
        extra = "Location: /navigation\r\n".into();
        String::new()
    } else if path == "/off-origin" {
        status = "302 Found";
        extra = "Location: http://127.0.0.1:1/forbidden\r\n".into();
        String::new()
    } else if path == "/injection" {
        include_str!("../fixtures/browser/injection.html").into()
    } else if path == "/navigation" || path == "/stale" {
        "<!doctype html><title>New document</title><h1>Navigation complete</h1><label>Other<input></label>".into()
    } else {
        include_str!("../fixtures/browser/read.html").into()
    };
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len())?;
    stream.flush()
}
