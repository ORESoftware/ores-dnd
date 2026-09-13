//! Every instance under contracts/instances must be accepted (valid/) or
//! rejected (invalid/) by the Rust decoder, exactly as both schema authorities
//! decide it. This is the same verdict list the tjsv runtime evidence carries.
mod common;

use ores_dnd_core::corpus::{decode_declaration, DECLARATIONS};

#[test]
fn every_corpus_instance_gets_the_declared_verdict() {
    let corpus = common::corpus();
    assert!(
        corpus.len() >= 70,
        "corpus unexpectedly small: {}",
        corpus.len()
    );
    let mut failures = Vec::new();
    for (declaration, expectation, file, json) in &corpus {
        assert!(
            DECLARATIONS.contains(&declaration.as_str()),
            "unknown declaration dir {declaration}"
        );
        let verdict = match decode_declaration(declaration, json) {
            Ok(()) => "accepted",
            Err(_) => "rejected",
        };
        if verdict != *expectation {
            failures.push(format!(
                "{declaration}/{expectation}/{file}: rust said {verdict}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "corpus divergences:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_declaration_has_corpus_coverage() {
    let corpus = common::corpus();
    for declaration in DECLARATIONS {
        assert!(
            corpus
                .iter()
                .any(|(d, e, _, _)| d == declaration && *e == "accepted"),
            "{declaration} has no valid instance"
        );
        assert!(
            corpus
                .iter()
                .any(|(d, e, _, _)| d == declaration && *e == "rejected"),
            "{declaration} has no invalid instance"
        );
    }
}
