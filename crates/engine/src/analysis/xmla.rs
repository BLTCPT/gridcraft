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
    if ![
        "DISCOVER_DATASOURCES",
        "DBSCHEMA_CATALOGS",
        "MDSCHEMA_CUBES",
        "MDSCHEMA_DIMENSIONS",
        "MDSCHEMA_HIERARCHIES",
        "MDSCHEMA_MEASURES",
        "MDSCHEMA_SAP_VARIABLES",
    ]
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

const SOAP: &str = "http://schemas.xmlsoap.org/soap/envelope/";
const ROWSET: &str = "urn:schemas-microsoft-com:xml-analysis:rowset";
const XSD: &str = "http://www.w3.org/2001/XMLSchema";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Envelope,
    Header,
    Body,
    Response,
    Return,
    Root,
    Metadata,
    Row,
    Field,
}

fn namespace_name(namespace: ResolveResult<'_>) -> Result<String, String> {
    match namespace {
        ResolveResult::Bound(ns) => std::str::from_utf8(ns.as_ref()).map(str::to_owned).map_err(|_| "Invalid XML namespace.".into()),
        ResolveResult::Unbound => Ok(String::new()),
        ResolveResult::Unknown(_) => Err("Undeclared XML namespace prefix.".into()),
    }
}

fn valid_xml_text(text: &str) -> bool {
    text.chars().all(|c| matches!(c, '\t' | '\r' | '\n') || (c >= '\u{20}' && c != '\u{fffe}' && c != '\u{ffff}'))
}

/// Validate every attribute, including ignored metadata. Iterator errors must never be skipped.
/// Attribute identity uses its expanded name; a different prefix cannot duplicate xsi:nil.
fn attributes(reader: &NsReader<&[u8]>, element: &quick_xml::events::BytesStart<'_>) -> Result<Option<bool>, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut nil = None;
    for attribute in element.attributes().with_checks(true) {
        let attribute = attribute.map_err(|_| "Malformed or duplicate XML attribute.")?;
        let value = attribute.decode_and_unescape_value(reader.decoder()).map_err(|_| "Invalid XML attribute value.")?;
        if !valid_xml_text(&value) || attribute.value.contains(&b'<') {
            return Err("Invalid XML attribute value.".into());
        }
        if attribute.key.as_ref() == b"xmlns" || attribute.key.as_ref().starts_with(b"xmlns:") {
            continue;
        }
        let (namespace, local) = reader.resolve_attribute(attribute.key);
        let namespace = namespace_name(namespace)?;
        let local = std::str::from_utf8(local.as_ref()).map_err(|_| "Invalid XML attribute name.")?.to_string();
        if !seen.insert((namespace.clone(), local.clone())) {
            return Err("Duplicate expanded XML attribute name.".into());
        }
        if local == "nil" {
            if namespace != XSI {
                return Err("XMLA null markers must use the XML Schema instance namespace.".into());
            }
            nil = Some(match value.as_ref() {
                "true" | "1" => true,
                "false" | "0" => false,
                _ => return Err("Invalid xsi:nil boolean.".into()),
            });
        }
    }
    Ok(nil)
}

fn append_text(scope: Option<&Scope>, field: &mut Option<(String, Option<String>)>, text: &str) -> Result<(), String> {
    if !valid_xml_text(text) {
        return Err("Invalid XML character.".into());
    }
    match scope {
        Some(Scope::Field) => match field {
            Some((_, Some(value))) => value.push_str(text),
            Some((_, None)) if text.is_empty() => {}
            _ => return Err("A null XMLA field cannot contain text.".into()),
        },
        Some(Scope::Header | Scope::Metadata) => {}
        _ if text.trim().is_empty() => {}
        _ => return Err("Unexpected text outside an XMLA field.".into()),
    }
    Ok(())
}

