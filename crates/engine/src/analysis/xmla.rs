//! Bounded XMLA SOAP codec for optional SAP BW XMLA endpoints. No AO workbook parsing.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::todo, clippy::unimplemented)]
use std::collections::BTreeMap;

use quick_xml::{NsReader, events::Event, name::ResolveResult};
use serde::Serialize;

pub const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_ROWS: usize = 10_000;
const XMLA: &str = "urn:schemas-microsoft-com:xml-analysis";

fn escape(value: &str) -> Result<String, String> {
    if value.len() > 64 * 1024 || value.chars().any(|c| c.is_control() && !matches!(c, '\t' | '\r' | '\n')) {
        return Err("XMLA input exceeds the limit or contains invalid control characters.".into());
    }
    Ok(quick_xml::escape::escape(value).into_owned())
}
fn envelope(body: String) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><soap:Envelope xmlns:soap=\"http://schemas.xmlsoap.org/soap/envelope/\"><soap:Body>{body}</soap:Body></soap:Envelope>"
    )
}
/// Only standard read-only discovery rowsets, with explicit technical catalog/cube restrictions.
pub fn discover(rowset: &str, catalog: Option<&str>, cube: Option<&str>) -> Result<String, String> {
    if !["DISCOVER_DATASOURCES", "DBSCHEMA_CATALOGS", "MDSCHEMA_CUBES", "MDSCHEMA_DIMENSIONS", "MDSCHEMA_HIERARCHIES", "MDSCHEMA_MEASURES"]
        .contains(&rowset)
    {
        return Err("Unsupported discovery rowset.".into());
    }
    let catalog = catalog.map(escape).transpose()?;
    let cube = cube.map(escape).transpose()?;
    let restrictions = format!(
        "{}{}",
        catalog.as_ref().map(|v| format!("<CATALOG_NAME>{v}</CATALOG_NAME>")).unwrap_or_default(),
        cube.map(|v| format!("<CUBE_NAME>{v}</CUBE_NAME>")).unwrap_or_default()
    );
    let properties = catalog.map(|v| format!("<Catalog>{v}</Catalog>")).unwrap_or_default();
    Ok(envelope(format!(
        "<Discover xmlns=\"{XMLA}\"><RequestType>{rowset}</RequestType><Restrictions><RestrictionList>{restrictions}</RestrictionList></Restrictions><Properties><PropertyList>{properties}<Format>Tabular</Format><Content>SchemaData</Content></PropertyList></Properties></Discover>"
    )))
}
/// A deliberately narrow read-only MDX subset. WITH, statements, session mutations and writes
/// are rejected; callers supply a SELECT obtained from an administrator, not generated SAP SQL.
pub fn execute(catalog: &str, mdx: &str) -> Result<String, String> {
    let mdx = mdx.trim();
    if mdx.get(..7).is_none_or(|s| !s.eq_ignore_ascii_case("SELECT ")) || mdx.contains(';') || mdx.contains("--") || mdx.contains("/*") {
        return Err("Only a single MDX SELECT followed by a space is supported (no comments, WITH or semicolons).".into());
    }
    let catalog = escape(catalog)?;
    let mdx = escape(mdx)?;
    Ok(envelope(format!(
        "<Execute xmlns=\"{XMLA}\"><Command><Statement>{mdx}</Statement></Command><Properties><PropertyList><Catalog>{catalog}</Catalog><Format>Tabular</Format><Content>SchemaData</Content></PropertyList></Properties></Execute>"
    )))
}

/// Field names are technical XMLA identifiers. Captions, member unique names, units and typed
/// strings remain separate fields as returned by the server; no caption-based identity joins.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rowset {
    pub rows: Vec<BTreeMap<String, Option<String>>>,
    pub source: &'static str,
}

