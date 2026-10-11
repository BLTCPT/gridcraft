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
    pub catalog: String,
    pub mdx: String,
    pub member: String,
    pub unit_field: String,
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
            catalog: "SYNTHETIC_BW".into(),
            mdx:
                "SELECT {[Measures].[Revenue]} ON COLUMNS, CrossJoin([Region].Members,[Product].Members) ON ROWS FROM [ZDEMO_SALES] WHERE ({{YEAR}})"
                    .into(),
            member: "[FiscalYear].[2026]".into(),
            unit_field: "Currency".into(),
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
        ui.label("AO-style workflow · Local simulator available · BW server validation pending");
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
            if let Some(bw) = &b.definition.bw {
                app.analysis_draft.catalog = bw.catalog.clone();
                app.analysis_draft.mdx = bw.mdx.clone();
                app.analysis_draft.member = bw.variables.get("YEAR").cloned().unwrap_or_default();
                app.analysis_draft.unit_field = bw.unit_field.clone().unwrap_or_default();
            }
        }
    }
    let connected = d.analysis_session.connected;
    let pending = d.analysis_session.pending.is_some();
    let provider = d.analysis_session.config.provider;
    let metadata = d.analysis_session.catalog.clone();
    let report = d.wb.analysis.as_ref().map(|b| (b.definition.clone(), b.source_rows, b.refresh_count));
    ui.colored_label(
        t.accent,
        egui::RichText::new(if pending {
            "REQUEST RUNNING"
        } else if !connected {
            "DISCONNECTED"
        } else if provider == ProviderKind::BwXmla {
            "BW XMLA SESSION"
        } else {
            "SYNTHETIC SESSION"
        })
        .strong(),
    );
    ui.small(if provider == ProviderKind::BwXmla {
        "Experimental XMLA subset; validate against your BW server."
    } else {
        "Original synthetic data. Not an SAP server."
    });
    ui.add_space(8.0);
    ui.collapsing("Connection settings", |ui| {
        egui::ComboBox::from_id_salt("analysis-provider").selected_text(app.analysis_draft.provider.label()).show_ui(ui, |ui| {
            for kind in [ProviderKind::Fixture, ProviderKind::BwSimulator, ProviderKind::BwXmla, ProviderKind::BwRfc] {
                ui.selectable_value(&mut app.analysis_draft.provider, kind, kind.label());
            }
        });
        if app.analysis_draft.provider == ProviderKind::BwRfc {
            ui.small("Leading production path, pending landscape confirmation. Requires a company-provided licensed RFC SDK, verified OLAP BAPIs and approved authentication. No SDK is loaded; Connect reports unavailable. HTTP Basic/XMLA does not implement RFC/SNC.");
        } else if app.analysis_draft.provider != ProviderKind::Fixture {
            ui.label("BW analytical endpoint");
            ui.add(
                egui::TextEdit::singleline(&mut app.analysis_draft.analytical_url)
                    .hint_text("https://bw.example/sap/bw/xml/soap/xmla")
                    .char_limit(2048),
            );
            ui.label("BOBJ repository endpoint (optional)");
            ui.add(egui::TextEdit::singleline(&mut app.analysis_draft.repository_url).hint_text("https://bi.example/biprws").char_limit(2048));
            ui.small("Live BW: approve the exact URL and credentials in the local launch environment. No credentials belong here. BOBJ repository access is separate and not implemented.");
        }
        if ui.button("Apply connection settings").clicked() {
            let config = ConnectionConfig {
                provider: app.analysis_draft.provider,
                analytical_url: if matches!(app.analysis_draft.provider, ProviderKind::Fixture | ProviderKind::BwRfc) {
                    String::new()
                } else {
                    app.analysis_draft.analytical_url.clone()
                },
                repository_url: if matches!(app.analysis_draft.provider, ProviderKind::Fixture | ProviderKind::BwRfc) {
                    String::new()
                } else {
                    app.analysis_draft.repository_url.clone()
                },
            };
            app.run_or_alert("analysis.configure", json!(config));
        }
    });
    ui.horizontal(|ui| {
        if ui.add_enabled(!connected && !pending, egui::Button::new("Connect")).clicked() {
            app.run_or_alert("analysis.connect", json!({}));
        }
        if ui.add_enabled(connected || pending, egui::Button::new("Disconnect")).clicked() {
            app.run_or_alert("analysis.disconnect", json!({}));
        }
    });
    if pending && ui.button("Cancel request").clicked() {
        app.run_or_alert("analysis.cancel", json!({}));
    }
    if provider == ProviderKind::BwRfc {
        ui.separator();
        ui.strong("RFC adapter unavailable");
        ui.small("Transport, authentication and analytical functions are separate. BICS is not a public SDK surface. See the RFC adapter design for prerequisites; the local XMLA simulator tests only the alternative XMLA path.");
        return;
    }
    if provider != ProviderKind::Fixture {
        show_bw(app, ui, connected, pending, report.is_some(), metadata);
        return;
    }
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
    let mut draft = if app.analysis_draft.doc_uid == d.uid { app.analysis_draft.clone() } else { AnalysisDraft::default() };
    draft = AnalysisDraft {
        doc_uid: d.uid,
        provider: d.analysis_session.config.provider,
        analytical_url: d.analysis_session.config.analytical_url.clone(),
        repository_url: d.analysis_session.config.repository_url.clone(),
        ..draft
    };
    if let Some(b) = &d.wb.analysis {
        draft.fiscal_year = b.definition.fiscal_year;
        draft.region = b.definition.region.clone().unwrap_or_else(|| "All regions".into());
        draft.rows = b.definition.rows.clone();
        draft.columns = b.definition.columns.clone();
        draft.measure = b.definition.measure.clone();
        if let Some(bw) = &b.definition.bw {
            draft.catalog = bw.catalog.clone();
            draft.mdx = bw.mdx.clone();
            draft.member = bw.variables.get("YEAR").cloned().unwrap_or_default();
            draft.unit_field = bw.unit_field.clone().unwrap_or_default();
        }
    }
    app.analysis_draft = draft;
}

