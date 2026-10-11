#![forbid(unsafe_code)]
use gridcraft_bw_sim::{Scenario, Simulator};
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|s| s == "--help") {
        println!(
            "Synthetic local XMLA simulator (not SAP)\n  gridcraft-bw-sim [--port 8787] [--scenario normal|fault|malformed|slow|unauthorized|forbidden|expired|mixed-units|duplicate|redirect|wrong-namespace|missing-measure|multidimensional]\nHTTP is bound to numeric loopback only. Fixed synthetic Basic credentials: synthetic / synthetic.\nCtrl+C stops the process. No BOBJ endpoint is implemented."
        );
        return Ok(());
    }
    let mut port = 8787;
    let mut scenario = Scenario::Normal;
    for pair in args.chunks(2) {
        match pair.first().map(String::as_str) {
            Some("--port") => port = pair.get(1).and_then(|p| p.parse().ok()).ok_or("Invalid port.")?,
            Some("--scenario") => scenario = pair.get(1).and_then(|s| Scenario::parse(s)).ok_or("Invalid scenario.")?,
            _ => return Err("Unknown option. Use --help.".into()),
        }
    }
    let sim = Simulator::start(port)?;
    sim.set_scenario(scenario);
    println!("SYNTHETIC XMLA SIMULATOR: {}", sim.endpoint);
    loop {
        std::thread::park_timeout(std::time::Duration::from_secs(60));
    }
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}
