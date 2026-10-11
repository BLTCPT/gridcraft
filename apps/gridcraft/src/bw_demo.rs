//! One-command synthetic network demo. The owned sibling process ends with the app.
use gridcraft_engine::Session;
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read},
    process::{Child, Command, Stdio},
    time::Duration,
};

pub struct Demo(Child);
impl Drop for Demo {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn endpoint(line: &str) -> Result<String, String> {
    let endpoint = line.trim().strip_prefix("SYNTHETIC XMLA SIMULATOR: ").ok_or("Simulator startup response was not recognized.")?;
    if !gridcraft_engine::analysis::network::simulator_endpoint(endpoint) {
        return Err("Simulator did not return a numeric loopback endpoint.".into());
    }
    Ok(endpoint.into())
}
pub fn start(session: &mut Session) -> Result<Demo, String> {
    let executable = std::env::current_exe().map_err(|_| "Cannot locate the demo package.")?;
    let directory = executable.parent().ok_or("Cannot locate the demo directory.")?;
    let mut command = Command::new(directory.join(if cfg!(windows) { "gridcraft-bw-sim.exe" } else { "gridcraft-bw-sim" }));
    command.args(["--port", "0"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    // The synthetic child has no need for live credentials, including inherited environment.
    for key in ["GRIDCRAFT_BW_XMLA_URL", "GRIDCRAFT_BW_USER", "GRIDCRAFT_BW_PASSWORD"] {
        command.env_remove(key);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW; no shell or script policy changes.
    }
    let mut demo =
        Demo(command.spawn().map_err(|_| "Cannot start the synthetic simulator. Keep gridcraft-bw-sim beside gridcraft in the extracted package.")?);
    let stdout = demo.0.stdout.take().ok_or("Simulator startup pipe unavailable.")?;
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("synthetic-demo-start".into())
        .spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout.take(1024))
                .read_line(&mut line)
                .map_err(|_| "Simulator startup read failed.".to_string())
                .and_then(|_| endpoint(&line));
            let _ = send.send(result);
        })
        .map_err(|_| "Cannot start the simulator reader.")?;
    let endpoint = receive.recv_timeout(Duration::from_secs(5)).map_err(|_| "Synthetic simulator startup timed out.")??;
    session.new_workbook();
    session.execute_headless("analysis.configure", json!({"provider":"bwSimulator","analyticalUrl":endpoint})).map_err(|e| e.to_string())?;
    session.execute_headless("analysis.connect", json!({})).map_err(|e| e.to_string())?;
    session.execute_headless("analysis.insert", json!({"query":"ZDEMO_SALES","fiscalYear":0,"rows":"Region","columns":"Product","measure":"Revenue",
        "bw":{"endpoint":endpoint,"catalog":"SYNTHETIC_BW","mdx":"SELECT {[Measures].[Revenue]} ON COLUMNS, CrossJoin([Region].Members,[Product].Members) ON ROWS FROM [ZDEMO_SALES] WHERE ({{YEAR}})","variables":{"YEAR":"[FiscalYear].[2026]"},"unitField":"Currency","simulator":true}})).map_err(|e| e.to_string())?;
    Ok(demo)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child_cannot_select_a_live_destination() {
        assert!(endpoint("SYNTHETIC XMLA SIMULATOR: http://127.0.0.1:45123/xmla\n").is_ok());
        for value in ["https://example.com/xmla", "http://localhost:8787/xmla", "http://127.0.0.1:8787/other", "http://127.0.0.1:8787/xmla?secret=x"]
        {
            assert!(endpoint(&format!("SYNTHETIC XMLA SIMULATOR: {value}")).is_err());
        }
    }
}
