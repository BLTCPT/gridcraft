//! Native host transport shared by desktop, CLI and the headless MCP host.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unimplemented)]
use gridcraft_engine::{
    Session,
    analysis::{
        network::{Request, Transport, simulator_endpoint},
        xmla::MAX_BYTES,
    },
};
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
    time::Duration,
};

static ACTIVE_REQUESTS: AtomicUsize = AtomicUsize::new(0);
struct Permit;
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE_REQUESTS.fetch_sub(1, Ordering::Relaxed);
    }
}

pub struct Client {
    timeout: Duration,
}
impl Default for Client {
    fn default() -> Self {
        Self { timeout: Duration::from_secs(20) }
    }
}
impl Client {
    /// Lower timeouts are useful for protocol simulator tests; never disable TLS validation.
    pub fn with_timeout(timeout: Duration) -> Self {
        Self { timeout: timeout.clamp(Duration::from_millis(50), Duration::from_secs(20)) }
    }
    fn send(&self, request: Request) -> Result<Vec<u8>, String> {
        if request.canceled.load(Ordering::Relaxed) {
            return Err("BW request canceled.".into());
        }
        let url = reqwest::Url::parse(&request.endpoint).map_err(|_| "Invalid BW endpoint.")?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || (!request.simulator && (url.scheme() != "https" || url.host_str().is_none()))
            || (request.simulator && !simulator_endpoint(&request.endpoint))
        {
            return Err("BW requires HTTPS; the synthetic simulator requires numeric loopback HTTP.".into());
        }
        let (user, password) = if request.simulator {
            ("synthetic".to_string(), "synthetic".to_string())
        } else {
            // Explicit origin pin prevents a workbook or edited URL from exfiltrating environment credentials.
            if std::env::var("GRIDCRAFT_BW_XMLA_URL").ok().as_deref() != Some(request.endpoint.as_str()) {
                return Err("Approve this exact endpoint locally with GRIDCRAFT_BW_XMLA_URL before connecting.".into());
            }
            (
                std::env::var("GRIDCRAFT_BW_USER").map_err(|_| "Set GRIDCRAFT_BW_USER in the launch environment.")?,
                std::env::var("GRIDCRAFT_BW_PASSWORD")
                    .map_err(|_| "Set GRIDCRAFT_BW_PASSWORD in the launch environment; never in workbook commands.")?,
            )
        };
        let client = reqwest::blocking::Client::builder()
            .https_only(!request.simulator)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(self.timeout)
            .connect_timeout(self.timeout.min(Duration::from_secs(10)))
            .build()
            .map_err(|_| "Could not initialize BW TLS transport.")?;
        let response = client
            .post(url)
            .basic_auth(user, Some(password))
            .header("Content-Type", "text/xml; charset=utf-8")
            .header("SOAPAction", format!("\"urn:schemas-microsoft-com:xml-analysis:{}\"", request.action))
            .body(request.body)
            .send()
            .map_err(|_| "BW request failed (network, TLS or timeout). Previous results were preserved.")?;
        if !response.status().is_success() {
            return Err(format!("BW returned HTTP {}. Previous results were preserved.", response.status().as_u16()));
        }
        let mut bytes = Vec::new();
        response.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| "Cannot read BW response.")?;
        if bytes.len() > MAX_BYTES {
            return Err("BW response exceeds 8 MiB.".into());
        }
        if request.canceled.load(Ordering::Relaxed) {
            return Err("BW request canceled; response discarded.".into());
        }
        Ok(bytes)
    }
}
impl Transport for Client {
    fn start(&self, request: Request) -> Result<Receiver<Result<Vec<u8>, String>>, String> {
        ACTIVE_REQUESTS
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| (n < 8).then_some(n + 1))
            .map_err(|_| "Too many BW requests are still running; wait for cancellation/timeout.".to_string())?;
        let permit = Permit;
        let (tx, rx) = mpsc::channel();
        let client = Self { timeout: self.timeout };
        std::thread::Builder::new()
            .name("bw-xmla".into())
            .spawn(move || {
                let _permit = permit;
                let _ = tx.send(client.send(request));
            })
            .map_err(|_| "Cannot start BW request worker.".to_string())?;
        Ok(rx)
    }
}
pub fn install(session: &mut Session) {
    session.analysis_transport = Some(Arc::new(Client::default()));
}
