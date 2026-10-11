//! CLI-only XMLA probe. Network requests require explicit --send and HTTPS.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unimplemented)]
use gridcraft_engine::analysis::network::{Request, Transport};
use gridcraft_engine::analysis::xmla::{MAX_BYTES, discover, execute, parse_rowset};
use std::{io::Read, process::ExitCode};

const USAGE: &str = "Experimental BW XMLA probe (not AO/BICS parity)\n\n  gridcraft-bw discover [--rowset MDSCHEMA_CUBES] [--catalog ID] [--cube ID]\n  gridcraft-bw execute --catalog ID --mdx-file query.mdx\n  gridcraft-bw parse --response-file response.xml\n\nDiscover/execute print a SOAP request without contacting a server. Add --send to call\nGRIDCRAFT_BW_XMLA_URL over HTTPS, with GRIDCRAFT_BW_USER and GRIDCRAFT_BW_PASSWORD\nread only from the process environment. No redirects, credential storage or TLS bypass.\nOutput is a JSON rowset; it is not imported into a workbook. Keep real output private.";

fn bounded_file(path: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|_| "Cannot open input file.".to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| "Cannot read input file.".to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("Input exceeds 8 MiB.".into());
    }
    Ok(bytes)
}
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(mode) = args.first() else {
        println!("{USAGE}");
        return Ok(());
    };
    if mode == "--help" {
        println!("{USAGE}");
        return Ok(());
    }
    let mut options = std::collections::BTreeMap::new();
    let mut send = false;
    let mut i = 1;
    while i < args.len() {
        let key = args.get(i).ok_or("Missing argument.")?;
        if key == "--send" {
            send = true;
            i += 1;
            continue;
        }
        if !["--rowset", "--catalog", "--cube", "--mdx-file", "--response-file"].contains(&key.as_str()) {
            return Err("Unknown argument. Use --help. Credentials must never be arguments.".into());
        }
        let value = args.get(i + 1).filter(|v| !v.starts_with("--")).ok_or("Missing option value.")?;
        if options.insert(key.as_str(), value.as_str()).is_some() {
            return Err("Duplicate option.".into());
        }
        i += 2;
    }
    let opt = |name| options.get(name).copied();
    if mode == "parse" {
        if send {
            return Err("parse does not contact a server.".into());
        }
        let result = parse_rowset(&bounded_file(opt("--response-file").ok_or("--response-file is required.")?)?)?;
        println!("{}", serde_json::to_string_pretty(&result).map_err(|_| "Cannot encode rowset.")?);
        return Ok(());
    }
    let (action, body) = match mode.as_str() {
        "discover" => ("Discover", discover(opt("--rowset").unwrap_or("MDSCHEMA_CUBES"), opt("--catalog"), opt("--cube"))?),
        "execute" => {
            let mdx = bounded_file(opt("--mdx-file").ok_or("--mdx-file is required.")?)?;
            ("Execute", execute(opt("--catalog").ok_or("--catalog is required.")?, std::str::from_utf8(&mdx).map_err(|_| "MDX must be UTF-8.")?)?)
        }
        _ => return Err("Choose discover, execute or parse. Use --help.".into()),
    };
    if !send {
        println!("{body}");
        return Ok(());
    }
    let endpoint = std::env::var("GRIDCRAFT_BW_XMLA_URL").map_err(|_| "Set GRIDCRAFT_BW_XMLA_URL locally.")?;
    let response = gridcraft_bw_client::Client::default().start(Request {
        endpoint,
        action,
        body,
        simulator: false,
        canceled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    })?;
    let bytes = response.recv().map_err(|_| "BW transport worker stopped.")??;
    let result = parse_rowset(&bytes)?;
    println!("{}", serde_json::to_string_pretty(&result).map_err(|_| "Cannot encode rowset.")?);
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
