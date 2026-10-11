//! Portable synthetic XMLA service for integration testing. No SAP code, runtime or real data.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unimplemented)]
use quick_xml::{NsReader, events::Event, name::ResolveResult};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};
const XMLA: &[u8] = b"urn:schemas-microsoft-com:xml-analysis";
const SOAP: &[u8] = b"http://schemas.xmlsoap.org/soap/envelope/";
#[derive(Clone, Copy, Debug, Default)]
#[repr(u8)]
pub enum Scenario {
    #[default]
    Normal,
    Unauthorized,
    Forbidden,
    Expired,
    Fault,
    Malformed,
    Slow,
    MixedUnits,
    Duplicate,
    Redirect,
    WrongNamespace,
    MissingMeasure,
    Multidimensional,
}
impl Scenario {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "normal" => Self::Normal,
            "unauthorized" => Self::Unauthorized,
            "forbidden" => Self::Forbidden,
            "expired" => Self::Expired,
            "fault" => Self::Fault,
            "malformed" => Self::Malformed,
            "slow" => Self::Slow,
            "mixed-units" => Self::MixedUnits,
            "duplicate" => Self::Duplicate,
            "redirect" => Self::Redirect,
            "wrong-namespace" => Self::WrongNamespace,
            "missing-measure" => Self::MissingMeasure,
            "multidimensional" => Self::Multidimensional,
            _ => return None,
        })
    }
}
pub struct Simulator {
    pub endpoint: String,
    scenario: Arc<AtomicU8>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    pub requests: Arc<AtomicUsize>,
}
impl Simulator {
    pub fn start(port: u16) -> Result<Self, String> {
        let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|_| "Cannot bind local simulator port.")?;
        let port = listener.local_addr().map_err(|_| "Cannot inspect simulator port.")?.port();
        listener.set_nonblocking(true).map_err(|_| "Cannot initialize simulator listener.")?;
        let stop = Arc::new(AtomicBool::new(false));
        let scenario = Arc::new(AtomicU8::new(0));
        let requests = Arc::new(AtomicUsize::new(0));
        let (st, sc, count) = (stop.clone(), scenario.clone(), requests.clone());
        let active = Arc::new(AtomicUsize::new(0));
        let worker = thread::Builder::new()
            .name("xmla-simulator".into())
            .spawn(move || {
                while !st.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if active.load(Ordering::Relaxed) >= 8 {
                                continue;
                            }
                            active.fetch_add(1, Ordering::Relaxed);
                            let (sc, count, a) = (sc.clone(), count.clone(), active.clone());
                            let _ = thread::Builder::new().name("xmla-fixture-request".into()).spawn(move || {
                                let _ = serve(stream, sc.load(Ordering::Relaxed), &count);
                                a.fetch_sub(1, Ordering::Relaxed);
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(5)),
                        Err(_) => break,
                    }
                }
            })
            .map_err(|_| "Cannot start simulator worker.")?;
        Ok(Self { endpoint: format!("http://127.0.0.1:{port}/xmla"), scenario, stop, worker: Some(worker), requests })
    }
    pub fn set_scenario(&self, scenario: Scenario) {
        self.scenario.store(scenario as u8, Ordering::Relaxed);
    }
}
impl Drop for Simulator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn write_response(stream: &mut TcpStream, code: u16, body: &str, extra: &str) -> Result<(), String> {
    write!(
        stream,
        "HTTP/1.1 {code} Fixture\r\nContent-Type: text/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )
    .map_err(|_| "Simulator connection ended.".into())
}
fn serve(mut stream: TcpStream, scenario: u8, count: &AtomicUsize) -> Result<(), String> {
    stream.set_read_timeout(Some(Duration::from_secs(2))).map_err(|_| "Socket setup failed.")?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(|_| "Socket setup failed.")?;
    let mut bytes = Vec::new();
    let (header_end, size) = loop {
        if bytes.len() > 70_000 {
            return Err("Request too large.".into());
        }
        if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            let header = std::str::from_utf8(bytes.get(..end).ok_or("Invalid request.")?).map_err(|_| "Invalid headers.")?;
            if !header.starts_with("POST /xmla HTTP/1.1\r\n") {
                return write_response(&mut stream, 404, "Not found", "");
            }
            let lower = header.to_ascii_lowercase();
            if !lower.lines().any(|l| l == "authorization: basic c3ludghldgljonn5bnrozxrpyw==")
                || scenario == Scenario::Unauthorized as u8
                || scenario == Scenario::Expired as u8
            {
                return write_response(&mut stream, 401, "Synthetic authentication required", "");
            }
            if lower.contains("transfer-encoding:") {
                return write_response(&mut stream, 400, "Chunked requests unsupported", "");
            }
            let size = lower
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|s| s.trim().parse::<usize>().ok())
                .filter(|n| *n <= 65_536)
                .ok_or("Invalid content length.")?;
            break (end + 4, size);
        }
        let mut buf = [0; 4096];
        let n = stream.read(&mut buf).map_err(|_| "Request read failed.")?;
        if n == 0 {
            return Err("Incomplete request.".into());
        }
        bytes.extend_from_slice(buf.get(..n).ok_or("Invalid read size.")?);
    };
    while bytes.len() < header_end + size {
        let mut buf = [0; 4096];
        let n = stream.read(&mut buf).map_err(|_| "Request read failed.")?;
        if n == 0 {
            return Err("Incomplete body.".into());
        }
        bytes.extend_from_slice(buf.get(..n).ok_or("Invalid read size.")?);
    }
    count.fetch_add(1, Ordering::Relaxed);
    if scenario == Scenario::Forbidden as u8 {
        return write_response(&mut stream, 403, "Synthetic forbidden", "");
    }
    if scenario == Scenario::Redirect as u8 {
        return write_response(&mut stream, 302, "", "Location: http://127.0.0.1:1/never\r\n");
    }
    if scenario == Scenario::Slow as u8 {
        thread::sleep(Duration::from_millis(700));
    }
    let body = bytes.get(header_end..header_end + size).ok_or("Invalid body.")?;
    let (action, request_type, statement, catalog) = parse_request(body)?;
    if scenario == Scenario::Fault as u8 {
        return write_response(&mut stream, 200, &fault(), "");
    }
    if scenario == Scenario::Malformed as u8 {
        return write_response(&mut stream, 200, "<broken", "");
    }
    if scenario == Scenario::Multidimensional as u8 {
        return write_response(
            &mut stream,
            200,
            &envelope("Execute", "<root xmlns=\"urn:schemas-microsoft-com:xml-analysis:mddataset\"><OlapInfo/><Axes/><CellData/></root>"),
            "",
        );
    }
    let rows = if action == "Discover" {
        match request_type.as_str() {
            "DISCOVER_DATASOURCES" => vec![vec![("DataSourceName", "SYNTHETIC_BW".to_string()), ("ProviderType", "MDP".into())]],
            "DBSCHEMA_CATALOGS" => vec![vec![("CATALOG_NAME", "SYNTHETIC_BW".into()), ("DESCRIPTION", "Original synthetic fixture".into())]],
            "MDSCHEMA_CUBES" => {
                vec![vec![("CATALOG_NAME", "SYNTHETIC_BW".into()), ("CUBE_NAME", "ZDEMO_SALES".into()), ("CUBE_CAPTION", "Synthetic sales".into())]]
            }
            "MDSCHEMA_DIMENSIONS" | "MDSCHEMA_HIERARCHIES" => ["Region", "Product", "FiscalYear"]
                .iter()
                .map(|name| vec![("DIMENSION_UNIQUE_NAME", format!("[{name}]")), ("DIMENSION_CAPTION", name.to_string())])
                .collect(),
            "MDSCHEMA_MEASURES" => {
                vec![vec![("MEASURE_UNIQUE_NAME", "[Measures].[Revenue]".into()), ("MEASURE_CAPTION", "Revenue".into()), ("DATA_TYPE", "5".into())]]
            }
            "MDSCHEMA_SAP_VARIABLES" => vec![vec![
                ("VARIABLE_NAME", "0FISCYEAR".into()),
                ("VARIABLE_CAPTION", "Fiscal year (synthetic metadata only)".into()),
                ("VARIABLE_IS_MANDATORY", "true".into()),
                ("VARIABLE_DEFAULT_LOW", "2026".into()),
            ]],
            _ => return write_response(&mut stream, 200, &fault(), ""),
        }
    } else {
        if catalog != "SYNTHETIC_BW" || !statement.contains("FROM [ZDEMO_SALES]") || !statement.starts_with("SELECT ") {
            return write_response(&mut stream, 200, &fault(), "");
        }
        let year = if statement.contains("[FiscalYear].[2026]") {
            2026
        } else if statement.contains("[FiscalYear].[2025]") {
            2025
        } else {
            return write_response(&mut stream, 200, &fault(), "");
        };
        let mut rows = Vec::new();
        for (ri, region) in ["Americas", "Europe", "Asia Pacific"].iter().enumerate() {
            for (pi, product) in ["Hardware", "Services"].iter().enumerate() {
                let revenue: usize = (1..=4)
                    .map(|quarter| ((ri + 1) * 40 + (pi + 1) * 12 + quarter * 8 + (year - 2025) * 10) * if pi == 0 { 1250 } else { 450 })
                    .sum();
                rows.push(vec![
                    ("Region", region.to_string()),
                    ("Product", product.to_string()),
                    (if scenario == Scenario::MissingMeasure as u8 { "Other" } else { "Revenue" }, revenue.to_string()),
                    ("Currency", if scenario == Scenario::MixedUnits as u8 && ri == 0 { "EUR" } else { "USD" }.into()),
                ]);
            }
        }
        if scenario == Scenario::Duplicate as u8
            && let Some(first) = rows.first().cloned()
        {
            rows.push(first);
        }
        rows
    };
    let mut root = String::from("<root xmlns=\"urn:schemas-microsoft-com:xml-analysis:rowset\">");
    for row in rows {
        root.push_str("<row>");
        for (key, value) in row {
            root.push_str(&format!("<{key}>{}</{key}>", quick_xml::escape::escape(&value)));
        }
        root.push_str("</row>");
    }
    root.push_str("</root>");
    if scenario == Scenario::WrongNamespace as u8 {
        root = root.replace("xml-analysis:rowset", "untrusted");
    }
    write_response(&mut stream, 200, &envelope(&action, &root), "")
}
fn envelope(action: &str, root: &str) -> String {
    format!(
        "<soap:Envelope xmlns:soap=\"http://schemas.xmlsoap.org/soap/envelope/\"><soap:Body><{action}Response xmlns=\"urn:schemas-microsoft-com:xml-analysis\"><return>{root}</return></{action}Response></soap:Body></soap:Envelope>"
    )
}
fn fault() -> String {
    "<soap:Envelope xmlns:soap=\"http://schemas.xmlsoap.org/soap/envelope/\"><soap:Body><soap:Fault><faultcode>soap:Client</faultcode><faultstring>Synthetic unsupported query, catalog or missing required year</faultstring></soap:Fault></soap:Body></soap:Envelope>".into()
}
fn parse_request(bytes: &[u8]) -> Result<(String, String, String, String), String> {
    let mut reader = NsReader::from_reader(bytes);
    let (mut action, mut rowset, mut statement, mut catalog, mut field) = (String::new(), String::new(), String::new(), String::new(), String::new());
    let mut depth = 0usize;
    let mut envelope = false;
    loop {
        match reader.read_resolved_event().map_err(|_| "Malformed XMLA request.")? {
            (ns, Event::Start(e)) => {
                depth += 1;
                if depth > 32 {
                    return Err("XMLA request too deep.".into());
                }
                let local = e.local_name();
                if local.as_ref() == b"Envelope" && matches!(&ns, ResolveResult::Bound(n) if n.as_ref() == SOAP) {
                    envelope = true;
                }
                if matches!(&ns, ResolveResult::Bound(n) if n.as_ref() == XMLA) {
                    match local.as_ref() {
                        b"Discover" => action = "Discover".into(),
                        b"Execute" => action = "Execute".into(),
                        b"RequestType" | b"Statement" | b"Catalog" => field = String::from_utf8_lossy(local.as_ref()).into_owned(),
                        _ => {}
                    }
                }
            }
            (_, Event::Text(t)) => {
                let value = t.decode().map_err(|_| "Invalid XMLA text.")?;
                match field.as_str() {
                    "RequestType" => rowset.push_str(&value),
                    "Statement" => statement.push_str(&value),
                    "Catalog" => catalog.push_str(&value),
                    _ => {}
                }
            }
            (_, Event::GeneralRef(r)) => {
                let entity = r.decode().map_err(|_| "Invalid XML entity.")?;
                let escaped = format!("&{entity};");
                let text = quick_xml::escape::unescape(&escaped).map_err(|_| "Invalid XML entity.")?;
                match field.as_str() {
                    "Statement" => statement.push_str(&text),
                    "Catalog" => catalog.push_str(&text),
                    _ => {}
                }
            }
            (_, Event::End(_)) => {
                depth = depth.saturating_sub(1);
                field.clear();
            }
            (_, Event::DocType(_) | Event::PI(_)) => return Err("Unsupported XML declaration.".into()),
            (_, Event::Eof) => break,
            _ => {}
        }
    }
    if !envelope || action.is_empty() || depth != 0 {
        return Err("Not a supported SOAP/XMLA request.".into());
    }
    Ok((action, rowset, statement, catalog))
}
