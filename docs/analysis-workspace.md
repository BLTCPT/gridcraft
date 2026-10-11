# Analysis workspace — experimental BW integration foundation

Last updated: 2026-10-11. Based on GridCraft `9f896c1249f40d18bf182199dc03042959be5f3f`.

The native Analysis tab implements an AO-style analytical workflow inside GridCraft. The
interactive providers include a built-in fixture, a local XMLA simulator and an experimental
read-only BW XMLA transport. No SAP server, BOBJ repository, RFC/BICS session, or
SAP Analysis for Office workbook has been validated. This is an independent implementation,
not a port of SAP binaries or source and not an SAP-certified client.

## Run

For the HTTP-backed one-command demo, build `gridcraft` and `gridcraft-bw-sim`, then run
`target/debug/gridcraft --bw-demo` (Windows: `.exe`). The portable Windows package contains
`START-DEMO.cmd`. See [the work-agent handoff](work-agent-handoff.md) and
[unavailable RFC scaffold/design](rfc-adapter-design.md). The command below is the separate
in-process fixture mode.

Use Rust **1.95.0** (pinned in `rust-toolchain.toml`), matching the pinned egui 0.36 dependencies.
The upstream workspace declared Rust 1.90, but its dependencies require 1.95. On the connected
Mac, `tools/cargo-local` selects the toolchain installed in the task's sibling `.rustup` directory;
it does not change the user's default Rust toolchain. Elsewhere it delegates to normal Cargo.

```sh
tools/cargo-local run -p gridcraft -- --analysis-demo
```

The command opens a new synthetic analysis workbook directly. In an ordinary launch, choose
**Analysis → Connect demo → Analysis workspace → Insert analysis**. The pane also
has Connect/Disconnect, variable choices (fiscal year and region), row/column dimensions,
measure selection, and Apply & refresh. Ribbon actions refresh or swap the axes.

The synthetic query has 24 original records in each of FY2025 and FY2026. FY2026 revenue
totals USD 2,553,600; filtering Americas gives USD 579,200. Choose Units to verify an additive
non-currency measure. No company data, downloaded workbook, SAP artwork or credentials
are included. The demo is deterministic: refresh does not invent new market/server values.

## Architecture implemented

| Layer | Actual files | Responsibility |
|---|---|---|
| L2 model | `crates/model/src/analysis.rs`, `workbook.rs` | Typed query definition, report binding, COW output snapshots and versioned serializable saved definition |
| L2 XLSX | `crates/xlsx/src/analysis.rs` | GridCraft-owned `xl/gridcraft/analysis.json` extension, size-limited read/write; no engine or network dependency |
| L5 engine | `crates/engine/src/analysis.rs` | Separate analytical-provider and workbook-repository interfaces; synthetic implementations; ephemeral connection state |
| L5 engine | `crates/engine/src/cmd/analysis.rs` | Discovery, metadata, variables, insert, atomic refresh, pivot axes, history and input validation |
| L5 engine | `crates/engine/src/analysis/xmla.rs` | Toolkit-free XMLA request generation and bounded tabular response decoding |
| L6 egui | `crates/ui-egui/src/analysis_pane.rs` | Thin native panel, reads state and invokes engine commands; no SAP protocol or data calculation |
| L6 MCP / L7 CLI | existing `execute_command` / `run` interfaces | Same registered engine commands, no duplicated business logic |
| L6 native transport | `crates/bw-client/src/lib.rs` | Background HTTPS worker shared by desktop, CLI/MCP and probe; credentials never enter the engine |
| L7 simulator | `apps/gridcraft-bw-sim` | Portable synthetic loopback HTTP/SOAP service and native UI/MCP/network E2E tests |
| L7 diagnostic host | `apps/gridcraft-bw/src/main.rs` | Optional HTTPS transport around the engine XMLA codec; no UI, workbook model, or alternative analysis implementation |

`cargo xtask layers` registers the diagnostic host at L7. No layer below L6 imports egui,
eframe, winit or rfd. The protocol code also compiles for WASM. HTTP is confined to the host-injected native transport; no blocking network call runs in the
egui frame loop. The engine queues requests and applies completions via `analysis.poll`.
Headless callers await that same completion command. Cancellation invalidates the pending
result; any workbook edit during the request causes its result to be discarded.

See [Windows build and simulator instructions](windows-analysis.md) for the native deliverable,
launch steps, supported semantics and reproducible failure tests.

## Commands and automation

```sh
tools/cargo-local run -p gridcraft-cli -- commands --search analysis
mkdir -p dist
tools/cargo-local run -p gridcraft-cli -- run \
  --cmd 'analysis.configure={"provider":"fixture"}' \
  --cmd 'analysis.connect={}' \
  --cmd 'analysis.insert={"query":"demo.sales","fiscalYear":2026,"rows":"Region","columns":"Product","measure":"Revenue"}' \
  --out dist/analysis-demo.xlsx
```

