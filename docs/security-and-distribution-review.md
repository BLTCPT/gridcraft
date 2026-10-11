# Authentication and distribution review — implementation facts

Updated 2026-10-11. This is an engineering inventory of the current prototype, not legal
clearance, enterprise security approval, SAP certification or a claim of production readiness.
No company credentials, systems, workbooks, branding or private data were used.

## Publication and Windows status

The public fork is `https://github.com/BLTCPT/gridcraft`. Generic feature-branch publication
was resumed by the owner after the initial review pause. `Analysis Windows` builds the exact
commit and publishes an unsigned portable CI artifact; consult its run and `BUILD.txt` for
terminal status/source identity. No production release, merge, deployment or upstream PR is
part of this work. Automated Windows validation and a human demo-PC check are distinct.

RFC/SNC is the leading intended production path. It remains **unavailable**, with a nonsecret
config validator and mock dataset lifecycle only. See `rfc-adapter-design.md`. The working
synthetic demo and optional live experimental transport below use XMLA; they do not establish
RFC, SNC, BICS or AO compatibility.

## Actual optional XMLA authentication behavior

| Area | Implemented behavior |
|---|---|
| BW transport | HTTP Basic `Authorization` on each XMLA request; no negotiated SSO or reusable SAP session |
| Live destination approval | Exact string match with locally supplied `GRIDCRAFT_BW_XMLA_URL`; the workbook/config alone cannot approve a destination |
| Credentials | `GRIDCRAFT_BW_USER` and `GRIDCRAFT_BW_PASSWORD` read from the launching process environment for each live request |
| Persistence | No password/token parameters accepted by analysis commands; no credential fields in workbook metadata, undo history or replay journal |
| Memory | Ordinary Rust/HTTP buffers and inherited environment; not a secret vault, not memory-zeroized, and accessible to the same OS user/process privileges |
| TLS | HTTPS required outside simulator; rustls verifies certificates against bundled public CA roots; no insecure bypass or OS trust-store modification |
| Enterprise CA/proxy | Custom enterprise CA, Windows trust integration, client certificates and authenticated proxy support are not implemented; transport currently disables proxies |
| Redirects | Rejected; no forwarding of Authorization to a redirected URL |
| Cookies/tokens | No cookie jar, browser/AO cookie extraction, SAML token exchange, BOBJ `X-SAP-LogonToken`, OAuth, SNC, Kerberos, SSPI/Negotiate or NTLM implementation |
| BOBJ separation | Optional repository URL is configuration only; no repository network calls or auth token sharing; BW XMLA is the only live analytical transport |
| Logs/errors | Sanitized network/TLS/timeouts and HTTP status errors; response bodies and credentials are not reflected in transport errors; no request/body logging added |
| Data output | Successful query results and technical selections can appear in workbook cells, CLI/MCP output and discovery state. These are data outputs, not redacted logs |
| Local simulator | Explicit provider selection, `http://127.0.0.1:PORT/xmla` only, fixed public synthetic credentials; never reads live credentials |
| Request safety | One pending request per document; at most eight transport workers per process; 20-second timeout; response size 8 MiB, 10,000 rows, 256 fields, depth 64 |
| Cancellation | Pending result is discarded immediately; an already running socket may finish/time out. No SAP Cancel/session protocol |
| Local automation | Existing optional desktop loopback control server and stdio MCP expose workbook commands/data. They are not enterprise identity or authorization boundaries |

No live endpoint or auth method has been validated. A successful Discover merely indicates
that a request completed, not that a particular employee identity or authorization model was
verified. There is no shared service account provisioned or installed by this task.

### Required authentication go/no-go before further real integration

Obtain the supported **nonproduction** connection flow from the organization's SAP/Basis and
identity owners: Windows RFC/SNC settings, company-provided licensed SDK, supported documented
OLAP BAPIs and test query. If XMLA is separately desired, confirm approved ICF authentication.
Windows login, BOBJ BI login and BW analytical identity are distinct. An AO login or browser
SAML session does not automatically grant an external client a BW session. Any future Windows
auth implementation must preserve employee identity and avoid implicit downgrade/fallback.

Run an early Windows test with **two separate authorized test users**: one allowed and one
denied a particular query/data slice. Verify the BW-observed identity, authorization-filtered
results and rejection behavior, without shared credentials or token reuse across origins.
Do not proceed to production based on successful synthetic tests or a service-account demo.
SSO/SNC, BI-to-BW assertion tickets, server mappings and token lifecycle require the actual
approved landscape; none is implemented or inferred here.

## Upstream license and trademark facts

- `LICENSE-MIT` grants use, copying, modification and distribution subject to retaining its
  copyright and permission notice. `LICENSE-APACHE` is offered as an alternative; its
  redistribution conditions, change notices, NOTICE handling, patent and trademark terms
  remain part of the file. `NOTICE` identifies the upstream copyright and this modified fork.
- `docs/brand/LICENSE-brand.txt` treats ArtCraft name/wordmark/marks as trademarks outside the
  open-source code license. It expressly requires removing the marks in published modified
  versions and prohibits presenting a derivative as an ArtCraft product. It permits plain-text
  provenance saying the work is based on GridCraft by the ArtCraft team.
- The eight ArtCraft graphic files were removed locally. Promotional ArtCraft README blocks
  were also removed. Required copyright, license, NOTICE and provenance text remain.
