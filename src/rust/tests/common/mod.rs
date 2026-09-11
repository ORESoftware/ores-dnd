#![allow(dead_code)]
use std::path::{Path, PathBuf};

pub fn contracts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts")
}

pub fn read(path: impl AsRef<Path>) -> String {
    std::fs::read_to_string(path.as_ref()).unwrap_or_else(|e| panic!("read {}: {e}", path.as_ref().display()))
}

/// (declaration, expectation, file name, json) for every corpus instance.
pub fn corpus() -> Vec<(String, &'static str, String, String)> {
    let mut out = Vec::new();
    let root = contracts_dir().join("instances");
    let mut decls: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().collect();
    decls.sort_by_key(|d| d.file_name());
    for decl in decls {
        let name = decl.file_name().to_string_lossy().into_owned();
        for (lane, expectation) in [("valid", "accepted"), ("invalid", "rejected")] {
            let dir = decl.path().join(lane);
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect();
            files.sort();
            for file in files {
                out.push((name.clone(), expectation, file.file_name().unwrap().to_string_lossy().into_owned(), read(&file)));
            }
        }
    }
    out
}
