//! In-memory analytical report binding. XLSX currently exports ordinary cells and pivots only.
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::Sheet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisQuery {
    pub query: String,
    pub fiscal_year: u16,
    #[serde(default)]
    pub region: Option<String>,
    pub rows: String,
    pub columns: String,
    pub measure: String,
    /// Present only for the optional read-only XMLA transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bw: Option<BwQuery>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BwQuery {
    pub endpoint: String,
    pub catalog: String,
    pub mdx: String,
    /// Explicit single-member MDX placeholders; not BW/BICS variable discovery.
    #[serde(default)]
    pub variables: std::collections::BTreeMap<String, String>,
    /// Unit/currency field must contain at most one non-empty value per result.
    #[serde(default)]
    pub unit_field: Option<String>,
    #[serde(default)]
    pub simulator: bool,
}

/// Kept in the COW workbook so refresh, layout changes and undo are atomic. No credentials.
#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisBinding {
    /// False when a saved report was changed outside the managed refresh workflow.
    pub refresh_safe: bool,
    pub definition: AnalysisQuery,
    pub data_sheet: String,
    pub report_sheet: String,
    pub refresh_count: u32,
    pub source_rows: usize,
    /// Snapshots guard against overwriting a user's subsequent edits on refresh.
    pub expected_data: Arc<Sheet>,
    pub expected_report: Arc<Sheet>,
}

/// Versioned, non-secret definition stored in GridCraft's own XLSX extension part.
/// This is not SAP Analysis for Office metadata and contains no session credentials.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedAnalysis {
    pub schema_version: u8,
    pub provider: String,
    pub definition: AnalysisQuery,
    pub data_sheet: String,
    pub report_sheet: String,
    pub refresh_count: u32,
    pub source_rows: usize,
    pub data_fingerprint: u64,
    pub report_fingerprint: u64,
    pub refresh_safe: bool,
}

/// Non-cryptographic change detection for imported cell content, not an authenticity check.
/// Input text preserves the distinction between a literal value and a formula.
pub fn content_fingerprint(sheet: &Sheet) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for (at, cell) in sheet.cells.iter() {
        if cell.value.is_empty() && cell.formula.is_none() {
            continue;
        }
        for byte in at.row.to_le_bytes().into_iter().chain(at.col.to_le_bytes()).chain(cell.input_text().bytes()).chain([0xff]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}
