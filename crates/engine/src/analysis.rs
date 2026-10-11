//! Analytical-provider boundary. The demo provider is synthetic and performs no network I/O.
//! BI-platform repository access and BW/HANA analytical execution are separate capabilities.
pub mod network;
pub mod rfc;
pub mod xmla;

use gridcraft_core::Value;
use gridcraft_model::analysis::AnalysisQuery;
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

pub const QUERY_ID: &str = "demo.sales";
pub const DIMENSIONS: &[&str] = &["Region", "Product", "Quarter"];
pub const MEASURES: &[&str] = &["Revenue", "Units"];
pub const REGIONS: &[&str] = &["Americas", "Europe", "Asia Pacific"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    #[default]
    Fixture,
    BwXmla,
    BwSimulator,
    BwRfc,
    HanaIna,
}

impl ProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fixture => "Synthetic demo",
            Self::BwXmla => "SAP BW XMLA (experimental)",
            Self::BwSimulator => "Local XMLA simulator (synthetic)",
            Self::BwRfc => "SAP BW RFC/OLAP BAPIs (unavailable)",
            Self::HanaIna => "SAP HANA InA (not implemented)",
        }
    }
}

/// Only non-secret settings are accepted. Tokens/passwords are never command parameters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionConfig {
    pub provider: ProviderKind,
    #[serde(default)]
    pub analytical_url: String,
    #[serde(default)]
    pub repository_url: String,
}

impl ConnectionConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.provider == ProviderKind::BwRfc {
            if !self.analytical_url.is_empty() || !self.repository_url.is_empty() {
                return Err("RFC is not an HTTP/BOBJ URL. Use analysis.rfc.validateConfig to review a non-secret destination draft.".into());
            }
            return Ok(());
        }
        for value in [&self.analytical_url, &self.repository_url] {
            if value.is_empty() {
                continue;
            }
            if self.provider == ProviderKind::BwSimulator && value == &self.analytical_url {
                if !network::simulator_endpoint(value) {
                    return Err("Simulator endpoint must be http://127.0.0.1:PORT/xmla.".into());
                }
                continue;
            }
            let authority = value.strip_prefix("https://").and_then(|x| x.split('/').next()).unwrap_or_default();
            if value.len() > 2048
                || authority.is_empty()
                || value.chars().any(|c| c.is_whitespace() || c.is_control())
                || value.contains(['@', '?', '#', '\\'])
                || !value.starts_with("https://")
            {
                return Err("Use a plain HTTPS endpoint without embedded credentials, query parameters or fragments.".into());
            }
        }
        if self.provider == ProviderKind::Fixture && (!self.analytical_url.is_empty() || !self.repository_url.is_empty()) {
            return Err("The synthetic provider does not accept server endpoints.".into());
        }
        if self.provider != ProviderKind::Fixture && self.analytical_url.is_empty() {
            return Err("Set the analytical endpoint separately from the BI-platform repository URL.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct AnalysisSession {
    pub config: ConnectionConfig,
    pub connected: bool,
    pub catalog: Json,
    pub pending: Option<network::Pending>,
}

/// Transport-independent query operations; a future provider must return typed values, not formulas.
pub trait AnalyticalProvider {
    fn queries(&self) -> Json;
    fn metadata(&self, query: &str) -> Result<Json, String>;
    fn execute(&self, query: &AnalysisQuery) -> Result<Vec<Vec<Value>>, String>;
}

/// Repository objects are intentionally NOT analytical queries.
pub trait WorkbookRepository {
    fn workbooks(&self) -> Json;
}

pub struct FixtureRepository;
impl WorkbookRepository for FixtureRepository {
    fn workbooks(&self) -> Json {
        json!({"synthetic": true, "live": false, "items": [{
            "id": "demo-workbook", "name": "Demo Sales Workbook", "kind": "syntheticExample",
            "description": "Original synthetic example. Not an AO workbook or BOBJ object."
        }]})
    }
}

pub struct FixtureProvider;
impl AnalyticalProvider for FixtureProvider {
    fn queries(&self) -> Json {
        json!({"synthetic": true, "live": false, "queries": [{"id": QUERY_ID, "name": "Sales by region and product", "provider": "fixture"}]})
    }
    fn metadata(&self, query: &str) -> Result<Json, String> {
        if query != QUERY_ID {
            return Err("Unknown synthetic query. Discover queries before inserting a report.".into());
        }
        Ok(json!({"query": QUERY_ID, "synthetic": true,
            "dimensions": DIMENSIONS, "measures": MEASURES,
            "variables": [{"id": "fiscalYear", "required": true, "type": "integer", "values": [2025, 2026]},
                {"id": "region", "required": false, "type": "singleMember", "values": REGIONS}],
            "currency": "USD", "aggregation": "sum", "hierarchies": false, "planning": false
        }))
    }
    fn execute(&self, query: &AnalysisQuery) -> Result<Vec<Vec<Value>>, String> {
        if query.bw.is_some() {
            return Err("A BW report cannot be executed by the synthetic fixture provider.".into());
        }
        self.metadata(&query.query)?;
        if ![2025, 2026].contains(&query.fiscal_year) {
            return Err("Fiscal year must be 2025 or 2026 for this synthetic query.".into());
        }
        if query.region.as_ref().is_some_and(|r| !REGIONS.contains(&r.as_str())) {
            return Err("Choose a region from the query metadata, or clear it for all regions.".into());
        }
        if !DIMENSIONS.contains(&query.rows.as_str()) || !DIMENSIONS.contains(&query.columns.as_str()) || query.rows == query.columns {
            return Err("Choose two different dimensions for rows and columns.".into());
        }
        if !MEASURES.contains(&query.measure.as_str()) {
            return Err("Choose Revenue or Units for the synthetic query.".into());
        }
        // Fixed, bounded, original test data. No company data and no pseudo live refresh values.
        let mut out = Vec::with_capacity(24);
        for (ri, region) in REGIONS.iter().enumerate() {
            if query.region.as_deref().is_some_and(|r| r != *region) {
                continue;
            }
            for (pi, product) in ["Hardware", "Services"].iter().enumerate() {
                for quarter in 1..=4 {
                    let units = (ri + 1) * 40 + (pi + 1) * 12 + quarter * 8 + usize::from(query.fiscal_year - 2025) * 10;
                    out.push(vec![
                        Value::text(*region),
                        Value::text(*product),
                        Value::text(format!("Q{quarter}")),
                        Value::Number((units * (if pi == 0 { 1250 } else { 450 })) as f64),
                        Value::Number(units as f64),
                    ]);
                }
            }
        }
        Ok(out)
    }
}

impl AnalysisSession {
    pub fn fixture(&self) -> Result<FixtureProvider, String> {
        if !self.connected {
            return Err("Connect to the synthetic provider first. No live SAP session is active.".into());
        }
        if self.config.provider != ProviderKind::Fixture {
            return Err(Self::unsupported(self.config.provider));
        }
        Ok(FixtureProvider)
    }
    pub fn unsupported(kind: ProviderKind) -> String {
        if kind == ProviderKind::BwRfc {
            return "RFC adapter unavailable: no SAP SDK or SNC provider loaded, no credentials accepted, no network request sent. A licensed company-provided SDK and verified OLAP BAPI interface are required; internal BICS is not implemented.".into();
        }
        format!(
            "{}: live transport is unavailable in this build. No network request was sent. BI-platform REST is a workbook repository interface, not a BW/HANA query engine.",
            kind.label()
        )
    }
}
