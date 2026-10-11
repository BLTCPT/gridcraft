# Windows third-party notices inputs

These are license/notice texts, not vendor binaries or source implementations. `sources.json`
records exact public upstream commit URLs, SHA-256 hashes and package versions for texts that
were omitted from published crate archives or require extra font attribution. No SAP SDK,
DLL, SNC library or confidential material is included.

`cargo xtask licenses --out target/windows-notices` derives the normal/build dependency graph
for the four Windows x64 executables from the locked manifest, checks known license-expression
choices, copies all matching crate license/notice texts, and verifies these supplemental files.
The Windows packager supplies Cargo's actual compiler-artifact JSON to identify the packages
and features built. Graph-only generation on a Mac is a preflight, not Windows binary validation.

The generated `THIRD-PARTY-NOTICES.txt`, `inventory.json` and license files are part of the
portable ZIP. Original terms remain verbatim. Retaining an alternative license's text does not
select it when an OR expression offers an alternative; selected terms are explicit per package.
The collector fails on unknown license expressions, packages without any notice text, altered
supplemental files or unexpected optional craft-fonts input. A licensing review must still
consider source-file notices, generated/native build inputs and product/contract-specific terms.
This is reproducible notice assembly, not legal clearance or a guarantee of exhaustive rights.
