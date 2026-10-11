# Work-agent handoff: Windows analytical workspace demo

Updated 2026-10-11. Public generic code only; no company systems, credentials or data.
Repository: https://github.com/BLTCPT/gridcraft
Branch: `feature/analysis-workspace`. Upstream: https://github.com/storytold/gridcraft
The portable artifact's `BUILD.txt` identifies its exact source commit. Use that commit to
reproduce its build; do not assume an arbitrary later branch head is the downloaded binary.

## Copyable handoff prompt

Continue the independent GridCraft Analysis fork on `feature/analysis-workspace` in an approved
company development environment. Prepare a reliable Windows x64 demonstration first, then
implement the organization's approved SAP BW RFC/SNC integration. Read `AGENTS.md`,
`docs/windows-analysis.md`, `docs/analysis-workspace.md`, `docs/rfc-adapter-design.md`, and
`docs/security-and-distribution-review.md` before editing. Preserve upstream MIT/Apache
copyright, attribution and generated dependency/font notices. The Rust/egui/wgpu architecture
is native; this is not an Excel COM/VSTO add-in and does not require Office for the demo.

Download the successful **Analysis Windows** workflow artifact for the requested commit from
https://github.com/BLTCPT/gridcraft/actions/workflows/analysis-windows.yml (GitHub sign-in may
be required for CI artifacts). Extract the outer Actions download, verify the included portable
ZIP against its adjacent SHA-256 file, then extract that ZIP into a user-writable directory.
Follow corporate policy for unsigned software; do not disable security controls. Open
`START-DEMO.cmd`, or run `gridcraft.exe --bw-demo`. It launches the sibling synthetic Rust
simulator on an available numeric loopback port, connects over actual HTTP, inserts a native
pivot report, and opens the Analysis pane. The app owns and stops its simulator when closed.
No SAP credentials, live endpoint, administrator installation or downloaded SAP runtime is used.

Demonstrate FY2026 revenue **2,553,600 USD**, switch YEAR to `[FiscalYear].[2025]` for
**2,349,600 USD**, refresh, swap axes and undo. Save as XLSX and reopen to show cached values
and native pivot persistence. A reopened workbook is deliberately disconnected; reconnect
through its matching running simulator endpoint before refresh. The in-process fixture mode
(`--analysis-demo`) is also available but is distinct from the HTTP-backed `--bw-demo`.

Run `smoke-analysis.ps1` from an approved PowerShell session for process-level save/reopen/
refresh and 401/403/expired-session rejection checks. Source tests in
`apps/gridcraft-bw-sim/tests/e2e.rs` additionally prove prior data/history survive malformed,
unauthorized, mixed-unit, duplicate, redirect, schema and timeout failures, canceled requests,
disconnects and edits during refresh. CI runs native egui interaction and MCP tests too.

For source development install Rust 1.95.0, Visual Studio C++ tools and the Windows SDK through
approved channels, add `wasm32-unknown-unknown`, then run:

```powershell
git clone https://github.com/BLTCPT/gridcraft.git
cd gridcraft
git switch feature/analysis-workspace
rustup target add wasm32-unknown-unknown
cargo xtask ci
.\packaging\windows\analysis-portable.ps1
.\target\analysis-windows-package\gridcraft.exe --bw-demo
```

The portable build writes `dist/analysis/gridcraft-analysis-windows-x64-portable.zip`, its
SHA-256, and `BUILD.txt`. It validates x64 PE/subsystems, performs real Windows process smoke
tests, and assembles notices from the actual Cargo compiler-artifact manifest. No release,
installer, upstream PR, SDK distribution or corporate deployment is authorized by this file.

Treat RFC/SNC as the intended production transport, subject to Basis confirmation. The RFC
provider currently reports **unavailable**: config validation and a mock cleanup contract are
present, but there is no SDK loading, RFC network call, SSO or BICS implementation. XMLA is an
optional experimental separate adapter, not a substitute for proving the actual RFC path.
Confirm whether the BOBJ use case is BW/HANA Analysis workbooks or universe/WebI; keep the
repository and analytical interfaces separate. Obtain the licensed SDK, approved SNC/provider
configuration and nonproduction destinations privately, without adding them to the public repo.
Verify documented OLAP BAPI signatures rather than guessing FFI or invoking internal BICS.
Use the [current SDK landing page](https://support.sap.com/en/product/connectors/nwrfcsdk.html)
and platform/download Note 2573790, plus the linked SDK Doxygen and
[programming guide](https://support.sap.com/content/dam/support/en_us/library/ssp/products/connectors/nwrfcsdk/NW_RFC_750_ProgrammingGuide.pdf).
Confirm runtime prerequisites and real function metadata through the approved SDK/BW target.

Implement behind the existing engine command/host-adapter seams. Preserve per-employee identity
and distinct RFC versus BW analysis authorizations. Validate with two nonproduction users, one
allowed and one denied a query/data slice, and confirm the observed identity on the server.
Preserve variables, hierarchy/aggregation/unit semantics, atomic refresh, error sanitization,
session cleanup and cancellation. Keep secrets out of commands, workbook metadata, logs,
history and public issue/PR text. All company-specific configuration, outputs and diagnostics
belong only in approved private locations. Do not install credentials or change auth/security
settings without the organization's authorization.

Use `cargo xtask ci` before every commit. Keep changes on a feature branch and use draft PRs
when asked to publish a review. Report exact commit, Windows build/test evidence, actual server
validation and remaining gaps. Describe this as an experimental demo until enterprise security,
authentication, licensing, signing, deployment and acceptance reviews have actually passed.
