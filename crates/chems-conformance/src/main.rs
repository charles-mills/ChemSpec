use std::{env, path::PathBuf, process::ExitCode};

use chems_conformance::validate_repository;

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let [command] = arguments.as_slice() else {
        usage();
        return ExitCode::from(1);
    };
    if !matches!(command.as_str(), "validate" | "report") {
        usage();
        return ExitCode::from(1);
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root should exist");
    let summary = match validate_repository(&root) {
        Ok(summary) => summary,
        Err(error) => {
            eprintln!("conformance validation failed: {error}");
            return ExitCode::from(2);
        }
    };

    println!(
        "specification: {} requirements; grammar: {} productions; reserved words: {}",
        summary.requirements, summary.grammar_productions, summary.reserved_words
    );
    println!(
        "manifest: {} components; {} cases; {} incomplete",
        summary.components, summary.cases, summary.incomplete_cases
    );
    for item in &summary.coverage {
        println!(
            "  {:<20} cases {:>3}; requirements {:>3}/{:<3}",
            item.component, item.cases, item.covered_requirements, item.total_requirements
        );
    }

    if command == "report" && !summary.is_complete() {
        eprintln!(
            "conformance is incomplete: {} incomplete cases and/or missing requirement coverage",
            summary.incomplete_cases
        );
        ExitCode::from(3)
    } else {
        ExitCode::SUCCESS
    }
}

fn usage() {
    eprintln!("usage: chems-conformance <validate|report>");
}
