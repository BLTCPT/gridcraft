use std::sync::Arc;

use gridcraft_engine::{
    Session,
    core::{CellRef, Value},
};
use serde_json::{Value as Json, json};

fn blank() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s
}
fn connected() -> Session {
    let mut s = blank();
    s.execute("analysis.configure", json!({"provider":"fixture"})).unwrap();
    s.execute("analysis.connect", json!({})).unwrap();
    s
}
fn query() -> Json {
    json!({"query":"demo.sales","fiscalYear":2026,"rows":"Region","columns":"Product","measure":"Revenue"})
}
fn inserted() -> Session {
    let mut s = connected();
    s.execute("analysis.insert", query()).unwrap();
    s
}
fn total(s: &Session) -> f64 {
    s.doc()
        .unwrap()
        .wb
        .active()
        .unwrap()
        .cells
        .iter()
        .filter_map(|(_, c)| if let Value::Number(n) = c.value { Some(n) } else { None })
        .fold(0.0, f64::max)
}

#[test]
fn discovery_separates_repository_objects_from_queries() {
    let mut s = connected();
    assert_eq!(s.execute("analysis.queries", json!({})).unwrap()["queries"][0]["id"], "demo.sales");
    assert_eq!(s.execute("analysis.workbooks", json!({})).unwrap()["items"][0]["kind"], "syntheticExample");
    let m = s.execute("analysis.metadata", json!({"query":"demo.sales"})).unwrap();
    assert_eq!(m["variables"][0]["required"], true);
    assert!(s.execute("analysis.metadata", json!({"query":"unknown"})).is_err());
}
#[test]
fn insert_aggregates_fixture_and_preserves_existing_work() {
    let mut s = connected();
    s.execute("cell.set", json!({"cell":"A1","input":"Keep my work"})).unwrap();
    let r = s.execute("analysis.insert", query()).unwrap();
    assert_eq!(r["sourceRows"], 24);
    assert_eq!(total(&s), 2_553_600.0);
    assert_eq!(s.doc().unwrap().wb.sheet(0).unwrap().value(CellRef::new(0, 0)), Value::text("Keep my work"));
    assert_eq!(s.doc().unwrap().wb.sheets.len(), 3);
    assert_eq!(s.doc().unwrap().wb.active().unwrap().pivots.len(), 1);
}
#[test]
fn prompts_and_layout_refresh_with_correct_totals_and_clear_previous_rows() {
    let mut s = inserted();
    s.execute("analysis.variables", json!({"fiscalYear":2026,"region":"Americas"})).unwrap();
    assert_eq!(total(&s), 579_200.0);
    let b = s.doc().unwrap().wb.analysis.as_ref().unwrap();
    assert_eq!(b.source_rows, 8);
    assert_eq!(b.expected_data.used_range().unwrap().end.row, 11);
    s.execute("analysis.layout", json!({"rows":"Quarter","columns":"Product","measure":"Units"})).unwrap();
    assert_eq!(total(&s), 704.0);
    s.execute("analysis.swapAxes", json!({})).unwrap();
    assert_eq!(total(&s), 704.0);
    assert_eq!(s.doc().unwrap().wb.analysis.as_ref().unwrap().definition.rows, "Product");
}
#[test]
fn invalid_prompt_or_layout_preserves_results_and_history() {
    let mut s = inserted();
    let before = s.doc().unwrap().wb.clone();
    let history = s.doc().unwrap().undo.len();
    for (id, p) in [
        ("analysis.variables", json!({"fiscalYear":9999})),
        ("analysis.variables", json!({"fiscalYear":2026,"region":"Unknown"})),
        ("analysis.layout", json!({"rows":"Region","columns":"Region","measure":"Revenue"})),
        ("analysis.layout", json!({"rows":"Region","columns":"Product","measure":"Margin"})),
    ] {
        assert!(s.execute(id, p).is_err());
        assert!(Arc::ptr_eq(&before, &s.doc().unwrap().wb));
        assert_eq!(s.doc().unwrap().undo.len(), history);
    }
}
#[test]
fn refresh_is_atomic_and_undo_restores_definition_and_results() {
    let mut s = inserted();
    s.execute("analysis.variables", json!({"fiscalYear":2025})).unwrap();
    assert_eq!(total(&s), 2_349_600.0);
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(total(&s), 2_553_600.0);
    assert_eq!(s.doc().unwrap().wb.analysis.as_ref().unwrap().definition.fiscal_year, 2026);
    s.execute("analysis.refresh", json!({})).unwrap();
    assert_eq!(total(&s), 2_553_600.0);
    assert_eq!(s.doc().unwrap().wb.sheets.len(), 3);
}
#[test]
fn local_edits_cannot_be_overwritten_by_refresh() {
    let mut s = inserted();
    s.execute("cell.set", json!({"cell":"B7","input":"My edit"})).unwrap();
    let before = s.doc().unwrap().wb.clone();
    assert!(s.execute("analysis.refresh", json!({})).unwrap_err().to_string().contains("preserve"));
    assert!(Arc::ptr_eq(&before, &s.doc().unwrap().wb));
    s.execute("edit.undo", json!({})).unwrap();
    s.execute("analysis.refresh", json!({})).unwrap();
}
#[test]
fn disconnect_retains_snapshot_but_disables_refresh() {
    let mut s = inserted();
    s.execute("analysis.disconnect", json!({})).unwrap();
    let before = s.doc().unwrap().wb.clone();
    assert!(s.execute("analysis.refresh", json!({})).is_err());
    assert!(Arc::ptr_eq(&before, &s.doc().unwrap().wb));
    s.execute("analysis.connect", json!({})).unwrap();
    s.execute("analysis.refresh", json!({})).unwrap();
}
#[test]
fn live_configuration_never_claims_connection_or_logs_credentials() {
    let mut s = connected();
    let secret = "never-save-this-secret";
    let err = s.execute("analysis.configure", json!({"provider":"fixture","password":secret})).unwrap_err().to_string();
    assert!(!err.contains(secret));
    assert!(s.doc().unwrap().analysis_session.connected);
    for endpoint in ["http://example.com", "https://user:password@example.com", "https://example.com/?token=secret"] {
        assert!(s.execute("analysis.configure", json!({"provider":"bwXmla","analyticalUrl":endpoint})).is_err());
    }
    s.execute(
        "analysis.configure",
        json!({"provider":"bwXmla","analyticalUrl":"https://bw.example/sap/bw/xml/soap/xmla","repositoryUrl":"https://bi.example/biprws"}),
    )
    .unwrap();
    assert!(s.execute("analysis.connect", json!({})).unwrap_err().to_string().contains("no XMLA transport"));
    assert!(!s.doc().unwrap().analysis_session.connected);
    assert!(!serde_json::to_string(&s.journal).unwrap().contains("analyticalUrl"));
    assert!(!serde_json::to_string(&s.doc().unwrap().wb).unwrap().contains("bw.example"));
}
#[test]
fn sessions_are_isolated_per_document_and_missing_variables_are_errors() {
    let mut s = inserted();
    s.new_workbook();
    assert!(!s.doc().unwrap().analysis_session.connected);
    assert!(s.doc().unwrap().wb.analysis.is_none());
    s.execute("analysis.connect", json!({})).unwrap();
    assert!(s.execute("analysis.insert", json!({"query":"demo.sales"})).is_err());
}
#[test]
fn protected_workbook_rejects_insert_without_partial_sheets() {
    let mut s = connected();
    Arc::make_mut(&mut s.doc_mut().unwrap().wb).protected_structure = true;
    let before = s.doc().unwrap().wb.clone();
    assert!(s.execute("analysis.insert", query()).is_err());
    assert!(Arc::ptr_eq(&before, &s.doc().unwrap().wb));
}
#[test]
fn json_snapshot_does_not_serialize_analytical_binding_or_sessions() {
    let s = inserted();
    let snapshot = serde_json::to_value(&s.doc().unwrap().wb).unwrap();
    assert!(snapshot.get("analysis").is_none());
    assert!(snapshot.get("analysis_session").is_none());
}

