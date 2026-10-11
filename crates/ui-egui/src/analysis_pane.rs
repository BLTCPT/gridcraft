//! Native Analysis task pane. Edits travel through the same engine commands as CLI/MCP.
use egui::Ui;
use gridcraft_engine::analysis::{ConnectionConfig, DIMENSIONS, MEASURES, ProviderKind, QUERY_ID, REGIONS};
use serde_json::json;

use crate::{SheetApp, theme::Tokens};

#[derive(Clone, Debug)]
pub struct AnalysisDraft {
    pub doc_uid: u64,
    pub provider: ProviderKind,
    pub analytical_url: String,
    pub repository_url: String,
    pub fiscal_year: u16,
    pub region: String,
    pub rows: String,
    pub columns: String,
    pub measure: String,
}
impl Default for AnalysisDraft {
    fn default() -> Self {
        Self {
            doc_uid: 0,
            provider: ProviderKind::Fixture,
            analytical_url: String::new(),
            repository_url: String::new(),
            fiscal_year: 2026,
            region: "All regions".into(),
            rows: "Region".into(),
            columns: "Product".into(),
            measure: "Revenue".into(),
        }
    }
}

pub fn ribbon(app: &mut SheetApp, ui: &mut Ui) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if ui.button("Analysis workspace").clicked() {
                app.grid.pane = Some("analysis".into());
            }
            if ui.button("Connect demo").clicked() && app.run("analysis.configure", json!({"provider":"fixture"})).is_ok() {
                app.run_or_alert("analysis.connect", json!({}));
                app.analysis_draft.provider = ProviderKind::Fixture;
                app.analysis_draft.analytical_url.clear();
                app.analysis_draft.repository_url.clear();
                app.grid.pane = Some("analysis".into());
            }
            if ui.button("Refresh analysis").clicked() {
                app.run_or_alert("analysis.refresh", json!({}));
            }
            if ui.button("Swap axes").clicked() {
                app.run_or_alert("analysis.swapAxes", json!({}));
            }
        });
        ui.label("AO-style workflow · Synthetic demo available · BW server validation pending");
        ui.small("BOBJ repository and BW query execution use separate interfaces.");
    });
}

fn choice(ui: &mut Ui, label: &str, value: &mut String, options: &[&str]) {
    ui.label(label);
    egui::ComboBox::from_id_salt(label).selected_text(value.as_str()).width(235.0).show_ui(ui, |ui| {
        for option in options {
            ui.selectable_value(value, (*option).into(), *option);
        }
    });
}

