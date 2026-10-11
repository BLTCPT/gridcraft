# Windows Analysis build and local BW protocol simulator

Updated 2026-10-11. Independent experimental GridCraft fork. Windows x64 is the first
packaged target; the Mac is a development host. No Microsoft Office installation, shell
runtime, SAP binary or ABAP runtime is bundled or required to run the executables.

## Download and run

**Status:** workflow and packaging are local and unrun on Windows because publication is
paused. No Windows executable artifact has been verified or delivered yet. Distribution
notices and distinct branding also remain under review.

When authorized, the `Analysis Windows` GitHub Actions workflow builds the exact feature-branch commit,
runs quality gates and network end-to-end tests on Windows, checks PE architecture and
GUI/console subsystem, and publishes an **unsigned portable ZIP as a CI artifact**.
This is not a public release or installer. Extract the ZIP into a user-writable directory.
`BUILD.txt` contains the source commit; the adjacent `.sha256` file verifies the download.
No administrator installation, file association changes or signing secrets are needed.

1. Start `gridcraft-bw-sim.exe` in a terminal. It prints its loopback URL. Keep it running.
2. Start `gridcraft.exe`, open **Analysis → Analysis workspace → Connection settings**.
3. Choose **Local XMLA simulator (synthetic)**, enter `http://127.0.0.1:8787/xmla`, click
   **Apply connection settings**, then **Connect**. Discovery completes asynchronously.
4. The supplied `SYNTHETIC_BW` catalog, MDX and field names work with the simulator.
   Click **Execute & insert BW report**. Expected FY2026 total: **2,553,600 USD**.
5. Change YEAR to `[FiscalYear].[2025]`, then **Apply YEAR member & refresh**.
   Expected total: **2,349,600 USD**. Refresh and Swap axes use the existing native pivot.
6. Save as XLSX. After reopening, select the same simulator connection and reconnect,
   then refresh. The saved query, cells and pivot persist; the connection does not.

`gridcraft-cli.exe` exposes the same commands headlessly. `gridcraft-cli.exe mcp` installs
the same transport in the MCP host. The desktop control channel uses those commands too.
Run `.\smoke-analysis.ps1` from PowerShell if your local policy permits scripts. The script launches the
simulator on port 18787 and verifies a real CLI → HTTP → pivot → XLSX → reopen/refresh path.
For ordinary GUI use no PowerShell script is necessary.

## Build from source on Windows

Install Rust 1.95.0 and the normal Visual Studio C++/Windows SDK build prerequisites.
From a Developer PowerShell in this checkout:

```powershell
cargo xtask ci
.\packaging\windows\analysis-portable.ps1
```

Output: `dist\analysis\gridcraft-analysis-windows-x64-portable.zip`. The package includes
`gridcraft.exe`, `gridcraft-cli.exe`, `gridcraft-bw.exe` (diagnostic request/response tool),
`gridcraft-bw-sim.exe`, licenses, attribution, documentation and a smoke script. The C runtime
is linked statically. The fork uses a separate portable package to avoid upstream MSI upgrade
identifiers and registry/file-association ownership. ARM64/x86 packages are not yet validated.

## Simulator scope and errors

This is an original Rust protocol simulator, **not SAP BW, ABAP or a clone of SAP's runtime**.
It implements a narrow synthetic XMLA/SOAP subset: catalog/cube/dimension/measure discovery,
synthetic variable metadata, and one deterministic sales query with two year selections.
It produces tabular XMLA rowsets; dimension fields feed GridCraft's pivot axes and typed
numeric cells. It is not a general MDX evaluator. Only numeric loopback is bound. Fixed Basic
credentials `synthetic` / `synthetic` protect against accidental unauthenticated clients;
they are public fixture values, not a security boundary. No BOBJ endpoint or token is emulated.

Run `gridcraft-bw-sim.exe --help` for scenarios. `--scenario fault`, `malformed`, `slow`,
`unauthorized`, `forbidden`, `expired`, `mixed-units`, `duplicate`, `redirect`, `wrong-namespace`,
`missing-measure`, and `multidimensional` exercise failures. Stop with Ctrl+C. Automated tests
switch scenarios after a successful query and assert that prior workbook values/history remain
unchanged. The `slow` scenario delays 700 ms; tests use a 75 ms client timeout. Production
transport times out after 20 s. Cancellation discards results immediately; a socket already
in flight may finish or time out in the background. It does not send a SAP Cancel command.

## Supported query semantics

The first client accepts an administrator-approved single MDX SELECT with tabular output.
Map technical rowset field names for a row dimension, column dimension and numeric additive
measure. Optional currency/unit mapping rejects multiple units. Duplicate tuples, missing
fields, non-numeric values, empty results and multidimensional cellsets are rejected rather
than silently producing misleading totals. A null measure is preserved as an empty source cell.
Use leaf results without server totals, hierarchy subtotals or exception-aggregated measures.

The YEAR control is an explicit **single-member MDX placeholder**, not SAP BW variable
processing. `MDSCHEMA_SAP_VARIABLES` metadata can be discovered, but the SAP `VARIABLES`
clause, variable exits/defaults/authorization, ranges and exclusion semantics are not yet
implemented. These require representative BW queries and server validation. Discovering a
variable does not make it supported. Do not interpret a blank member as clearing a BW default.

A true ABAP/BW runtime test requires a licensed, configured SAP environment. None is supplied
or redistributed. Successful local tests establish this client's behavior against synthetic
protocol fixtures, not AO/BICS compatibility or SAP certification.