- The app name still uses **GridCraft Analysis (experimental fork)** as a provisional label.
  No new product name has been chosen, no trademark clearance has been performed, and no
  organization affiliation is asserted. Removing a word or renaming the product would not
  remove the provenance or third-party obligations of retained code.
- `AGENTS.md` mandates clean-room implementation and forbids proprietary vendor assets/code.
  No SAP or Microsoft binary/source was examined or redistributed. The simulator is original
  Rust code using synthetic fixtures, not an ABAP runtime.

## Asset and dependency inventory

`ATTRIBUTION.md` records repository assets. The Windows icon `assets/app-icon/gridcraft.ico`
is an original GridCraft placeholder under MIT OR Apache-2.0 according to
`assets/app-icon/LICENSE.txt`; it is not one of the restricted ArtCraft brand files. The
legacy upstream MSI identifies Learning Machines LLC and uses the upstream UpgradeCode.
The new prototype uses a separate unsigned portable ZIP and does not call that MSI packager.
Its EXE CompanyName was changed locally to GridCraft contributors. Legacy upstream packaging
is retained for provenance but must not be used to ship an independently branded installer.

`docs/dependency-license-inventory.tsv` records **243 locked Windows normal/build dependency
packages** in the four-executable graph preflight. Unlike metadata-only reachability, this
uses Cargo's selected target/features. The final Windows bundle instead uses the actual build's
`compiler-artifact` records, rejects test/non-Windows executable manifests and requires all four
successful release binaries. Host build dependencies are included. This is not a complete SBOM.

`cargo xtask licenses` copies crate license/notice texts and verifies 18 pinned upstream
supplemental texts (including egui fallback fonts), with per-file SHA-256 and explicit selected
license alternatives. The preflight assembled **459 original texts across 243 packages**.
`third-party/THIRD-PARTY-NOTICES.txt`, `inventory.json` and individual files are included in the
portable ZIP. Unknown expressions, no notice texts, changed supplemental hashes, stale output
or optional craft-fonts inputs fail the packager. `legal/sources.json` records provenance.

Notable declarations requiring separate review/notice handling:

- `epaint_default_fonts 0.36.2`: `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`.
  Its source embeds Hack, Noto Emoji, Ubuntu Light and emoji-icon-font bytes. Therefore a
  claim that the binary contains no third-party fonts is incorrect even without craft-fonts.
- `ring 0.17.14`: `Apache-2.0 AND ISC`, with additional LICENSE-other-bits/NOTICE content.
- ICU-related crates: `Unicode-3.0`; `unicode-ident` includes Unicode terms alongside MIT/Apache.
- `webpki-roots 1.0.9`: `CDLA-Permissive-2.0` for bundled CA root material.
- `self_cell 1.3.0`: `Apache-2.0 OR GPL-2.0-only`; it offers an Apache alternative. A GPL string
  in an OR expression is not evidence that the application is GPL-only.
- Other declarations include MIT, Apache-2.0, ISC, BSD, Zlib, BSL-1.0 and Unlicense alternatives.

The collector assembles reviewed inputs reproducibly; it cannot by itself guarantee exhaustive
source-file/native-toolchain rights or legal approval. Build-specific package/features are
recorded for review. Retaining both alternatives' texts does not select a GPL alternative when
Apache is explicitly selected. No SAP SDK, SNC provider, proprietary font or vendor artwork was
added. No legal conclusion about any organization's contracts or SAP entitlements is made.

## Evidence

- Local `cargo xtask ci`: all seven gates passed; **622 passed, 0 failed, 1 ignored** after
  hardening. Windows results are recorded separately by the exact-commit workflow run.
- Six actual-loopback E2E tests cover native pane, MCP, discovery, member selections, pivots,
  refresh, undo, save/reopen, endpoint policy and error preservation. The 11 injected failure
  modes are synthetic; they are not captured SAP server responses.
- The native Mac app opened the simulator-produced XLSX, reconnected, refreshed successfully
  and displayed a synthetic total of USD 2,553,600 with refresh count 2.
- Additional local process checks rejected a temporary self-signed TLS server before any HTTP
  credentials were transmitted, omitted synthetic credential sentinels from errors, and proved
  the simulator ignores a different set of synthetic live credentials in its launch environment.
- Independent local security review hardened SOAP response nesting, namespace/attribute and
  XML validity checks, with regression tests. Closing a document or dropping its session now
  cancels queued requests; the transport checks cancellation before sending HTTP.
- RFC draft commands reject credentials, do not journal config, and prove that SDK/network
  capabilities remain false. Mock dataset cleanup is exercised on success/failure/cancellation.
- One-command network demo uses an owned sibling simulator, ephemeral loopback port, bounded
  startup handshake, no inherited live credentials and cleanup when the native app closes.
- Windows CI includes the native demo bootstrap, PE/subsystem validation, process-level
  round trip and authentication rejection checks. Check the actual run result before use.

The first Windows run passed 620 tests (0 failed, 1 ignored) and six gates, then exposed
CRLF conversion in the strict generated-parity comparison. The check now normalizes CRLF
only, with a regression test that still rejects content/trailing-newline changes. Git attributes
preserve Cargo.lock/parity LF and exact supplemental-license bytes across platforms. A simulated
`core.autocrlf=true` checkout verified all 18 supplemental hashes and the Windows batch launcher.
