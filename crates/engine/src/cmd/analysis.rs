//! AO-inspired analytical workflow using the existing command, COW history and pivot engine.
use std::sync::Arc;

use gridcraft_core::{CellRef, Value};
use gridcraft_model::analysis::{AnalysisBinding, AnalysisQuery};
use gridcraft_model::{Color, LineInfo, PivotField, PivotLayout, PivotTable, PivotValue, Sheet, Style, Workbook};
use serde_json::{Value as Json, json};

use super::*;
use crate::analysis::{AnalysisSession, AnalyticalProvider, ConnectionConfig, FixtureRepository, ProviderKind, WorkbookRepository};

pub fn specs() -> Vec<CommandSpec> {
    let mut v = vec![
        cmd!(query "analysis.xmla.discoverRequest", "Build BW XMLA Discovery", [], None, "{rowset, catalog?, cube?} → SOAP request only; no network", always, xmla_discover),
        cmd!(query "analysis.xmla.executeRequest", "Build BW XMLA Query", [], None, "{catalog, mdx} → read-only SOAP request only; no network", always, xmla_execute),
        cmd!(query "analysis.xmla.parse", "Read BW XMLA Rowset", [], None, "{xml} → bounded tabular rowset; does not import into a workbook", always, xmla_parse),
        cmd!(query "analysis.configure", "Configure Analytics", ["Analysis", "Connection"], None, "{provider: fixture|bwXmla|bwRfc|hanaIna, analyticalUrl?, repositoryUrl?}; no credentials", has_doc, configure),
        cmd!(query "analysis.connect", "Connect", ["Analysis", "Connection"], None, "{}; only the synthetic provider is available", has_doc, connect),
        cmd!(query "analysis.disconnect", "Disconnect", ["Analysis", "Connection"], None, "{}", has_doc, disconnect),
        cmd!(query "analysis.status", "Analysis Status", [], None, "{} → configuration, connection and report state (no secrets)", has_doc, status),
        cmd!(query "analysis.queries", "Query Catalog", ["Analysis", "Data Source"], None, "{} → synthetic query catalog", has_doc, queries),
        cmd!(query "analysis.metadata", "Query Metadata", [], None, "{query} → dimensions, measures and variables", has_doc, metadata),
        cmd!(query "analysis.workbooks", "Repository Workbooks", [], None, "{} → synthetic repository examples; NOT analytical query sources", has_doc, workbooks),
        cmd!(
            "analysis.insert",
            "Insert Analysis",
            ["Analysis", "Data Source"],
            None,
            "{query, fiscalYear, region?, rows, columns, measure}; creates two new sheets",
            has_doc,
            insert
        ),
        cmd!("analysis.refresh", "Refresh Analysis", ["Analysis", "Data"], None, "{}; refresh current report atomically", has_doc, refresh),
        cmd!(
            "analysis.variables",
            "Apply Variables",
            ["Analysis", "Data"],
            None,
            "{fiscalYear, region?}; validates and refreshes atomically",
            has_doc,
            variables
        ),
        cmd!(
            "analysis.layout",
            "Change Analysis Layout",
            ["Analysis", "Layout"],
            None,
            "{rows, columns, measure}; validates and refreshes atomically",
            has_doc,
            layout
        ),
        cmd!("analysis.swapAxes", "Swap Axes", ["Analysis", "Layout"], None, "{}", has_doc, swap_axes),
    ];
    // Connection configuration never enters the replay journal. Failed/unknown secret params
    // must not be reflected in errors, journals, workbooks, or logs.
    for c in &mut v {
        c.journal = false;
    }
    v
}

fn strict<T: serde::de::DeserializeOwned>(p: &Json, cmd: &str) -> Result<T> {
    serde_json::from_value(p.clone())
        .map_err(|_| bad(cmd, "Missing, unknown or incorrectly typed parameters. See app.commands; do not pass credentials."))
}
fn no_params(p: &Json, cmd: &str) -> Result<()> {
    if p.as_object().is_some_and(|o| o.is_empty()) { Ok(()) } else { Err(bad(cmd, "This command accepts an empty object only.")) }
}
fn failure(message: impl Into<String>) -> EngineError {
    EngineError::Other(message.into())
}

