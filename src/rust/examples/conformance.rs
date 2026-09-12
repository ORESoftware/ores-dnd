//! tjsv runtime-evidence adapter: `cargo run -p ores-dnd-core --example conformance -- <cases.json> <out.json>`
//! Reads the trusted case list produced by scripts/conformance/corpus.mjs and
//! writes this runtime's adapter block (verdicts only — never expectations).
use ores_dnd_core::corpus::decode_declaration;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    id: String,
    declaration: String,
    path: String,
}

#[derive(Deserialize)]
struct Cases {
    cases: Vec<Case>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResultRow {
    case_id: String,
    declaration: String,
    verdict: &'static str,
}

#[derive(Serialize)]
struct Adapter {
    id: &'static str,
    language: &'static str,
    runtime: String,
    validator: &'static str,
    toolchain: String,
    status: &'static str,
    results: Vec<ResultRow>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let cases_path = args.next().expect("cases.json path");
    let out_path = args.next().expect("output path");
    let root = std::path::Path::new(&cases_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();
    let cases: Cases =
        serde_json::from_str(&std::fs::read_to_string(&cases_path).expect("read cases"))
            .expect("parse cases");
    let mut results = Vec::new();
    for case in cases.cases {
        let path = if std::path::Path::new(&case.path).is_absolute() {
            case.path.clone().into()
        } else {
            root.join(&case.path)
        };
        let json = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let verdict = match decode_declaration(
            case.declaration
                .rsplit('.')
                .next()
                .unwrap_or(&case.declaration),
            &json,
        ) {
            Ok(()) => "accepted",
            Err(_) => "rejected",
        };
        results.push(ResultRow {
            case_id: case.id,
            declaration: case.declaration,
            verdict,
        });
    }
    let rustc = option_env!("ORES_DND_RUSTC_VERSION").unwrap_or("rustc");
    let adapter = Adapter {
        id: "rust-serde",
        language: "rust",
        runtime: rustc.to_owned(),
        validator: "serde@1",
        toolchain: format!(
            "cargo@{}",
            option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("stable")
        ),
        status: "passed",
        results,
    };
    std::fs::write(&out_path, serde_json::to_string_pretty(&adapter).unwrap())
        .expect("write evidence");
    eprintln!(
        "rust adapter: {} cases -> {out_path}",
        adapter.results.len()
    );
}