pub fn parse_rowset(xml: &[u8]) -> Result<Rowset, String> {
    if xml.len() > MAX_BYTES {
        return Err("XMLA response exceeds 8 MiB.".into());
    }
    let mut reader = NsReader::from_reader(xml);
    let mut stack: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    let mut row: Option<BTreeMap<String, Option<String>>> = None;
    let mut field: Option<(String, Option<String>)> = None;
    let mut row_depth = 0;
    let mut response = false;
    let mut root = false;
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(|_| "Malformed XMLA response.".to_string())?;
        let rowset_namespace = matches!(&namespace, ResolveResult::Bound(ns) if ns.as_ref() == b"urn:schemas-microsoft-com:xml-analysis:rowset");
        let xmla_namespace = matches!(&namespace, ResolveResult::Bound(ns) if ns.as_ref() == XMLA.as_bytes());
        match event {
            Event::DocType(_) | Event::PI(_) => return Err("DTD and processing instructions are not accepted.".into()),
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if matches!(name.as_str(), "Fault" | "Error" | "Messages") {
                    return Err("The server returned an XMLA error. No results were imported.".into());
                }
                if stack.len() >= 64 {
                    return Err("XMLA nesting limit exceeded.".into());
                }
                if (name == "DiscoverResponse" || name == "ExecuteResponse") && xmla_namespace && stack.last().is_some_and(|s| s == "Body") {
                    response = true;
                }
                if name == "root" && response {
                    if !rowset_namespace {
                        return Err("Only XMLA tabular rowsets are supported; multidimensional cellsets need a separate decoder.".into());
                    }
                    root = true;
                }
                if name == "row" && root && stack.last().is_some_and(|s| s == "root") {
                    if rows.len() >= MAX_ROWS {
                        return Err("XMLA row limit exceeded.".into());
                    }
                    row = Some(BTreeMap::new());
                    row_depth = stack.len() + 1;
                } else if row.is_some() && stack.len() == row_depth {
                    if row.as_ref().is_some_and(|r| r.len() >= 256) {
                        return Err("XMLA column limit exceeded.".into());
                    }
                    let nil = e
                        .attributes()
                        .with_checks(true)
                        .any(|a| a.ok().is_some_and(|a| a.key.local_name().as_ref() == b"nil" && a.value.as_ref() == b"true"));
                    field = Some((name.clone(), if nil { None } else { Some(String::new()) }));
                } else if field.is_some() {
                    return Err("Nested XMLA field values are unsupported.".into());
                }
                stack.push(name);
            }
            Event::Empty(e) => {
                if field.is_some() {
                    return Err("Nested XMLA field values are unsupported.".into());
                }
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if matches!(name.as_str(), "Error" | "Fault") {
                    return Err("The server returned an XMLA error. No results were imported.".into());
                }
                if row.is_some() && stack.len() == row_depth {
                    let nil = e
                        .attributes()
                        .with_checks(true)
                        .any(|a| a.ok().is_some_and(|a| a.key.local_name().as_ref() == b"nil" && a.value.as_ref() == b"true"));
                    if let Some(r) = &mut row
                        && (r.len() >= 256 || r.insert(name, if nil { None } else { Some(String::new()) }).is_some())
                    {
                        return Err("Duplicate XMLA field or column limit exceeded.".into());
                    }
                }
                if name_is_root(&e) && response && rowset_namespace {
                    root = true;
                }
            }
            Event::Text(e) => {
                if let Some((_, Some(value))) = &mut field {
                    let text = e.xml_content().map_err(|_| "Invalid XMLA text encoding.".to_string())?;
                    value.push_str(&quick_xml::escape::unescape(&text).map_err(|_| "Invalid XMLA entity.".to_string())?);
                }
            }
            Event::CData(e) => {
                if let Some((_, Some(value))) = &mut field {
                    value.push_str(&e.decode().map_err(|_| "Invalid XMLA text encoding.".to_string())?);
                }
            }
            Event::GeneralRef(e) => {
                if let Some((_, Some(value))) = &mut field {
                    let name = e.decode().map_err(|_| "Invalid XMLA entity.".to_string())?;
                    value.push_str(
                        &quick_xml::escape::unescape(&format!("&{name};")).map_err(|_| "External XMLA entities are unsupported.".to_string())?,
                    );
                } else {
                    return Err("Unexpected XMLA entity.".into());
                }
            }
            Event::End(_) => {
                if stack.len() == row_depth + 1
                    && let Some((name, value)) = field.take()
                    && let Some(r) = &mut row
                    && r.insert(name, value).is_some()
                {
                    return Err("Duplicate XMLA field.".into());
                }
                if stack.len() == row_depth
                    && let Some(r) = row.take()
                {
                    rows.push(r);
                }
                stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !response || !root || !stack.is_empty() {
        return Err("Expected a complete XMLA tabular response. Multidimensional cellsets are not supported.".into());
    }
    Ok(Rowset { rows, source: "xmla-rowset" })
}
fn name_is_root(e: &quick_xml::events::BytesStart<'_>) -> bool {
    e.local_name().as_ref() == b"root"
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    fn wrap(rows: &str) -> String {
        format!(
            "<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\"><s:Body><DiscoverResponse xmlns=\"urn:schemas-microsoft-com:xml-analysis\"><return><root xmlns=\"urn:schemas-microsoft-com:xml-analysis:rowset\">{rows}</root></return></DiscoverResponse></s:Body></s:Envelope>"
        )
    }
    #[test]
    fn discovery_escapes_technical_ids() {
        let xml = discover("MDSCHEMA_CUBES", Some("A&B"), Some("X</CUBE_NAME>")).unwrap();
        assert!(xml.contains("A&amp;B"));
        assert!(xml.contains("X&lt;/CUBE_NAME&gt;"));
        assert!(discover("EXECUTE_SQL", None, None).is_err());
    }
    #[test]
    fn read_only_subset_rejects_session_and_write_statements() {
        for s in ["UPDATE CUBE X", "CREATE SESSION CUBE X", "SELECT X; DELETE X", "WITH MEMBER X SELECT X", "SELECT X --comment"] {
            assert!(execute("BW", s).is_err());
        }
        assert!(execute("BW", "SELECT {[Measures].[Value]} ON COLUMNS FROM [Cube]").unwrap().contains("<Format>Tabular</Format>"));
    }
    #[test]
    fn rowset_preserves_identifiers_captions_entities_and_nulls() {
        let xml = wrap(
            "<row><MEMBER_UNIQUE_NAME>[Region].[1]</MEMBER_UNIQUE_NAME><MEMBER_CAPTION>Sales &amp; Service</MEMBER_CAPTION><VALUE>123.50</VALUE><EMPTY/><MISSING xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:nil=\"true\"/></row>",
        );
        let r = parse_rowset(xml.as_bytes()).unwrap();
        assert_eq!(r.rows[0]["MEMBER_UNIQUE_NAME"], Some("[Region].[1]".into()));
        assert_eq!(r.rows[0]["MEMBER_CAPTION"], Some("Sales & Service".into()));
        assert_eq!(r.rows[0]["VALUE"], Some("123.50".into()));
        assert_eq!(r.rows[0]["MISSING"], None);
    }
    #[test]
    fn faults_malformed_and_hostile_xml_are_errors() {
        for xml in [
            "<html>login</html>".into(),
            wrap("<row><A>1</A><A>2</A></row>"),
            wrap("<row><A><B/></A></row>"),
            wrap("<Messages><Error/></Messages>"),
            "<!DOCTYPE x [<!ENTITY e SYSTEM 'file:///etc/passwd'>]><x>&e;</x>".into(),
            wrap("<row><A>1</row>"),
        ] {
            assert!(parse_rowset(xml.as_bytes()).is_err(), "{xml}");
        }
        assert!(parse_rowset(&vec![b' '; MAX_BYTES + 1]).is_err());
    }
}
