use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;

#[test]
fn native_analysis_workflow_inserts_swaps_and_disconnects_via_commands() {
    let mut session = Session::new();
    session.new_workbook();
    let mut app = SheetApp::new(session, Default::default());
    app.ui.ribbon_tab = "Analysis".into();
    app.grid.pane = Some("analysis".into());
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 1100.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(4);
    h.get_by_label("Connect demo").click();
    h.run_steps(3);
    assert!(h.state().session.doc().unwrap().analysis_session.connected);
    h.get_by_label("Insert analysis").click();
    h.run_steps(4);
    assert!(h.state().session.doc().unwrap().wb.analysis.is_some());
    assert!(h.state().message.is_none(), "{:?}", h.state().message);
    h.get_by_label("Swap axes").click();
    h.run_steps(4);
    assert_eq!(h.state().session.doc().unwrap().wb.analysis.as_ref().unwrap().definition.rows, "Product");
    assert_eq!(h.state().analysis_draft.rows, "Product");
    assert_eq!(h.state().analysis_draft.columns, "Region");
    h.get_by_label("Disconnect").click();
    h.run_steps(3);
    assert!(!h.state().session.doc().unwrap().analysis_session.connected);
    assert!(h.state().session.doc().unwrap().wb.analysis.is_some());
}
