//! Optional GridCraft-owned analysis definition. No network calls, engine or credentials.
use crate::{
    IoError,
    read::Ctx,
    write::{Out, Rels},
};
use gridcraft_model::{
    Workbook,
    analysis::{AnalysisBinding, SavedAnalysis, content_fingerprint},
};

const PART: &str = "xl/gridcraft/analysis.json";
const MAX_DEFINITION: u64 = 16 * 1024;

pub fn write(wb: &Workbook, out: &mut Out, rels: &mut Rels) -> Result<(), IoError> {
    let Some(b) = &wb.analysis else {
        return Ok(());
    };
    let definition = SavedAnalysis {
        schema_version: 1,
        provider: "fixture".into(),
        definition: b.definition.clone(),
        data_sheet: b.data_sheet.clone(),
        report_sheet: b.report_sheet.clone(),
        refresh_count: b.refresh_count,
        source_rows: b.source_rows,
        data_fingerprint: content_fingerprint(&b.expected_data),
        report_fingerprint: content_fingerprint(&b.expected_report),
        refresh_safe: b.refresh_safe,
    };
    let data = serde_json::to_vec(&definition).map_err(|_| IoError::Format("Cannot serialize GridCraft analysis definition.".into()))?;
    if data.len() as u64 > MAX_DEFINITION {
        return Err(IoError::TooLarge("GridCraft analysis definition.".into()));
    }
    out.part(PART, Some("application/json"), data);
    rels.add_uri("https://github.com/BLTCPT/gridcraft/relationships/analysis", "gridcraft/analysis.json");
    Ok(())
}

pub fn read(cx: &mut Ctx<'_>, wb: &mut Workbook) -> Result<(), IoError> {
    let Some(data) = cx.pkg.read_limited(PART, MAX_DEFINITION)? else {
        return Ok(());
    };
    let saved = match serde_json::from_slice::<SavedAnalysis>(&data) {
        Ok(saved) if saved.schema_version == 1 && saved.provider == "fixture" && saved.source_rows <= 10_000 => saved,
        _ => {
            cx.warn("Unsupported or invalid GridCraft analysis definition; cells were preserved without a refresh binding.");
            return Ok(());
        }
    };
    let data = wb.sheet_index(&saved.data_sheet).and_then(|i| wb.sheets.get(i)).cloned();
    let report = wb.sheet_index(&saved.report_sheet).and_then(|i| wb.sheets.get(i)).cloned();
    let (Some(data), Some(report)) = (data, report) else {
        cx.warn("GridCraft analysis sheets are missing; cached cells were preserved.");
        return Ok(());
    };
    if saved.data_sheet == saved.report_sheet || report.pivots.len() != 1 {
        cx.warn("GridCraft analysis report structure is invalid; cached cells were preserved.");
        return Ok(());
    }
    let refresh_safe =
        saved.refresh_safe && content_fingerprint(&data) == saved.data_fingerprint && content_fingerprint(&report) == saved.report_fingerprint;
    if !refresh_safe {
        cx.warn("GridCraft analysis cells were modified; refresh is disabled to preserve those changes.");
    }
    wb.analysis = Some(AnalysisBinding {
        refresh_safe,
        definition: saved.definition,
        data_sheet: saved.data_sheet,
        report_sheet: saved.report_sheet,
        refresh_count: saved.refresh_count,
        source_rows: saved.source_rows,
        expected_data: data,
        expected_report: report,
    });
    Ok(())
}
