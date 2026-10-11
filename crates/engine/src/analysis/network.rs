//! Host transport contract and transient request state. No sockets or credentials in the engine.
use super::xmla;
use gridcraft_core::Value;
use gridcraft_model::{Workbook, analysis::AnalysisQuery};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
};

#[derive(Clone, Debug)]
pub struct Request {
    pub endpoint: String,
    pub action: &'static str,
    pub body: String,
    pub simulator: bool,
    pub canceled: Arc<AtomicBool>,
}
pub trait Transport: Send + Sync {
    fn start(&self, request: Request) -> Result<Receiver<Result<Vec<u8>, String>>, String>;
}
#[derive(Clone, Debug)]
pub enum Operation {
    Connect,
    Discover,
    Execute { definition: Box<AnalysisQuery>, refreshing: bool },
}
#[derive(Clone, Debug)]
pub struct Pending {
    pub receiver: Arc<Mutex<Receiver<Result<Vec<u8>, String>>>>,
    pub canceled: Arc<AtomicBool>,
    pub before: Arc<Workbook>,
    pub operation: Operation,
}
impl Pending {
    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::Relaxed);
    }
}
/// Plain HTTP is limited to the explicitly selected simulator on the numeric loopback address.
pub fn simulator_endpoint(url: &str) -> bool {
    url.strip_prefix("http://127.0.0.1:").and_then(|s| s.strip_suffix("/xmla")).is_some_and(|port| port.parse::<u16>().is_ok_and(|n| n > 0))
}
/// Deliberately narrow single-member substitution, never SQL/string interpolation. It is not
/// SAP variable processing; BW authorization/default/exit variables remain server controlled.
pub fn statement(q: &AnalysisQuery) -> Result<String, String> {
    let b = q.bw.as_ref().ok_or("Missing XMLA definition.")?;
    if b.catalog.is_empty() || b.catalog.len() > 256 || b.mdx.len() > 32_768 || b.variables.len() > 16 {
        return Err("XMLA catalog, statement or variable limit exceeded.".into());
    }
    let mut mdx = b.mdx.clone();
    for (name, member) in &b.variables {
        if name.is_empty()
            || name.len() > 64
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            || member.len() > 256
            || !member.starts_with('[')
            || !member.ends_with(']')
            || !member.chars().all(|c| c.is_ascii_alphanumeric() || "[].&_ -".contains(c))
        {
            return Err("Provide bounded single-member unique names for each MDX placeholder.".into());
        }
        let placeholder = format!("{{{{{name}}}}}");
        if !mdx.contains(&placeholder) {
            return Err("A supplied member has no matching MDX placeholder.".into());
        }
        mdx = mdx.replace(&placeholder, member);
    }
    if mdx.contains(['{', '}']) && (mdx.contains("{{") || mdx.contains("}}")) {
        return Err("Required MDX member placeholders are missing.".into());
    }
    xmla::execute(&b.catalog, &mdx)
}

pub fn typed_rows(q: &AnalysisQuery, rowset: xmla::Rowset) -> Result<(Vec<Vec<Value>>, Option<String>), String> {
    let b = q.bw.as_ref().ok_or("Missing XMLA definition.")?;
    if [&q.rows, &q.columns, &q.measure].iter().any(|s| s.is_empty() || s.len() > 256)
        || q.rows == q.columns
        || q.rows == q.measure
        || q.columns == q.measure
    {
        return Err("Map three different technical rowset fields to rows, columns and numeric measure.".into());
    }
    if rowset.rows.is_empty() {
        return Err("XMLA returned no rows; the previous report was preserved.".into());
    }
    let mut result = Vec::with_capacity(rowset.rows.len());
    let mut units = BTreeSet::new();
    let mut tuples = BTreeSet::new();
    for row in rowset.rows {
        let text = |key: &str| -> Result<String, String> {
            row.get(key)
                .and_then(|v| v.clone())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| "A mapped dimension or unit field is missing or null.".into())
        };
        let a = text(&q.rows)?;
        let c = text(&q.columns)?;
        if !tuples.insert((a.clone(), c.clone())) {
            return Err("Duplicate dimension tuples are unsupported; request a leaf-level, non-overlapping rowset without totals.".into());
        }
        let number = row.get(&q.measure).ok_or("The numeric measure field is missing.")?;
        let value = match number {
            None => Value::Empty,
            Some(n) => {
                Value::Number(n.parse::<f64>().ok().filter(|n| n.is_finite()).ok_or("The measure contains a non-finite or non-numeric value.")?)
            }
        };
        if let Some(unit) = &b.unit_field {
            units.insert(text(unit)?);
        }
        result.push(vec![Value::text(a), Value::text(c), value]);
    }
    if units.len() > 1 {
        return Err("Mixed currencies or units cannot be summed. Restrict the query to one unit.".into());
    }
    Ok((result, units.into_iter().next()))
}
