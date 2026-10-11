# RFC integration plan and unavailable capability scaffold

Updated 2026-10-11. RFC/SNC is the leading intended production connection path, pending
confirmation by the organization's SAP/Basis team. The local XMLA simulator remains a
separate demonstration path. It is not an RFC emulator or proof of Analysis for Office parity.

## Implemented now

`analysis.rfc.capabilities` returns `available: false`, `sdkLoaded: false`,
`networkSent: false` and no implemented authentication methods. The native pane displays
that unavailable state. Selecting the RFC provider cannot connect or execute a remote call.
`analysis.rfc.validateConfig` validates a nonsecret draft only; it never loads a library,
stores credentials, writes the draft into a workbook, or probes a host. For example:

```json
{
  "destination": {"kind": "direct", "ashost": "bw.example.invalid", "sysnr": "00"},
  "client": "100",
  "language": "EN",
  "authentication": {
    "kind": "snc",
    "partnerName": "p:CN=synthetic-example",
    "providerLibrary": "C:\\approved\\provider.dll",
    "qop": 3
  }
}
```

The alternative destination is `{"kind":"balanced","mshost":"message.example.invalid",
"sysid":"TST","group":"PUBLIC","msserv":"sapmsTST"}`. SNC can include `myName`;
quality-of-protection drafts accept privacy (3) or maximum (9), with no automatic fallback.
A future host must verify that the negotiated protection satisfies the organization's policy;
accepting a draft does not establish that any provider or server supports it. A `password`
authentication draft has no username/password fields: a future approved host credential
provider would own secrets. Unexpected credential fields are rejected without echoing them.
SAProuter, arbitrary RFC function calls, C ABI declarations and actual SDK loading are absent.

`crates/engine/src/analysis/rfc.rs` defines an internal `OlapAdapter` abstraction, not SAP C
signatures. Its mock-tested lifecycle releases a created dataset after success, failure or
cancellation. A request already canceled performs no creation. Cleanup failure discards the
result and requires host connection invalidation. This contract is not a live provider.

## Work-agent implementation sequence

1. Confirm whether the real use case is BI-platform-managed BW/HANA Analysis workbooks or
   universe/WebI documents. These are different analytical/repository interfaces. Record the
   supported BW version, client, query/provider, direct or message-server destination and
   required variables using technical names. Keep all actual landscape details private.
2. Obtain the appropriately licensed Windows x64 SAP NetWeaver RFC SDK and its matching
   headers/documentation from company-approved sources. Consult the current SDK landing page,
   platform/download Note 2573790 and Notes 1025361/1056696; verify target compiler/architecture
   support with Basis. Do not download, publish or vendor these
   files in this repository or its artifacts. Obtain an approved SNC provider separately.
3. Confirm a supported **documented OLAP BAPI/MDX** route on that target. Candidate discovery
   is `BAPI_MDPROVIDER_GET_CUBES`; candidate lifecycle is `BAPI_MDDATASET_CREATE_OBJECT`,
   `SELECT_DATA`, `GET_AXIS_INFO`, `GET_AXIS_DATA`, `GET_CELL_DATA`, `DELETE_OBJECT` (each with
   the `BAPI_MDDATASET_` prefix). Verify every signature and release status against the target;
   these names are a research roadmap, not implemented ABI or a promise of availability.
   Do not invoke SAP's internal BICS interfaces or claim that this reproduces AO internals.
4. Keep the existing L0–L5 crates SDK-free, safe Rust and WASM-compatible. The repository
   forbids unsafe code. A direct C FFI needs an explicitly reviewed narrow host boundary or
   a separate approved broker process; do not silently weaken `unsafe_code = forbid` or
   invent SDK structs. Resolve the licensed SDK path explicitly, verify its provenance and
   architecture, and avoid current-directory/PATH DLL search. Never load a workbook path.
5. Preserve the employee's approved identity with host-managed credentials/SNC. Distinguish
   RFC logon, `S_RFC` permissions and BW analysis authorizations. Scope connection state to the
   session/user/destination, handle expiry/disconnect/cancel, and invalidate connections when
   dataset cleanup cannot be confirmed. Reject downgrade rather than retrying anonymously.
   BOBJ repository authentication stays separate; do not reuse BW credentials/tokens across
   origins or extract AO/browser cookies. No shared service account is required by the demo.
6. Map axis metadata, member unique names, measures, typed values, units, nulls, errors and
   server totals explicitly. Preserve server aggregation semantics and authorization-filtered
   values; do not sum exception-aggregated measures or duplicate server totals locally.
   Implement BW variable defaults, exits, hierarchy selection, ranges/exclusions and mandatory
   validation with representative queries. The demo YEAR placeholder is not that contract.
7. Reuse the engine's async transport completion, cancellation, atomic refresh, saved definition
   and native pivot behaviors. Add sanitized error categories and structured private diagnostics
   without secrets or data values. Use two test users, one allowed and one denied, to verify
   observed server identities, authorization filtering, refresh/reconnect and no data leakage.
8. Before production: threat model, security/rights review, enterprise trust/proxy/SSO decisions,
   secure secret storage, dependency/vulnerability review, code signing, update policy,
   accessibility/performance, recovery and Windows user-acceptance testing remain necessary.

## Primary references for verification

- [SAP connector descriptions and licensing terms](https://support.sap.com/en/product/connectors.html.html)
  prohibit connector redistribution and require the applicable user/developer entitlements.
- [Current NetWeaver RFC SDK landing page](https://support.sap.com/en/product/connectors/nwrfcsdk.html)
  links current platform/download guidance and SDK API documentation; use it instead of an old
  copied platform list.
- [SAP NW RFC 7.50 Programming Guide](https://support.sap.com/content/dam/support/en_us/library/ssp/products/connectors/nwrfcsdk/NW_RFC_750_ProgrammingGuide.pdf)
  covers connection parameters, Windows runtime libraries and backend DDIC metadata lookup.
- [SAP OLAP BAPI sequence](https://help.sap.com/docs/SUPPORT_CONTENT/bwplaolap/3361383885.html).
- [SAP BW external interfaces/BICS boundary](https://help.sap.com/docs/SAP_NETWEAVER_750/64e2cdef95134a2b8870ccfa29cbedc3/48f318b441213697e10000000a42189b.html).
- [RFC/SNC connection parameters](https://help.sap.com/saphelp_em92/helpdata/de/48/b0ff6b792d356be10000000a421937/content.htm?no_cache=true).

The work agent must recheck these against the licensed SDK and actual supported landscape.