These same commands are available to MCP and the desktop JSON control channel:

| Command | Parameters / result |
|---|---|
| `analysis.configure` | `provider`, optional separate `analyticalUrl` and `repositoryUrl`; always disconnects |
| `analysis.connect`, `analysis.disconnect`, `analysis.status` | Empty object; explicit synthetic/live status |
| `analysis.queries` | Synthetic query catalog; does not list repository documents as analytical queries |
| `analysis.metadata` | `query`; dimensions, measures, required variables, available members and limits |
| `analysis.workbooks` | Synthetic repository examples; not real BOBJ objects |
| `analysis.insert` | `query`, required `fiscalYear`, optional `region`, `rows`, `columns`, `measure` |
| `analysis.variables` | `fiscalYear`, optional `region`; validate and refresh together |
| `analysis.layout` | `rows`, `columns`, `measure`; validate and refresh together |
| `analysis.refresh`, `analysis.swapAxes` | Empty object |
| `analysis.discover` | `rowset`, optional `catalog`, `cube`; real configured transport, raw technical metadata |
| `analysis.bwVariables` | `variables` map; bounded single-member MDX placeholders, not SAP variable processing |
| `analysis.poll`, `analysis.cancel` | Complete or discard a pending request; completion is an atomic undo step |
| `analysis.xmla.discoverRequest` | `rowset`, optional `catalog`, `cube`; builds SOAP, never sends |
| `analysis.xmla.executeRequest` | `catalog`, `mdx`; builds restricted read-only SOAP, never sends |
| `analysis.xmla.parse` | `xml`; returns bounded rowset, does not import into cells |
| `analysis.rfc.capabilities` | Reports unavailable RFC capability and prerequisites; no network or SDK loading |
| `analysis.rfc.validateConfig` | Validates a nonsecret RFC/SNC draft only; never connects or persists it |

All analysis commands reject unknown fields and avoid the replay journal. Connection commands
accept no passwords or tokens. The XMLA providers connect through native transport. The BW RFC/OLAP and HANA InA adapters
remain unavailable and return explicit errors. Errors never imply live success.

## Report ownership, refresh and persistence

Insert creates new, uniquely named source and report sheets, preserving existing sheets.
One managed analysis per workbook is currently supported. The report is a native GridCraft
PivotTable over typed source values, with an explicit synthetic label and refresh counter.
Variables and layout are applied to a COW workbook copy; failure retains the prior definition,
values and undo history. Successful changes are one undo step. Refresh replaces the complete
owned output and removes obsolete rows. It refuses protected workbooks, renamed/deleted
analysis sheets, and edits to managed sheets, including changes made in the normal pivot pane.
Use the Analysis controls for managed layout changes; use a copy for freeform editing.

XLSX retains cells, a standard PivotTable, and this fork's **own versioned definition**. The
128 KiB optional metadata part stores non-secret definitions, including the exact BW endpoint
and technical query selections, but never users, passwords or tokens. Reopened files are
disconnected; configure the same provider/endpoint and connect explicitly before refreshing.
Do not publish workbooks containing sensitive server names or query selections. Unknown or malformed
metadata is ignored with a warning, preserving cached values. Content fingerprints detect
saved cell edits and block destructive refresh; these are accidental-change checks, not
cryptographic authenticity. Other spreadsheet software may discard this private extension.

This does **not** preserve SAP AO data-source bindings, prompts, planning settings, `.sapaox`
launch behavior, macros or BICS semantics. Existing AO XLSX files may expose cached cells
through the upstream importer; unmodelled parts can be lost on save. Use copies, and do not
claim AO workbook round-trip compatibility.

## BW XMLA probe

SAP documents XMLA as an optional BW interface exposing Discover and Execute/MDX. This is
separate from AO's usual stateful RFC/BICS access. Availability and query behavior must be
confirmed on the actual BW installation.

```sh
# Offline request preview: no server or credentials required.
tools/cargo-local run -p gridcraft-bw -- discover --rowset MDSCHEMA_CUBES
# Technical IDs supplied by the administrator, not display captions:
tools/cargo-local run -p gridcraft-bw -- discover --rowset MDSCHEMA_MEASURES --catalog BW --cube TECHNICAL_QUERY_ID
# Single SELECT only; no WITH, comments, semicolons, planning or session writes:
tools/cargo-local run -p gridcraft-bw -- execute --catalog BW --mdx-file /private/path/query.mdx
# Decode an administrator-provided, sanitized tabular response offline:
tools/cargo-local run -p gridcraft-bw -- parse --response-file /private/path/response.xml
```