pub fn show(app: &mut SheetApp, ui: &mut Ui) {
    let Some(d) = app.session.active() else {
        ui.label("Open a workbook to use Analysis.");
        return;
    };
    let t = Tokens::get(ui.ctx());
    if app.analysis_draft.doc_uid != d.uid {
        app.analysis_draft = AnalysisDraft {
            doc_uid: d.uid,
            provider: d.analysis_session.config.provider,
            analytical_url: d.analysis_session.config.analytical_url.clone(),
            repository_url: d.analysis_session.config.repository_url.clone(),
            ..Default::default()
        };
        if let Some(b) = &d.wb.analysis {
            app.analysis_draft.fiscal_year = b.definition.fiscal_year;
            app.analysis_draft.region = b.definition.region.clone().unwrap_or_else(|| "All regions".into());
            app.analysis_draft.rows = b.definition.rows.clone();
            app.analysis_draft.columns = b.definition.columns.clone();
            app.analysis_draft.measure = b.definition.measure.clone();
        }
    }
    let connected = d.analysis_session.connected;
    let report = d.wb.analysis.as_ref().map(|b| (b.definition.clone(), b.source_rows, b.refresh_count));
    ui.colored_label(t.accent, egui::RichText::new(if connected { "SYNTHETIC SESSION" } else { "DISCONNECTED" }).strong());
    ui.small("Original demo data only. No SAP server is connected.");
    ui.add_space(8.0);
    ui.collapsing("Connection settings", |ui| {
        egui::ComboBox::from_id_salt("analysis-provider").selected_text(app.analysis_draft.provider.label()).show_ui(ui, |ui| {
            for kind in [ProviderKind::Fixture, ProviderKind::BwXmla, ProviderKind::BwRfc] {
                ui.selectable_value(&mut app.analysis_draft.provider, kind, kind.label());
            }
        });
        if app.analysis_draft.provider != ProviderKind::Fixture {
            ui.label("BW analytical endpoint");
            ui.add(
                egui::TextEdit::singleline(&mut app.analysis_draft.analytical_url)
                    .hint_text("https://bw.example/sap/bw/xml/soap/xmla")
                    .char_limit(2048),
            );
            ui.label("BOBJ repository endpoint (optional)");
            ui.add(egui::TextEdit::singleline(&mut app.analysis_draft.repository_url).hint_text("https://bi.example/biprws").char_limit(2048));
            ui.small("Configuration only. Live adapters are not connected to this pane. Do not enter passwords or tokens.");
        }
        if ui.button("Apply connection settings").clicked() {
            let config = ConnectionConfig {
                provider: app.analysis_draft.provider,
                analytical_url: if app.analysis_draft.provider == ProviderKind::Fixture {
                    String::new()
                } else {
                    app.analysis_draft.analytical_url.clone()
                },
                repository_url: if app.analysis_draft.provider == ProviderKind::Fixture {
                    String::new()
                } else {
                    app.analysis_draft.repository_url.clone()
                },
            };
            app.run_or_alert("analysis.configure", json!(config));
        }
    });
    ui.horizontal(|ui| {
        if ui.add_enabled(!connected, egui::Button::new("Connect")).clicked() {
            app.run_or_alert("analysis.connect", json!({}));
        }
        if ui.add_enabled(connected, egui::Button::new("Disconnect")).clicked() {
            app.run_or_alert("analysis.disconnect", json!({}));
        }
    });
    ui.separator();
    ui.strong("Data source");
    ui.label("Sales by region and product");
    ui.small("demo.sales · 3 dimensions · 2 measures");
    ui.collapsing("Query metadata", |ui| {
        if connected {
            if let Ok(metadata) = app.session.run("analysis.metadata", json!({"query": QUERY_ID})) {
                ui.small(format!("Dimensions: {}", metadata["dimensions"]));
                ui.small("Measures: Revenue (USD), Units · Sum aggregation");
                ui.small("No hierarchies, planning, or SAP workbook semantics.");
            }
        } else {
            ui.small("Connect to discover query metadata.");
        }
    });
    ui.separator();
    ui.strong("Variables");
    ui.label("Fiscal year (required)");
    egui::ComboBox::from_id_salt("analysis-year").selected_text(app.analysis_draft.fiscal_year.to_string()).show_ui(ui, |ui| {
        for year in [2025, 2026] {
            ui.selectable_value(&mut app.analysis_draft.fiscal_year, year, year.to_string());
        }
    });
    let mut regions = vec!["All regions"];
    regions.extend_from_slice(REGIONS);
    choice(ui, "Region filter", &mut app.analysis_draft.region, &regions);
    let region = (app.analysis_draft.region != "All regions").then(|| app.analysis_draft.region.clone());
    if ui.add_enabled(connected && report.is_some(), egui::Button::new("Apply variables & refresh")).clicked() {
        app.run_or_alert("analysis.variables", json!({"fiscalYear": app.analysis_draft.fiscal_year, "region": region}));
    }
    ui.separator();
    ui.strong("Crosstab layout");
    choice(ui, "Rows", &mut app.analysis_draft.rows, DIMENSIONS);
    choice(ui, "Columns", &mut app.analysis_draft.columns, DIMENSIONS);
    choice(ui, "Measure", &mut app.analysis_draft.measure, MEASURES);
    let distinct = app.analysis_draft.rows != app.analysis_draft.columns;
    if !distinct {
        ui.colored_label(t.danger, "Rows and columns need different dimensions.");
    }
    if report.is_none() {
        if ui.add_enabled(connected && distinct, egui::Button::new("Insert analysis")).clicked() {
            app.run_or_alert(
                "analysis.insert",
                json!({"query": QUERY_ID, "fiscalYear": app.analysis_draft.fiscal_year, "region": region,
                "rows": app.analysis_draft.rows, "columns": app.analysis_draft.columns, "measure": app.analysis_draft.measure}),
            );
        }
    } else if ui.add_enabled(connected && distinct, egui::Button::new("Apply layout & refresh")).clicked() {
        app.run_or_alert(
            "analysis.layout",
            json!({"rows": app.analysis_draft.rows, "columns": app.analysis_draft.columns, "measure": app.analysis_draft.measure}),
        );
    }
    if let Some((definition, count, refreshes)) = report {
        ui.add_space(8.0);
        ui.small(format!("Applied: FY {} · {} by {} · {}", definition.fiscal_year, definition.rows, definition.columns, definition.measure));
        ui.small(format!("{count} synthetic records · {refreshes} successful refreshes"));
    }
    ui.separator();
    ui.small(
        "Save preserves the GridCraft query definition, cells and PivotTable. Reconnect after reopening. SAP AO workbook metadata is not supported.",
    );
}

/// Keep applied controls in sync when commands come from the ribbon, undo or the control API.
pub fn sync_applied(app: &mut SheetApp) {
    let Some(d) = app.session.active() else {
        return;
    };
    let mut draft = AnalysisDraft {
        doc_uid: d.uid,
        provider: d.analysis_session.config.provider,
        analytical_url: d.analysis_session.config.analytical_url.clone(),
        repository_url: d.analysis_session.config.repository_url.clone(),
        ..Default::default()
    };
    if let Some(b) = &d.wb.analysis {
        draft.fiscal_year = b.definition.fiscal_year;
        draft.region = b.definition.region.clone().unwrap_or_else(|| "All regions".into());
        draft.rows = b.definition.rows.clone();
        draft.columns = b.definition.columns.clone();
        draft.measure = b.definition.measure.clone();
    }
    app.analysis_draft = draft;
}