fn show_bw(app: &mut SheetApp, ui: &mut Ui, connected: bool, pending: bool, has_report: bool, metadata: serde_json::Value) {
    ui.separator();
    ui.strong("BW XMLA · read-only");
    ui.small("Start gridcraft-bw-sim, select Local XMLA simulator, and set http://127.0.0.1:8787/xmla to test without SAP.");
    ui.label("Technical catalog");
    ui.add(egui::TextEdit::singleline(&mut app.analysis_draft.catalog).char_limit(256));
    ui.horizontal(|ui| {
        if ui.add_enabled(connected && !pending, egui::Button::new("Discover cubes")).clicked() {
            app.run_or_alert("analysis.discover", json!({"rowset":"MDSCHEMA_CUBES", "catalog":app.analysis_draft.catalog}));
        }
        if ui.add_enabled(connected && !pending, egui::Button::new("Discover variables")).clicked() {
            app.run_or_alert("analysis.discover", json!({"rowset":"MDSCHEMA_SAP_VARIABLES", "catalog":app.analysis_draft.catalog}));
        }
    });
    ui.collapsing("Last discovery response", |ui| {
        egui::ScrollArea::vertical().max_height(130.0).show(ui, |ui| {
            ui.small(serde_json::to_string_pretty(&metadata).unwrap_or_default());
        });
    });
    ui.label("Approved MDX SELECT (tabular result)");
    ui.add(egui::TextEdit::multiline(&mut app.analysis_draft.mdx).desired_rows(4).char_limit(32_768));
    ui.label("YEAR member placeholder (optional)");
    ui.add(egui::TextEdit::singleline(&mut app.analysis_draft.member).char_limit(256));
    ui.small("Substitutes {{YEAR}} with a single member unique name. This does not clear or process BW variables/defaults/exits.");
    for (label, value) in [
        ("Row field", &mut app.analysis_draft.rows),
        ("Column field", &mut app.analysis_draft.columns),
        ("Numeric measure field", &mut app.analysis_draft.measure),
        ("Unit/currency field (optional)", &mut app.analysis_draft.unit_field),
    ] {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).char_limit(256));
    }
    ui.small("Use technical rowset fields and additive leaf results only. Mixed units and duplicate tuples are rejected. Hierarchical axes and exception aggregation are unsupported.");
    if !has_report && ui.add_enabled(connected && !pending, egui::Button::new("Execute & insert BW report")).clicked() {
        let d = &app.analysis_draft;
        let variables = if d.member.is_empty() { json!({}) } else { json!({"YEAR": d.member}) };
        let params = json!({"query": "XMLA SELECT", "fiscalYear": 0, "rows":d.rows,"columns":d.columns,"measure":d.measure,
            "bw":{"endpoint":d.analytical_url,"catalog":d.catalog,"mdx":d.mdx,"variables":variables,
                "unitField":(!d.unit_field.is_empty()).then_some(&d.unit_field),"simulator":d.provider == ProviderKind::BwSimulator}});
        app.run_or_alert("analysis.insert", params);
    }
    if has_report {
        if ui.add_enabled(connected && !pending, egui::Button::new("Apply YEAR member & refresh")).clicked() {
            app.run_or_alert("analysis.bwVariables", json!({"variables":{"YEAR":app.analysis_draft.member}}));
        }
        if ui.add_enabled(connected && !pending, egui::Button::new("Apply field layout & refresh")).clicked() {
            app.run_or_alert(
                "analysis.layout",
                json!({"rows":app.analysis_draft.rows,"columns":app.analysis_draft.columns,"measure":app.analysis_draft.measure}),
            );
        }
        ui.small("Refresh uses the saved query and exact endpoint. Reconnect explicitly after reopening an XLSX. Edit a new workbook to change the MDX/catalog.");
    }
}
