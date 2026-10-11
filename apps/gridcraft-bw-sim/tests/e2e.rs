use gridcraft_bw_sim::{Scenario, Simulator};
use gridcraft_engine::Session;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

fn exec(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute_headless(cmd, p).unwrap()
}
fn setup(sim: &Simulator) -> Session {
    let mut s = Session::new();
    gridcraft_bw_client::install(&mut s);
    s.new_workbook();
    exec(&mut s, "analysis.configure", json!({"provider":"bwSimulator", "analyticalUrl":sim.endpoint}));
    let status = exec(&mut s, "analysis.connect", json!({}));
    assert_eq!(status["connected"], true);
    assert_eq!(status["synthetic"], true);
    assert_eq!(status["live"], false);
    s
}
fn query(sim: &Simulator) -> Value {
    json!({"query":"ZDEMO_SALES", "fiscalYear":0,"rows":"Region","columns":"Product","measure":"Revenue",
        "bw":{"endpoint":sim.endpoint,"catalog":"SYNTHETIC_BW",
        "mdx":"SELECT {[Measures].[Revenue]} ON COLUMNS, CrossJoin([Region].Members,[Product].Members) ON ROWS FROM [ZDEMO_SALES] WHERE ({{YEAR}})",
        "variables":{"YEAR":"[FiscalYear].[2026]"},"unitField":"Currency","simulator":true}})
}
fn source_total(s: &Session) -> f64 {
    let wb = &s.doc().unwrap().wb;
    let b = wb.analysis.as_ref().unwrap();
    let sheet = wb.sheet(wb.sheet_index(&b.data_sheet).unwrap()).unwrap();
    (4..4 + b.source_rows as u32)
        .map(|r| match sheet.cell(gridcraft_core::CellRef::new(r, 2)).unwrap().value {
            gridcraft_core::Value::Number(n) => n,
            _ => 0.0,
        })
        .sum()
}
#[test]
fn tcp_discovery_prompt_pivot_refresh_undo_save_reopen() {
    let sim = Simulator::start(0).unwrap();
    let mut s = setup(&sim);
    for rowset in ["MDSCHEMA_CUBES", "MDSCHEMA_DIMENSIONS", "MDSCHEMA_MEASURES", "MDSCHEMA_SAP_VARIABLES"] {
        let r = exec(&mut s, "analysis.discover", json!({"rowset":rowset,"catalog":"SYNTHETIC_BW","cube":"ZDEMO_SALES"}));
        assert!(!r["rows"].as_array().unwrap().is_empty());
    }
    let mut missing = query(&sim);
    missing["bw"]["variables"] = json!({});
    assert!(s.execute_headless("analysis.insert", missing).is_err());
    exec(&mut s, "analysis.insert", query(&sim));
    assert_eq!(source_total(&s), 2_553_600.0);
    assert_eq!(s.doc().unwrap().wb.active().unwrap().pivots.len(), 1);
    assert!(s.doc().unwrap().wb.active().unwrap().cells.iter().any(|(_, c)| c.value == gridcraft_core::Value::Number(2_553_600.0)));
    exec(&mut s, "analysis.bwVariables", json!({"variables":{"YEAR":"[FiscalYear].[2025]"}}));
    assert_eq!(source_total(&s), 2_349_600.0);
    exec(&mut s, "edit.undo", json!({}));
    assert_eq!(source_total(&s), 2_553_600.0);
    exec(&mut s, "analysis.swapAxes", json!({}));
    assert_eq!(s.doc().unwrap().wb.analysis.as_ref().unwrap().definition.rows, "Product");
    exec(&mut s, "analysis.refresh", json!({}));
    let saved = exec(&mut s, "file.saveBytes", json!({}));
    let mut reopened = Session::new();
    gridcraft_bw_client::install(&mut reopened);
    exec(&mut reopened, "file.open", json!({"name":"synthetic-bw.xlsx","base64":saved["base64"]}));
    assert!(!reopened.doc().unwrap().analysis_session.connected);
    assert!(reopened.execute_headless("analysis.refresh", json!({})).is_err());
    exec(&mut reopened, "analysis.configure", json!({"provider":"bwSimulator","analyticalUrl":sim.endpoint}));
    exec(&mut reopened, "analysis.connect", json!({}));
    exec(&mut reopened, "analysis.refresh", json!({}));
    assert_eq!(source_total(&reopened), 2_553_600.0);
    assert!(sim.requests.load(std::sync::atomic::Ordering::Relaxed) >= 10);
    assert!(s.journal.iter().all(|(name, _)| !name.starts_with("analysis.")));
}
#[test]
fn transport_and_protocol_failures_preserve_report() {
    let sim = Simulator::start(0).unwrap();
    let mut s = setup(&sim);
    exec(&mut s, "analysis.insert", query(&sim));
    for scenario in [
        Scenario::Unauthorized,
        Scenario::Forbidden,
        Scenario::Expired,
        Scenario::Fault,
        Scenario::Malformed,
        Scenario::MixedUnits,
        Scenario::Duplicate,
        Scenario::Redirect,
        Scenario::WrongNamespace,
        Scenario::MissingMeasure,
        Scenario::Multidimensional,
    ] {
        sim.set_scenario(scenario);
        let before = s.doc().unwrap().wb.clone();
        let undo = s.doc().unwrap().undo.len();
        assert!(s.execute_headless("analysis.refresh", json!({})).is_err(), "{scenario:?}");
        assert!(Arc::ptr_eq(&before, &s.doc().unwrap().wb), "{scenario:?}");
        assert_eq!(undo, s.doc().unwrap().undo.len());
    }
    sim.set_scenario(Scenario::Slow);
    s.analysis_transport = Some(Arc::new(gridcraft_bw_client::Client::with_timeout(Duration::from_millis(75))));
    let before = s.doc().unwrap().wb.clone();
    assert!(s.execute_headless("analysis.refresh", json!({})).is_err());
    assert!(Arc::ptr_eq(&before, &s.doc().unwrap().wb));
}
#[test]
fn cancel_disconnect_and_concurrent_edits_discard_late_results() {
    let sim = Simulator::start(0).unwrap();
    let mut s = setup(&sim);
    exec(&mut s, "analysis.insert", query(&sim));
    sim.set_scenario(Scenario::Slow);
    let before = s.doc().unwrap().wb.clone();
    s.execute("analysis.refresh", json!({})).unwrap();
    exec(&mut s, "analysis.cancel", json!({}));
    std::thread::sleep(Duration::from_millis(750));
    exec(&mut s, "analysis.poll", json!({}));
    assert!(Arc::ptr_eq(&before, &s.doc().unwrap().wb));
    s.execute("analysis.refresh", json!({})).unwrap();
    exec(&mut s, "analysis.disconnect", json!({}));
    assert!(s.doc().unwrap().analysis_session.pending.is_none());
    sim.set_scenario(Scenario::Normal);
    exec(&mut s, "analysis.connect", json!({}));
    sim.set_scenario(Scenario::Slow);
    s.execute("analysis.refresh", json!({})).unwrap();
    exec(&mut s, "cell.set", json!({"cell":"H20","input":"keep my work"}));
    let edited = s.doc().unwrap().wb.clone();
    assert!(s.execute_headless("analysis.poll", json!({})).is_err());
    assert!(Arc::ptr_eq(&edited, &s.doc().unwrap().wb));
}
#[test]
fn endpoint_policy_does_not_send_credentials_to_unapproved_origins() {
    let sim = Simulator::start(0).unwrap();
    let mut s = Session::new();
    s.new_workbook();
    gridcraft_bw_client::install(&mut s);
    for url in ["http://example.com/xmla", "http://localhost:8787/xmla", "http://127.0.0.1:8787/biprws", "http://127.0.0.1:8787/xmla?token=x"] {
        assert!(s.execute_headless("analysis.configure", json!({"provider":"bwSimulator","analyticalUrl":url})).is_err());
    }
    exec(&mut s, "analysis.configure", json!({"provider":"bwXmla","analyticalUrl":"https://unapproved.invalid/xmla"}));
    assert!(s.execute_headless("analysis.connect", json!({})).is_err());
    assert_eq!(sim.requests.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn headless_mcp_uses_identical_network_commands() {
    use gridcraft_mcp::{Backend, Headless};
    let sim = Simulator::start(0).unwrap();
    let mut s = Session::new();
    s.new_workbook();
    gridcraft_bw_client::install(&mut s);
    let mut mcp = Headless::with_session(s);
    for (command, params) in [
        ("analysis.configure", json!({"provider":"bwSimulator","analyticalUrl":sim.endpoint})),
        ("analysis.connect", json!({})),
        ("analysis.insert", query(&sim)),
        ("analysis.refresh", json!({})),
    ] {
        mcp.call("engine.execute", json!({"command":command,"params":params})).unwrap();
    }
    assert_eq!(source_total(&mcp.session), 2_553_600.0);
    assert_eq!(mcp.session.doc().unwrap().wb.analysis.as_ref().unwrap().refresh_count, 2);
}
#[test]
fn native_pane_connects_and_refreshes_through_real_http() {
    use egui_kittest::kittest::Queryable;
    use gridcraft_ui_egui::SheetApp;
    let sim = Simulator::start(0).unwrap();
    let mut s = Session::new();
    s.new_workbook();
    gridcraft_bw_client::install(&mut s);
    exec(&mut s, "analysis.configure", json!({"provider":"bwSimulator","analyticalUrl":sim.endpoint}));
    let mut app = SheetApp::new(s, Default::default());
    app.ui.ribbon_tab = "Analysis".into();
    app.grid.pane = Some("analysis".into());
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1600.0, 1400.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h.get_by_label("Connect").click();
    for _ in 0..100 {
        h.run_steps(1);
        if h.state().session.doc().unwrap().analysis_session.connected {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.state().session.doc().unwrap().analysis_session.connected);
    h.get_by_label("Execute & insert BW report").click();
    for _ in 0..100 {
        h.run_steps(1);
        if h.state().session.doc().unwrap().wb.analysis.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(h.state().message.is_none(), "{:?}", h.state().message);
    assert_eq!(source_total(&h.state().session), 2_553_600.0);
    h.get_by_label("Refresh analysis").click();
    for _ in 0..100 {
        h.run_steps(1);
        if h.state().session.doc().unwrap().wb.analysis.as_ref().unwrap().refresh_count == 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(h.state().session.doc().unwrap().wb.analysis.as_ref().unwrap().refresh_count, 2);
    h.get_by_label("Disconnect").click();
    h.run_steps(3);
    assert!(!h.state().session.doc().unwrap().analysis_session.connected);
    assert_eq!(source_total(&h.state().session), 2_553_600.0);
}
