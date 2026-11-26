//! A tiny fake Google for tests: canned replies chosen by path prefix and query text.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct Route {
    path: String,
    method: Option<&'static str>,
    query: Vec<&'static str>,
    status: u16,
    body: String,
    max_hits: usize,
    hits: AtomicUsize,
}

impl Route {
    pub fn new(path: impl Into<String>, status: u16, body: &str) -> Route {
        Route {
            path: path.into(),
            method: None,
            query: vec![],
            status,
            body: body.to_string(),
            max_hits: usize::MAX,
            hits: AtomicUsize::new(0),
        }
    }

    /// Only requests with this HTTP method.
    pub fn method(mut self, m: &'static str) -> Route {
        self.method = Some(m);
        self
    }

    /// Only requests whose target contains every one of these.
    pub fn with(mut self, query: &[&'static str]) -> Route {
        self.query = query.to_vec();
        self
    }

    /// Stops matching after this many requests.
    pub fn times(mut self, n: usize) -> Route {
        self.max_hits = n;
        self
    }
}

pub struct Fake {
    pub base: String,
    /// `"<METHOD> <target>\n<body>"` for every request, in order.
    pub requests: Arc<Mutex<Vec<String>>>,
}

pub async fn serve(routes: Vec<Route>) -> Fake {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let routes = Arc::new(routes);
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let (routes, log) = (routes.clone(), log.clone());
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 2048];
                let (head_end, content_len) = loop {
                    let n = stream.read(&mut chunk).await.unwrap_or(0);
                    buf.extend_from_slice(&chunk[..n]);
                    if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..i]).to_lowercase();
                        let len = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        break (i + 4, len);
                    }
                    if n == 0 {
                        return;
                    }
                };
                while buf.len() < head_end + content_len {
                    let n = stream.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
                let mut first = head.lines().next().unwrap_or("").split(' ');
                let (method, target) = (first.next().unwrap_or(""), first.next().unwrap_or(""));
                log.lock()
                    .unwrap()
                    .push(format!("{method} {target}\n{body}"));
                let route = routes.iter().find(|r| {
                    target.starts_with(&r.path)
                        && r.method.is_none_or(|m| m == method)
                        && r.query
                            .iter()
                            .all(|q| target.contains(q) || body.contains(q))
                        && r.hits.load(Ordering::SeqCst) < r.max_hits
                });
                let (status, body) = match route {
                    Some(r) => {
                        r.hits.fetch_add(1, Ordering::SeqCst);
                        (r.status, r.body.clone())
                    }
                    None => (404, "{}".to_string()),
                };
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    Fake { base, requests }
}