pub fn parse_rowset(xml: &[u8]) -> Result<Rowset, String> {
    if xml.len() > MAX_BYTES {
        return Err("XMLA response exceeds 8 MiB.".into());
    }
    if !std::str::from_utf8(xml).is_ok_and(valid_xml_text) {
        return Err("XMLA responses must contain valid UTF-8 XML characters.".into());
    }
    let mut reader = NsReader::from_reader(xml);
    reader.config_mut().expand_empty_elements = true;
    reader.config_mut().check_comments = true;
    let mut stack = Vec::new();
    let mut rows = Vec::new();
    let mut row: Option<BTreeMap<String, Option<String>>> = None;
    let mut field = None;
    let (mut envelope, mut header, mut body, mut response, mut returned, mut root, mut schema, mut declared) =
        (false, false, false, false, false, false, false, false);
    loop {
        let (namespace, event) = reader.read_resolved_event().map_err(|_| "Malformed XMLA response.".to_string())?;
        let namespace = namespace_name(namespace)?;
        match event {
            Event::DocType(_) | Event::PI(_) => return Err("DTD and processing instructions are not accepted.".into()),
            Event::Decl(e) => {
                if declared || envelope || !stack.is_empty() || e.version().map_err(|_| "Invalid XML declaration.")?.as_ref() != b"1.0" {
                    return Err("Only one XML 1.0 declaration before the envelope is supported.".into());
                }
                let declaration = std::str::from_utf8(&e).map_err(|_| "Invalid XML declaration.")?;
                let element = quick_xml::events::BytesStart::from_content(declaration, 3);
                attributes(&reader, &element)?;
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|_| "Invalid XML declaration.")?;
                    let valid = match attribute.key.as_ref() {
                        b"version" => attribute.value.as_ref() == b"1.0",
                        b"encoding" => attribute.value.eq_ignore_ascii_case(b"UTF-8"),
                        b"standalone" => matches!(attribute.value.as_ref(), b"yes" | b"no"),
                        _ => false,
                    };
                    if !valid {
                        return Err("Unsupported XML declaration.".into());
                    }
                }
                declared = true;
            }
            Event::Start(e) => {
                if stack.len() >= 64 {
                    return Err("XMLA nesting limit exceeded.".into());
                }
                let name = std::str::from_utf8(e.local_name().as_ref()).map_err(|_| "Invalid XMLA element name.")?.to_string();
                if matches!(name.as_str(), "Fault" | "Error" | "Messages") {
                    return Err("The server returned an XMLA error. No results were imported.".into());
                }
                let nil = attributes(&reader, &e)?;
                let scope = match stack.last() {
                    None if !envelope && namespace == SOAP && name == "Envelope" => {
                        envelope = true;
                        Scope::Envelope
                    }
                    Some(Scope::Envelope) if !header && !body && namespace == SOAP && name == "Header" => {
                        header = true;
                        Scope::Header
                    }
                    Some(Scope::Envelope) if !body && namespace == SOAP && name == "Body" => {
                        body = true;
                        Scope::Body
                    }
                    Some(Scope::Body) if !response && namespace == XMLA && matches!(name.as_str(), "DiscoverResponse" | "ExecuteResponse") => {
                        response = true;
                        Scope::Response
                    }
                    Some(Scope::Response) if !returned && namespace == XMLA && name == "return" => {
                        returned = true;
                        Scope::Return
                    }
                    Some(Scope::Return) if !root && namespace == ROWSET && name == "root" => {
                        root = true;
                        Scope::Root
                    }
                    Some(Scope::Root) if !schema && rows.is_empty() && namespace == XSD && name == "schema" => {
                        schema = true;
                        Scope::Metadata
                    }
                    Some(Scope::Header | Scope::Metadata) => Scope::Metadata,
                    Some(Scope::Root) if namespace == ROWSET && name == "row" => {
                        if rows.len() >= MAX_ROWS {
                            return Err("XMLA row limit exceeded.".into());
                        }
                        row = Some(BTreeMap::new());
                        Scope::Row
                    }
                    Some(Scope::Row) if namespace == ROWSET => {
                        if row.as_ref().is_some_and(|r| r.len() >= 256 || r.contains_key(&name)) {
                            return Err("Duplicate XMLA field or column limit exceeded.".into());
                        }
                        field = Some((name, if nil == Some(true) { None } else { Some(String::new()) }));
                        Scope::Field
                    }
                    _ => return Err("Expected a single SOAP/XMLA tabular response with correctly namespaced elements.".into()),
                };
                if nil.is_some() && scope != Scope::Field {
                    return Err("XMLA null markers are supported only on row fields.".into());
                }
                stack.push(scope);
            }
            Event::Text(e) => {
                let text = e.xml_content().map_err(|_| "Invalid XMLA text encoding.")?;
                let text = quick_xml::escape::unescape(&text).map_err(|_| "Invalid XMLA entity.")?;
                append_text(stack.last(), &mut field, &text)?;
            }
            Event::CData(e) => {
                if !matches!(stack.last(), Some(Scope::Field | Scope::Metadata)) {
                    return Err("Unexpected XMLA CDATA.".into());
                }
                append_text(stack.last(), &mut field, &e.decode().map_err(|_| "Invalid XMLA text encoding.")?)?;
            }
            Event::GeneralRef(e) => {
                let entity = format!("&{};", e.decode().map_err(|_| "Invalid XMLA entity.")?);
                let text = quick_xml::escape::unescape(&entity).map_err(|_| "External or invalid XMLA entities are unsupported.")?;
                append_text(stack.last(), &mut field, &text)?;
            }
            Event::End(_) => match stack.pop() {
                Some(Scope::Field) => {
                    let (name, value) = field.take().ok_or("Missing XMLA field state.")?;
                    row.as_mut().ok_or("Missing XMLA row state.")?.insert(name, value);
                }
                Some(Scope::Row) => rows.push(row.take().ok_or("Missing XMLA row state.")?),
                Some(_) => {}
                None => return Err("Unexpected XMLA end element.".into()),
            },
            Event::Eof => break,
            Event::Comment(_) => {}
            Event::Empty(_) => return Err("Unexpanded XMLA element.".into()),
        }
    }
    if !envelope || !body || !response || !returned || !root || !stack.is_empty() {
        return Err("Expected a complete XMLA tabular response. Multidimensional cellsets are not supported.".into());
    }
    Ok(Rowset { rows, source: "xmla-rowset" })
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

    #[test]
    fn rowset_requires_one_qualified_soap_document() {
        let valid = wrap("<row><A>1</A></row>");
        for xml in [
            valid.replace(SOAP, "urn:fake-soap"),
            valid.replace("<row>", "<row xmlns=\"urn:fake-rows\">"),
            valid.replace("<A>", "<A xmlns=\"urn:fake-fields\">"),
            format!("{valid}{valid}"),
            format!("{}<root xmlns=\"{ROWSET}\"><row><A>1</A></row></root>", wrap("")),
            wrap("<root><row><A>1</A></row></root>"),
            valid.replace("<return>", "<wrong><return>").replace("</return>", "</return></wrong>"),
            valid.replace("</return>", "</return><return/>"),
            valid.replace("</s:Body>", "</s:Body><s:Body/>"),
            format!("{valid}trailing text"),
            valid.replace("<A>", "<x:A>").replace("</A>", "</x:A>"),
        ] {
            assert!(parse_rowset(xml.as_bytes()).is_err(), "{xml}");
        }
    }

    #[test]
    fn all_attributes_and_null_markers_are_validated() {
        for field in [
            "<A nil=bad>1</A>",
            "<A x=\"1\" x=\"2\">1</A>",
            "<A nil=\"true\">1</A>",
            "<A xmlns:x=\"urn:fake\" x:nil=\"true\">1</A>",
            "<A xmlns:x=\"http://www.w3.org/2001/XMLSchema-instance\" x:nil=\"yes\"/>",
            "<A xmlns:x=\"http://www.w3.org/2001/XMLSchema-instance\" x:nil=\"true\">1</A>",
            "<A xmlns:x=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:y=\"http://www.w3.org/2001/XMLSchema-instance\" x:nil=\"true\" y:nil=\"false\"/>",
            "<A ignored=\"&external;\">1</A>",
            "<A ignored=\"<\">1</A>",
            "<A ignored=\"&#0;\">1</A>",
            "<A>&#0;</A>",
        ] {
            assert!(parse_rowset(wrap(&format!("<row>{field}</row>")).as_bytes()).is_err(), "{field}");
        }
        let xml = wrap(&format!("<row xmlns:i=\"{XSI}\"><A i:nil=\"true\"/><B i:nil=\"1\"/><C i:nil=\"false\">1</C><D i:nil=\"0\">0</D><E/></row>"));
        let rows = parse_rowset(xml.as_bytes()).unwrap().rows;
        assert_eq!(rows[0]["A"], None);
        assert_eq!(rows[0]["B"], None);
        assert_eq!(rows[0]["C"], Some("1".into()));
        assert_eq!(rows[0]["D"], Some("0".into()));
        assert_eq!(rows[0]["E"], Some(String::new()));
    }

    #[test]
    fn schema_metadata_header_empty_rows_and_safe_entities_remain_supported() {
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>{}",
            wrap(&format!(
                "<xs:schema xmlns:xs=\"{XSD}\" targetNamespace=\"{ROWSET}\"><xs:element name=\"root\"><xs:annotation><xs:documentation>Names &amp; values</xs:documentation></xs:annotation></xs:element></xs:schema><!-- valid comment --><row/><row><A><![CDATA[x&y]]> &amp; &#49;</A></row>"
            ))
            .replace("<s:Body>", "<s:Header><Session xmlns=\"urn:schemas-microsoft-com:xml-analysis\" SessionId=\"opaque\"/></s:Header><s:Body>")
        );
        let rows = parse_rowset(xml.as_bytes()).unwrap().rows;
        assert!(rows[0].is_empty());
        assert_eq!(rows[1]["A"], Some("x&y & 1".into()));
        assert!(parse_rowset(wrap("").as_bytes()).unwrap().rows.is_empty());
        let empty = wrap("").replace(&format!("<root xmlns=\"{ROWSET}\"></root>"), &format!("<root xmlns=\"{ROWSET}\"/>"));
        assert!(parse_rowset(empty.as_bytes()).unwrap().rows.is_empty());
    }

    #[test]
    fn metadata_cannot_hide_malformed_xml_or_unsupported_declarations() {
        for xml in [
            format!("{}<?xml version=\"1.0\"?>", wrap("")),
            format!("<?xml version=\"1.0\" version=\"1.0\"?>{}", wrap("")),
            format!("<?xml version=\"1.0\" encoding=\"UTF-16\"?>{}", wrap("")),
            wrap("<!-- bad -- comment -->"),
            wrap(&format!("<x:schema xmlns:x=\"{XSD}\" invalid=unquoted/>")),
            wrap(&format!("<x:schema xmlns:x=\"{XSD}\"><x:annotation>&external;</x:annotation></x:schema>")),
        ] {
            assert!(parse_rowset(xml.as_bytes()).is_err(), "{xml}");
        }
    }
}