fn configure(s: &mut Session, p: &Json) -> Result<Json> {
    let config: ConnectionConfig = strict(p, "analysis.configure")?;
    config.validate().map_err(failure)?;
    s.doc_mut()?.analysis_session = AnalysisSession { config, connected: false };
    status(s, &json!({}))
}
fn connect(s: &mut Session, p: &Json) -> Result<Json> {
    no_params(p, "analysis.connect")?;
    let state = &mut s.doc_mut()?.analysis_session;
    state.config.validate().map_err(failure)?;
    if state.config.provider != ProviderKind::Fixture {
        return Err(failure(AnalysisSession::unsupported(state.config.provider)));
    }
    state.connected = true;
    status(s, &json!({}))
}
fn disconnect(s: &mut Session, p: &Json) -> Result<Json> {
    no_params(p, "analysis.disconnect")?;
    s.doc_mut()?.analysis_session.connected = false;
    status(s, &json!({}))
}
fn status(s: &mut Session, p: &Json) -> Result<Json> {
    no_params(p, "analysis.status")?;
    let d = s.doc()?;
    Ok(json!({"config": d.analysis_session.config, "connected": d.analysis_session.connected,
        "live": false, "synthetic": d.analysis_session.config.provider == ProviderKind::Fixture,
        "report": d.wb.analysis.as_ref().map(|b| json!({"definition": b.definition,
            "dataSheet": b.data_sheet, "reportSheet": b.report_sheet, "refreshCount": b.refresh_count, "sourceRows": b.source_rows})),
        "persistence": "XLSX stores the GridCraft analysis definition and values. Reconnect explicitly after reopening. SAP AO metadata is unsupported."}))
}
fn queries(s: &mut Session, p: &Json) -> Result<Json> {
    no_params(p, "analysis.queries")?;
    Ok(s.doc()?.analysis_session.fixture().map_err(failure)?.queries())
}
fn metadata(s: &mut Session, p: &Json) -> Result<Json> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        query: String,
    }
    let q: Params = strict(p, "analysis.metadata")?;
    s.doc()?.analysis_session.fixture().map_err(failure)?.metadata(&q.query).map_err(failure)
}
fn workbooks(s: &mut Session, p: &Json) -> Result<Json> {
    no_params(p, "analysis.workbooks")?;
    s.doc()?.analysis_session.fixture().map_err(failure)?;
    Ok(FixtureRepository.workbooks())
}
fn insert(s: &mut Session, p: &Json) -> Result<Json> {
    if s.doc()?.wb.analysis.is_some() {
        return Err(failure("This workbook already has an analysis. Open a new workbook to insert another."));
    }
    let definition: AnalysisQuery = strict(p, "analysis.insert")?;
    render(s, definition, false)
}
fn current(s: &Session) -> Result<AnalysisQuery> {
    s.doc()?.wb.analysis.as_ref().map(|b| b.definition.clone()).ok_or_else(|| failure("Insert an analysis into this workbook first."))
}
fn refresh(s: &mut Session, p: &Json) -> Result<Json> {
    no_params(p, "analysis.refresh")?;
    render(s, current(s)?, true)
}
fn variables(s: &mut Session, p: &Json) -> Result<Json> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Params {
        fiscal_year: u16,
        #[serde(default)]
        region: Option<String>,
    }
    let args: Params = strict(p, "analysis.variables")?;
    let mut q = current(s)?;
    q.fiscal_year = args.fiscal_year;
    q.region = args.region;
    render(s, q, true)
}
fn layout(s: &mut Session, p: &Json) -> Result<Json> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        rows: String,
        columns: String,
        measure: String,
    }
    let args: Params = strict(p, "analysis.layout")?;
    let mut q = current(s)?;
    q.rows = args.rows;
    q.columns = args.columns;
    q.measure = args.measure;
    render(s, q, true)
}
fn swap_axes(s: &mut Session, p: &Json) -> Result<Json> {
    no_params(p, "analysis.swapAxes")?;
    let mut q = current(s)?;
    std::mem::swap(&mut q.rows, &mut q.columns);
    render(s, q, true)
}