Network access requires **`--send`** and locally supplied process environment variables
`GRIDCRAFT_BW_XMLA_URL`, `GRIDCRAFT_BW_USER`, `GRIDCRAFT_BW_PASSWORD`. Never paste credentials
into chat, command arguments, repository files or workbook cells. This first host supports
Basic authentication over verified HTTPS with the bundled public CA roots only; no SSO/SNC, password persistence, redirects,
certificate bypass, SAP logon-ticket forwarding or OS trust-store installation. The native transport additionally pins credentials to the exact locally approved
`GRIDCRAFT_BW_XMLA_URL`; workbook URLs cannot redirect them. Simulator mode uses only fixed
synthetic credentials on numeric loopback. A read-only server account remains required; the SELECT syntax gate is not a substitute for authorization.

Requests time out after 20 seconds (10 seconds to connect). Responses are capped at 8 MiB,
10,000 rows, 256 fields per row and nesting depth 64. DTDs, SOAP errors, malformed XML,
duplicate fields and unsupported multidimensional cellsets produce errors. Technical names,
captions, formatted numeric strings and null values remain separate rowset fields. The probe
prints JSON. The native desktop, CLI and MCP also support the result-to-workbook bridge using
`analysis.insert` with an optional `bw` definition. Network calls are tested against the local
synthetic simulator; no live SAP endpoint has been validated. Response JSON can contain company
data when pointed at a real server; keep it outside this public fork.

## Required to validate the real BW/BOBJ path

1. BW product/version and support package; BOBJ BI platform version; AO version used as the
   behavioral reference. Confirm read-only queries versus planning (planning is out of scope).
2. Administrator-approved HTTPS BW XMLA service URL (commonly `/sap/bw/xml/soap/xmla`), whether
   XMLA and external query access are enabled, and whether tabular responses are supported.
   If XMLA is unavailable, choose a supported SDK/RFC route with its licensing and deployment
   requirements before implementing transport.
3. Approved authentication method, locally provisioned read-only test account, required network
   access and standard trusted certificate chain. Credentials should not be supplied in chat.
4. One non-sensitive technical catalog/query ID; technical dimension, hierarchy and measure IDs;
   required variables and known selections; units/currencies; expected row counts and totals
   from AO for the exact same selections. Check authorization differences and empty/error cases.
5. For BOBJ repository work: separate `/biprws` endpoint, authentication policy and a test folder/
   document ID with rights. BI REST auth/content listing is not an analytical execution API.
6. A sanitized workbook copy and permitted validation dataset if AO file behavior is required.
   Test cells/metadata preservation independently; do not save over production workbooks.

## Sources and clean-room scope

- [GridCraft repository and license](https://github.com/storytold/gridcraft): Rust/egui layering,
  commands, history and native PivotTable engine reused under MIT OR Apache-2.0.
- [SAP BW XMLA interface](https://help.sap.com/docs/SAP_BPC_VERSION_BW4HANA/dd104a87ab9249968e6279e61378ff66/9f9b8f3c855f0454e10000000a11405a.html): optional Discover/Execute route.
- [SAP BI platform REST SDK](https://help.sap.com/docs/SAP_BUSINESSOBJECTS_BUSINESS_INTELLIGENCE_PLATFORM/db6a17c0d1214fd6971de66ea0122378/45aa774b6e041014910aba7db0e91070.html): repository/authentication boundary.
- [SAP Analysis for Office administration](https://help.sap.com/docs/SAP_BUSINESSOBJECTS_ANALYSIS_OFFICE/df90d3701aba42b9a4351caa387bd672/7f4485614983471eaced76418b587242.html?locale=en-US): AO connection architecture.
- [XMLA Discover](https://learn.microsoft.com/en-us/analysis-services/xmla/xml-elements-methods-discover)
  and [Execute](https://learn.microsoft.com/en-us/analysis-services/xmla/xml-elements-methods-execute):
  SOAP namespace/action and standard envelope shapes. These examples specify XMLA, not SAP
  server compatibility; target SAP behavior still requires testing.

No proprietary SAP or Microsoft binary/source/asset was read or copied. ArtCraft trademark
graphics were removed from this modified branch per the original brand terms; upstream code,
asset licenses and copyright notices remain. No deployment, upstream PR, SAP write or public
release was performed. Generic implementation publication on the public feature branch was
authorized and then paused for authentication/licensing review. No custom commit has been
pushed; no company data or unrelated installer files are included.

## BW variable boundaries

- [SAP variable discovery rowset](https://help.sap.com/docs/SAP_BPC_VERSION_BW4HANA/dd104a87ab9249968e6279e61378ff66/ba12533d6dd60610e10000000a114084.html)
- [SAP MDX VARIABLES clause](https://help.sap.com/docs/SAP_BPC_VERSION_BW4HANA/dd104a87ab9249968e6279e61378ff66/a718533d6dd60610e10000000a114084.html)

These are separate from ordinary MDX WHERE members. This prototype exposes discovery and
explicit member substitution, not the full SAP variable processor. Technical IDs remain strings.
