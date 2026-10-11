//! Application-side RFC adapter contract only. No C ABI, SAP headers, runtime or BICS calls.
use serde::{Deserialize, Serialize};
use serde_json::{Value as Json, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Destination {
    Direct { ashost: String, sysnr: String },
    Balanced { mshost: String, sysid: String, group: String, msserv: Option<String> },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Authentication {
    /// Runtime credentials would be acquired from an approved host credential provider, never commands.
    Password,
    /// This design supports privacy or maximum protection only; it does not implement SSO.
    #[serde(rename_all = "camelCase")]
    Snc { partner_name: String, my_name: Option<String>, provider_library: String, qop: u8 },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub destination: Destination,
    pub client: String,
    pub language: String,
    pub authentication: Authentication,
}
fn bounded(s: &str) -> bool {
    !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control)
}
fn host(s: &str) -> bool {
    bounded(s) && s.len() <= 253 && s.chars().all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.client.len() != 3
            || !self.client.bytes().all(|b| b.is_ascii_digit())
            || self.language.len() != 2
            || !self.language.bytes().all(|b| b.is_ascii_alphabetic())
        {
            return Err("RFC client must have three digits and language two letters.".into());
        }
        let valid = match &self.destination {
            Destination::Direct { ashost, sysnr } => host(ashost) && sysnr.len() == 2 && sysnr.bytes().all(|b| b.is_ascii_digit()),
            Destination::Balanced { mshost, sysid, group, msserv } => {
                host(mshost)
                    && sysid.len() == 3
                    && sysid.bytes().all(|b| b.is_ascii_alphanumeric())
                    && bounded(group)
                    && msserv.as_ref().is_none_or(|s| bounded(s) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            }
        };
        if !valid {
            return Err("Invalid RFC destination draft; SAProuter and arbitrary URLs are not supported.".into());
        }
        if let Authentication::Snc { partner_name, my_name, provider_library, qop } = &self.authentication
            && (!bounded(partner_name) || my_name.as_ref().is_some_and(|s| !bounded(s)) || !bounded(provider_library) || !matches!(qop, 3 | 9))
        {
            return Err(
                "SNC draft requires partner, approved library path and privacy (3) or maximum (9) protection. No downgrade is supported.".into()
            );
        }
        Ok(())
    }
}
pub fn capabilities() -> Json {
    json!({"provider":"bwRfc","available":false,"networkSent":false,"sdkLoaded":false,
        "reason":"Requires company-provided, appropriately licensed SAP NetWeaver RFC SDK, approved Windows authentication/SNC configuration and verified target OLAP BAPI signatures.",
        "transport":"RFC","analyticalInterface":"Documented OLAP BAPIs/MDX; not internal BICS",
        "authenticationImplemented":[],"draftAuthentication":["password via future host credential provider","SNC with approved provider; no downgrade"],
        "repositoryAuthentication":"Separate BOBJ interface; no token reuse",
        "candidateDiscovery":"BAPI_MDPROVIDER_GET_CUBES",
        "candidateDatasetLifecycle":["BAPI_MDDATASET_CREATE_OBJECT","BAPI_MDDATASET_SELECT_DATA","BAPI_MDDATASET_GET_AXIS_INFO","BAPI_MDDATASET_GET_AXIS_DATA","BAPI_MDDATASET_GET_CELL_DATA","BAPI_MDDATASET_DELETE_OBJECT"],
        "warning":"Names are a research roadmap. ABI and target signatures must be verified; no RFC function is executed by this build."})
}
/// Internal abstraction, not SAP SDK function signatures. An approved host adapter would own
/// the RFC connection and map these operations to verified target BAPIs. No unsafe code here.
pub trait OlapAdapter {
    type Handle;
    fn create_dataset(&mut self) -> Result<Self::Handle, String>;
    fn select_read_only(&mut self, handle: &Self::Handle, mdx: &str) -> Result<(), String>;
    fn axis_metadata(&mut self, handle: &Self::Handle) -> Result<Json, String>;
    fn axis_members(&mut self, handle: &Self::Handle) -> Result<Json, String>;
    fn cells(&mut self, handle: &Self::Handle) -> Result<Json, String>;
    fn delete_dataset(&mut self, handle: Self::Handle) -> Result<(), String>;
}
/// Mock-testable cleanup contract. It is not registered as an execution command or a real provider.
pub fn read_dataset<A: OlapAdapter>(adapter: &mut A, mdx: &str, canceled: &std::sync::atomic::AtomicBool) -> Result<Json, String> {
    if canceled.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("RFC request canceled.".into());
    }
    let handle = adapter.create_dataset()?;
    let result = (|| {
        let check = || if canceled.load(std::sync::atomic::Ordering::Relaxed) { Err("RFC request canceled.".to_string()) } else { Ok(()) };
        check()?;
        adapter.select_read_only(&handle, mdx)?;
        check()?;
        let metadata = adapter.axis_metadata(&handle)?;
        check()?;
        let members = adapter.axis_members(&handle)?;
        check()?;
        let cells = adapter.cells(&handle)?;
        check()?;
        Ok(json!({"axisMetadata":metadata,"axisMembers":members,"cells":cells}))
    })();
    let cleanup = adapter.delete_dataset(handle);
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (_, Err(_)) => Err("RFC dataset cleanup failed; invalidate the host connection and do not reuse it.".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mock_dataset_always_cleans_up() {
        struct Mock {
            calls: Vec<&'static str>,
            fail: bool,
        }
        impl OlapAdapter for Mock {
            type Handle = u32;
            fn create_dataset(&mut self) -> Result<u32, String> {
                self.calls.push("create");
                Ok(1)
            }
            fn select_read_only(&mut self, _: &u32, _: &str) -> Result<(), String> {
                self.calls.push("select");
                if self.fail { Err("synthetic authorization denial".into()) } else { Ok(()) }
            }
            fn axis_metadata(&mut self, _: &u32) -> Result<Json, String> {
                self.calls.push("metadata");
                Ok(json!({}))
            }
            fn axis_members(&mut self, _: &u32) -> Result<Json, String> {
                self.calls.push("members");
                Ok(json!([]))
            }
            fn cells(&mut self, _: &u32) -> Result<Json, String> {
                self.calls.push("cells");
                Ok(json!([]))
            }
            fn delete_dataset(&mut self, _: u32) -> Result<(), String> {
                self.calls.push("delete");
                Ok(())
            }
        }
        for (fail, canceled, expected) in [
            (false, false, vec!["create", "select", "metadata", "members", "cells", "delete"]),
            (true, false, vec!["create", "select", "delete"]),
            (false, true, vec![]),
        ] {
            let mut m = Mock { calls: vec![], fail };
            let r = read_dataset(&mut m, "synthetic", &std::sync::atomic::AtomicBool::new(canceled));
            assert_eq!(m.calls, expected);
            assert_eq!(r.is_ok(), !fail && !canceled);
        }
    }
}
