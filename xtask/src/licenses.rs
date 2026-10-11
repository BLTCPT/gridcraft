//! Deterministic notice assembly, not a legal-approval or source-level license scanner.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};
const TARGET: &str = "x86_64-pc-windows-msvc";
const ROOTS: &[&str] = &["gridcraft", "gridcraft-cli", "gridcraft-bw", "gridcraft-bw-sim"];
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(path: &Path) -> Result<Vec<u8>, String> {
    let size = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if size > 2 * 1024 * 1024 {
        return Err(format!("Oversized notice: {}", path.display()));
    }
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}
fn relative(root: &Path, name: &str) -> Result<PathBuf, String> {
    let p = Path::new(name);
    if p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err("Unsafe notice path.".into());
    }
    let path = root.join(p).canonicalize().map_err(|e| format!("Notice path: {e}"))?;
    if !path.starts_with(root.canonicalize().map_err(|e| e.to_string())?) {
        return Err("Notice path escapes root.".into());
    }
    Ok(path)
}
fn terms(expr: &str) -> Result<&'static str, String> {
    Ok(match expr {
        "MIT"
        | "MIT OR Apache-2.0"
        | "Apache-2.0 OR MIT"
        | "MIT/Apache-2.0"
        | "Apache-2.0/MIT"
        | "Unlicense OR MIT"
        | "0BSD OR MIT OR Apache-2.0"
        | "Zlib OR Apache-2.0 OR MIT"
        | "MIT OR Apache-2.0 OR Zlib"
        | "MIT OR Zlib OR Apache-2.0"
        | "BSD-2-Clause OR Apache-2.0 OR MIT" => "MIT",
        "Apache-2.0" | "Apache-2.0 OR ISC OR MIT" | "BSD-3-Clause OR Apache-2.0" | "Apache-2.0 OR BSL-1.0" | "Apache-2.0 OR GPL-2.0-only" => {
            "Apache-2.0"
        }
        "Apache-2.0 AND MIT" => "Apache-2.0 AND MIT",
        "Apache-2.0 AND ISC" => "Apache-2.0 AND ISC",
        "BSL-1.0" => "BSL-1.0",
        "Zlib" => "Zlib",
        "Unicode-3.0" => "Unicode-3.0",
        "ISC" => "ISC",
        "BSD-3-Clause" => "BSD-3-Clause",
        "(MIT OR Apache-2.0) AND Unicode-3.0" => "MIT AND Unicode-3.0",
        "(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0" => "MIT AND OFL-1.1 AND Ubuntu-font-1.0; also retain bundled Hack/Bitstream Vera terms",
        "CDLA-Permissive-2.0" => "CDLA-Permissive-2.0",
        _ => return Err(format!("Unreviewed license expression: {expr}. Update the explicit notice policy after review.")),
    })
}
fn notice_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    ["LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT", "AUTHORS"].iter().any(|p| n.starts_with(p))
}
fn collect(root: &Path, dir: &Path, depth: usize, visited: &mut usize, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 16 {
        return Err("Dependency directory nesting exceeds notice scan limit.".into());
    }
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        *visited += 1;
        if *visited > 50_000 {
            return Err("Dependency file count exceeds notice scan limit.".into());
        }
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_symlink() {
            return Err("Unexpected symlink in dependency source.".into());
        }
        if ty.is_dir() {
            collect(root, &entry.path(), depth + 1, visited, out)?;
        } else if notice_name(&entry.file_name().to_string_lossy()) && ty.is_file() {
            out.push(entry.path().strip_prefix(root).map_err(|e| e.to_string())?.to_owned());
        }
    }
    Ok(())
}
fn tree_packages(text: &str) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let (package, features) = line.split_once('|').ok_or("Unrecognized cargo tree output.")?;
        let mut parts = package.split_whitespace();
        let name = parts.next().ok_or("Missing tree package.")?;
        let version = parts.next().and_then(|s| s.strip_prefix('v')).ok_or("Missing tree version.")?;
        let features = features.trim_end_matches(" (*)");
        out.entry(format!("{name}@{version}")).or_default().extend(features.split(',').filter(|f| !f.is_empty()).map(str::to_owned));
    }
    Ok(out)
}
fn command_output(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = crate::cargo().args(args).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("Cargo notice input failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(out.stdout)
}
/// Optional compiler-artifact input is produced by the exact Windows package build, not tests.
fn artifact_packages(path: &Path, packages: &[Value]) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_artifacts(text.trim_start_matches('\u{feff}'), packages)
}
fn parse_artifacts(text: &str, packages: &[Value]) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut binaries = BTreeSet::new();
    let mut finished = false;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let value: Value = serde_json::from_str(line).map_err(|_| "Non-JSON line in Cargo artifact manifest.")?;
        if value["reason"] == "build-finished" {
            finished = value["success"] == true;
        }
        if value["reason"] != "compiler-artifact" {
            continue;
        }
        if value["profile"]["test"] == true {
            return Err("Test artifacts cannot define a distribution notice bundle.".into());
        }
        let package = packages.iter().find(|p| p["id"] == value["package_id"]).ok_or("Unknown compiled package in notice manifest.")?;
        let name = package["name"].as_str().ok_or("Compiled package missing name.")?;
        if ROOTS.contains(&name) && value["target"]["kind"].as_array().is_some_and(|k| k.iter().any(|v| v == "bin")) {
            let executable = value["executable"].as_str().ok_or("Missing Windows executable in artifact manifest.")?.replace('\\', "/");
            if !executable.ends_with(&format!("/{TARGET}/release/{name}.exe")) {
                return Err("Distribution manifest contains a non-Windows-x64 release executable.".into());
            }
            binaries.insert(name.to_owned());
        }
        let key = format!("{name}@{}", package["version"].as_str().ok_or("Missing compiled package version.")?);
        out.entry(key).or_default().extend(value["features"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_owned));
    }
    if !finished || binaries.len() != ROOTS.len() {
        return Err("Notice manifest must come from a successful build of all four executables.".into());
    }
    Ok(out)
}
pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    let mut output = None;
    let mut artifacts = None;
    for pair in args.chunks(2) {
        match pair {
            ["--out", path] => output = Some(PathBuf::from(path)),
            ["--artifacts", path] => artifacts = Some(PathBuf::from(path)),
            _ => return Err("Use licenses --out PATH [--artifacts PATH].".into()),
        }
    }
    let output = output.ok_or("Missing --out directory.")?;
    if output.exists() {
        return Err("Notice output directory already exists; use a fresh directory to prevent stale files.".into());
    }
    if std::env::var_os("CRAFT_FONTS_DIR").is_some() || root.parent().is_some_and(|p| p.join("craft-fonts").exists()) {
        return Err("Optional craft-fonts input requires its own verified notice set before packaging.".into());
    }
    let meta: Value = serde_json::from_slice(&command_output(&["metadata", "--locked", "--format-version", "1", "--filter-platform", TARGET])?)
        .map_err(|e| e.to_string())?;
    let packages = meta["packages"].as_array().ok_or("Missing Cargo packages.")?;
    let mut tree_args = vec!["tree", "--locked", "--target", TARGET, "--edges", "normal,build", "--prefix", "none", "--format", "{p}|{f}"];
    for name in ROOTS {
        tree_args.extend(["-p", name]);
    }
    let tree = String::from_utf8(command_output(&tree_args)?).map_err(|e| e.to_string())?;
    let selected = if let Some(path) = &artifacts { artifact_packages(path, packages)? } else { tree_packages(&tree)? };
    let supplemental: Value = serde_json::from_slice(&read(&root.join("legal/sources.json"))?).map_err(|e| e.to_string())?;
    let supplements = supplemental["sources"].as_array().ok_or("Missing supplemental license sources.")?;
    for source in supplements {
        let path = relative(root, source["file"].as_str().ok_or("Missing source file.")?)?;
        if sha(&read(&path)?) != source["sha256"].as_str().unwrap_or_default() {
            return Err(format!("Supplemental notice hash mismatch: {}", path.display()));
        }
    }
    let mut entries = Vec::new();
    let mut files: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    let mut combined = String::from(
        "THIRD-PARTY NOTICES — GridCraft Analysis experimental fork\n\nOriginal license and notice texts follow. This bundle is notice assembly, not legal clearance.\nAlternative license choices and supplemental font terms are recorded per package.\n\n",
    );
    for (key, features) in selected {
        let package = packages
            .iter()
            .find(|p| format!("{}@{}", p["name"].as_str().unwrap_or_default(), p["version"].as_str().unwrap_or_default()) == key)
            .ok_or_else(|| format!("Unknown package {key}"))?;
        if package["source"].is_null() {
            continue;
        }
        let expression = package["license"].as_str().ok_or_else(|| format!("{key} needs manual license-file review."))?;
        let selected_terms = terms(expression)?;
        let manifest = Path::new(package["manifest_path"].as_str().ok_or("Missing package manifest.")?);
        let pkg_root = manifest.parent().ok_or("Missing package directory.")?;
        let mut paths = Vec::new();
        collect(pkg_root, pkg_root, 0, &mut 0, &mut paths)?;
        paths.sort();
        let mut texts = Vec::new();
        for path in paths {
            let bytes = read(&pkg_root.join(&path))?;
            texts.push((path.to_string_lossy().replace('\\', "/"), None, bytes));
        }
        for source in supplements.iter().filter(|s| s["packages"].as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(&key)))) {
            let file = source["file"].as_str().ok_or("Missing supplemental file.")?;
            texts.push((file.into(), source["url"].as_str().map(str::to_owned), read(&relative(root, file)?)?));
        }
        if texts.is_empty() {
            return Err(format!("{key} has no license/notice text; review is required."));
        }
        combined.push_str(&format!("\n============================================================\n{key}\nDeclared: {expression}\nSelected: {selected_terms}\nRepository: {}\n",package["repository"].as_str().unwrap_or("not declared")));
        let mut notices = Vec::new();
        for (i, (source, url, bytes)) in texts.into_iter().enumerate() {
            let text = std::str::from_utf8(&bytes).map_err(|_| format!("Non-UTF8 notice in {key}: {source}"))?;
            let path = PathBuf::from("licenses").join(&key).join(format!("{:03}.txt", i + 1));
            combined.push_str(&format!("\n--- {source} ---\n{text}\n"));
            notices.push(json!({"file":path.to_string_lossy().replace('\\',"/"),"originalPath":source,"sourceUrl":url,"sha256":sha(&bytes)}));
            files.push((path, bytes));
        }
        entries.push(json!({"package":key,"declaredLicense":expression,"selectedTerms":selected_terms,"repository":package["repository"],"source":package["source"],"features":features,"notices":notices}));
    }
    let count = entries.len();
    let inventory = json!({"schemaVersion":1,"target":TARGET,"executables":ROOTS,"scope":if artifacts.is_some(){"compiler-artifacts"}else{"cargo-tree-preflight"},"cargoLockSha256":sha(&read(&root.join("Cargo.lock"))?),"packages":entries,"limitations":"Source-file/native-toolchain rights review and actual Windows runtime validation remain separate. No SAP SDK/runtime included."});
    files.push(("THIRD-PARTY-NOTICES.txt".into(), combined.into_bytes()));
    files.push(("inventory.json".into(), serde_json::to_vec_pretty(&inventory).map_err(|e| e.to_string())?));
    for (path, bytes) in files {
        let target = output.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&target, bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    println!("Verified notice bundle: {count} packages, {} ({})", output.display(), inventory["scope"]);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn choices_fail_closed_and_keep_and_obligations() {
        assert!(terms("GPL-3.0-only").is_err());
        assert_eq!(terms("Apache-2.0 OR GPL-2.0-only").unwrap(), "Apache-2.0");
        assert_eq!(terms("Apache-2.0 AND ISC").unwrap(), "Apache-2.0 AND ISC");
    }
    #[test]
    fn tree_deduplicates_and_unifies_features() {
        let p = tree_packages("x v1.2.3|a,b\nx v1.2.3|a,c (*)\nmacro v2.0 (proc-macro)|\n").unwrap();
        assert_eq!(p["x@1.2.3"].len(), 3);
        assert_eq!(p.len(), 2);
        assert!(tree_packages("not a tree").is_err());
    }
    #[test]
    fn paths_cannot_escape_input() {
        assert!(relative(Path::new("."), "../LICENSE").is_err());
        assert!(relative(Path::new("."), "/LICENSE").is_err());
    }
    #[test]
    fn sha256_known_vector() {
        assert_eq!(sha(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
    #[test]
    fn artifacts_require_all_release_windows_binaries_and_success() {
        let packages: Vec<Value> = ROOTS.iter().map(|n| json!({"id":n,"name":n,"version":"1.0"})).collect();
        let artifacts: Vec<Value> = ROOTS.iter().map(|n| json!({"reason":"compiler-artifact","package_id":n,"target":{"kind":["bin"]},"profile":{"test":false},"executable":format!("C:/build/target/{TARGET}/release/{n}.exe"),"features":["default"]})).collect();
        let manifest = |values: &[Value], success: bool| {
            values.iter().chain([&json!({"reason":"build-finished","success":success})]).map(Value::to_string).collect::<Vec<_>>().join("\n")
        };
        assert_eq!(parse_artifacts(&manifest(&artifacts, true), &packages).unwrap().len(), 4);
        assert!(parse_artifacts(&manifest(&artifacts, false), &packages).is_err());
        assert!(parse_artifacts(&manifest(&artifacts[..3], true), &packages).is_err());
        let mut invalid = artifacts.clone();
        invalid[0]["profile"]["test"] = json!(true);
        assert!(parse_artifacts(&manifest(&invalid, true), &packages).is_err());
        invalid = artifacts.clone();
        invalid[0]["executable"] = json!("/tmp/target/release/gridcraft");
        assert!(parse_artifacts(&manifest(&invalid, true), &packages).is_err());
    }
}