#[test]
fn xlsx_roundtrip_preserves_gridcraft_definition_without_reconnecting() {
    let s = inserted();
    let bytes = gridcraft_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    let (wb, report) = gridcraft_xlsx::read_xlsx(&bytes).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(wb.analysis.as_ref().unwrap().definition.fiscal_year, 2026);
    assert!(wb.analysis.as_ref().unwrap().refresh_safe);
    let mut reopened = Session::new();
    reopened.add_document(gridcraft_engine::DocState::new(wb, None, "Roundtrip".into()));
    assert!(reopened.execute("analysis.refresh", json!({})).is_err());
    reopened.execute("analysis.connect", json!({})).unwrap();
    reopened.execute("analysis.refresh", json!({})).unwrap();
    assert_eq!(total(&reopened), 2_553_600.0);
}
#[test]
fn saved_user_edits_cannot_be_overwritten_after_reopening() {
    let mut s = inserted();
    s.execute("cell.set", json!({"cell":"B7","input":"Keep saved edit"})).unwrap();
    let bytes = gridcraft_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    let (wb, report) = gridcraft_xlsx::read_xlsx(&bytes).unwrap();
    assert!(!wb.analysis.as_ref().unwrap().refresh_safe);
    assert!(!report.warnings.is_empty());
    let mut reopened = Session::new();
    reopened.add_document(gridcraft_engine::DocState::new(wb, None, "Roundtrip".into()));
    reopened.execute("analysis.connect", json!({})).unwrap();
    assert!(reopened.execute("analysis.refresh", json!({})).is_err());
}

#[test]
fn unknown_or_corrupt_xlsx_definition_keeps_cached_values() {
    use std::io::{Cursor, Read, Write};
    let s = inserted();
    let bytes = gridcraft_xlsx::write_xlsx(&s.doc().unwrap().wb).unwrap();
    for replacement in [b"{invalid".to_vec(), b"{\"schemaVersion\":99}".to_vec()] {
        let mut input = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..input.len() {
            let mut file = input.by_index(i).unwrap();
            let name = file.name().to_string();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            if name == "xl/gridcraft/analysis.json" {
                data = replacement.clone();
            }
            output.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            output.write_all(&data).unwrap();
        }
        let modified = output.finish().unwrap().into_inner();
        let (wb, report) = gridcraft_xlsx::read_xlsx(&modified).unwrap();
        assert!(wb.analysis.is_none());
        assert_eq!(wb.sheets.len(), 3);
        assert!(!report.warnings.is_empty());
        assert!(wb.active().unwrap().cells.iter().any(|(_, c)| c.value == Value::Number(2_553_600.0)));
    }
}

#[test]
fn xmla_operations_are_commands_and_never_send_requests() {
    let mut s = Session::new();
    let out = s.execute("analysis.xmla.discoverRequest", json!({"rowset":"MDSCHEMA_CUBES","catalog":"BW"})).unwrap();
    assert_eq!(out["networkSent"], false);
    assert!(out["xml"].as_str().unwrap().contains("<Catalog>BW</Catalog>"));
    assert!(s.execute("analysis.xmla.executeRequest", json!({"catalog":"BW","mdx":"UPDATE CUBE X"})).is_err());
    assert!(s.journal.is_empty());
}