fn fresh_sheet(wb: &Workbook, prefix: &str) -> String {
    (1..).map(|n| format!("{prefix} {n}")).find(|n| wb.sheet_index(n).is_none()).unwrap_or_else(|| wb.next_sheet_name())
}

/// View state (scroll, zoom, current selection) is harmless; everything else is guarded.
fn unedited(actual: &Sheet, expected: &Sheet) -> bool {
    let mut actual = actual.clone();
    actual.view_active = expected.view_active;
    actual.view_top_left = expected.view_top_left;
    actual.zoom = expected.zoom;
    actual == *expected
}

fn render(s: &mut Session, definition: AnalysisQuery, refreshing: bool) -> Result<Json> {
    let rows = s.doc()?.analysis_session.fixture().map_err(failure)?.execute(&definition).map_err(failure)?;
    edit(s, |cx| {
        if cx.wb.protected_structure {
            return Err(failure("Unprotect the workbook structure before changing its analysis."));
        }
        let old = cx.wb.analysis.clone();
        let (data_name, report_name, generation) = if refreshing {
            let b = old.as_ref().ok_or_else(|| failure("No analytical binding remains in this workbook."))?;
            let data = cx.wb.sheet_index(&b.data_sheet).and_then(|i| cx.wb.sheet(i));
            let report = cx.wb.sheet_index(&b.report_sheet).and_then(|i| cx.wb.sheet(i));
            if !b.refresh_safe || !data.is_some_and(|s| unedited(s, &b.expected_data)) || !report.is_some_and(|s| unedited(s, &b.expected_report)) {
                return Err(failure(
                    "Analysis sheets were edited, renamed or deleted. Refresh was canceled to preserve your changes. Undo those edits or start an analysis in a new workbook.",
                ));
            }
            (b.data_sheet.clone(), b.report_sheet.clone(), b.refresh_count.saturating_add(1))
        } else {
            (fresh_sheet(&cx.wb, "Analysis Data"), fresh_sheet(&cx.wb, "Analysis Report"), 1)
        };
        let mut data = Sheet::new(&data_name);
        let mut report = Sheet::new(&report_name);
        for sheet in [&mut data, &mut report] {
            sheet.default_col_width = 125.0;
            sheet.freeze = Some((4, 1));
            sheet.cols.insert(0, LineInfo { size: Some(180.0), ..Default::default() });
            sheet.merges.push(gridcraft_core::RangeRef::new(CellRef::new(0, 0), CellRef::new(0, 6)));
            sheet.merges.push(gridcraft_core::RangeRef::new(CellRef::new(1, 0), CellRef::new(1, 6)));
        }
        data.set_value(CellRef::new(0, 0), Value::text("SYNTHETIC DEMO | Original fixture data"));
        report.set_value(CellRef::new(0, 0), Value::text("SYNTHETIC DEMO | Sales analysis"));
        let subtitle = format!(
            "FY {} | {} | {} | Refresh {} | USD revenue",
            definition.fiscal_year,
            definition.region.as_deref().unwrap_or("All regions"),
            definition.measure,
            generation
        );
        report.set_value(CellRef::new(1, 0), Value::text(subtitle));
        for (ci, header) in ["Region", "Product", "Quarter", "Revenue", "Units"].iter().enumerate() {
            data.set_value(CellRef::new(3, ci as u32), Value::text(*header));
        }
        for (ri, row) in rows.iter().enumerate() {
            for (ci, value) in row.iter().enumerate() {
                data.set_value(CellRef::new(ri as u32 + 4, ci as u32), value.clone());
            }
        }
        let mut title = Style::default();
        title.font.bold = true;
        title.font.size = 14.0;
        title.font.color = Color::Rgb(0x107C41);
        let title = cx.wb.styles.intern(title);
        data.set_style(CellRef::new(0, 0), title);
        report.set_style(CellRef::new(0, 0), title);
        let data_idx = cx.wb.sheet_index(&data_name).unwrap_or(cx.wb.sheets.len());
        if data_idx == cx.wb.sheets.len() {
            cx.wb.sheets.push(Arc::new(data));
        } else if let Some(slot) = cx.wb.sheets.get_mut(data_idx) {
            *slot = Arc::new(data);
        }
        let report_idx = cx.wb.sheet_index(&report_name).unwrap_or(cx.wb.sheets.len());
        let pivot_id = old.as_ref().and_then(|b| b.expected_report.pivots.first().map(|p| p.id)).unwrap_or_else(|| cx.wb.next_object_id());
        let pivot_name = if let Some(p) = old.as_ref().and_then(|b| b.expected_report.pivots.first()) {
            p.name.clone()
        } else {
            (1..)
                .map(|n| format!("AnalysisPivot{n}"))
                .find(|name| !cx.wb.sheets.iter().flat_map(|s| &s.pivots).any(|p| p.name == *name))
                .unwrap_or_else(|| format!("AnalysisPivot{pivot_id}"))
        };
        report.pivots.push(PivotTable {
            id: pivot_id,
            name: pivot_name,
            source: format!("{}!A4:E{}", gridcraft_formula::quote_sheet(&data_name), rows.len() + 4),
            anchor: CellRef::new(4, 0),
            rows: vec![PivotField { source_col: definition.rows.clone(), ..Default::default() }],
            columns: vec![PivotField { source_col: definition.columns.clone(), ..Default::default() }],
            values: vec![PivotValue {
                source_col: definition.measure.clone(),
                name: definition.measure.clone(),
                number_format: Some(if definition.measure == "Revenue" { "\"$\"#,##0.00" } else { "#,##0" }.into()),
                ..Default::default()
            }],
            layout: PivotLayout::Tabular,
            subtotals: false,
            ..Default::default()
        });
        if report_idx == cx.wb.sheets.len() {
            cx.wb.sheets.push(Arc::new(report));
        } else if let Some(slot) = cx.wb.sheets.get_mut(report_idx) {
            *slot = Arc::new(report);
        }
        let area = crate::pivot::refresh(&mut cx.wb, report_idx, 0, false).map_err(failure)?;
        let expected_data = cx.wb.sheets.get(data_idx).cloned().ok_or_else(|| failure("Analysis data sheet is missing."))?;
        let expected_report = cx.wb.sheets.get(report_idx).cloned().ok_or_else(|| failure("Analysis report sheet is missing."))?;
        cx.wb.analysis = Some(AnalysisBinding {
            refresh_safe: true,
            definition,
            data_sheet: data_name.clone(),
            report_sheet: report_name.clone(),
            refresh_count: generation,
            source_rows: rows.len(),
            expected_data,
            expected_report,
        });
        cx.wb.active_sheet = report_idx;
        *cx.sel = crate::Selection::at(CellRef::new(4, 0));
        cx.structural = true;
        Ok(json!({"synthetic": true, "live": false, "dataSheet": data_name, "reportSheet": report_name,
            "range": area.a1(), "sourceRows": rows.len(), "refreshCount": generation}))
    })
}

fn xmla_discover(_: &mut Session, p: &Json) -> Result<Json> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        rowset: String,
        catalog: Option<String>,
        cube: Option<String>,
    }
    let p: Params = strict(p, "analysis.xmla.discoverRequest")?;
    let soap = crate::analysis::xmla::discover(&p.rowset, p.catalog.as_deref(), p.cube.as_deref()).map_err(failure)?;
    Ok(json!({"networkSent":false,"soapAction":"Discover","xml":soap}))
}
fn xmla_execute(_: &mut Session, p: &Json) -> Result<Json> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        catalog: String,
        mdx: String,
    }
    let p: Params = strict(p, "analysis.xmla.executeRequest")?;
    let soap = crate::analysis::xmla::execute(&p.catalog, &p.mdx).map_err(failure)?;
    Ok(json!({"networkSent":false,"soapAction":"Execute","xml":soap}))
}
fn xmla_parse(_: &mut Session, p: &Json) -> Result<Json> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Params {
        xml: String,
    }
    let p: Params = strict(p, "analysis.xmla.parse")?;
    Ok(json!(crate::analysis::xmla::parse_rowset(p.xml.as_bytes()).map_err(failure)?))
}
