use std::{env, fs, io, path::Path, process::ExitCode};

use chem_catalogue::ValidatedCatalogueBundle;
use chem_domain::ContentDigest;
use chem_kernel::expand_provisional;
use chems_lang::{format_source, parse_bytes};
use serde_json::json;

mod authoring;

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("chems: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[String]) -> Result<(), String> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(usage());
    };
    match command {
        "parse" => parse_command(&arguments[1..]),
        "format" | "fmt" => format_command(&arguments[1..]),
        "inspect" => inspect_command(&arguments[1..]),
        "catalogue" => authoring::catalogue_command(&arguments[1..]),
        "--help" | "-h" | "help" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn parse_command(arguments: &[String]) -> Result<(), String> {
    let path = exactly_one_path(arguments, "parse")?;
    let bytes = fs::read(path).map_err(|error| io_error(path, &error))?;
    let result = parse_bytes(&bytes);
    let output = json!({
        "schema_version": 1,
        "cst": result.cst,
        "ast": result.ast,
        "diagnostics": result.diagnostics,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    require_no_diagnostics(&output)
}

fn inspect_command(arguments: &[String]) -> Result<(), String> {
    match arguments.first().map(String::as_str) {
        Some("source") => inspect_source(&arguments[1..]),
        Some("expanded") => inspect_expanded(&arguments[1..]),
        _ => Err("inspect requires `source` or `expanded`".to_owned()),
    }
}

fn inspect_source(arguments: &[String]) -> Result<(), String> {
    let path = exactly_one_path(arguments, "inspect source")?;
    let bytes = fs::read(path).map_err(|error| io_error(path, &error))?;
    let result = parse_bytes(&bytes);
    let output = json!({
        "schema_version": 1,
        "inspection": "authored_source",
        "source": path,
        "source_bytes_digest": ContentDigest::sha256(&bytes),
        "ast": result.ast,
        "diagnostics": result.diagnostics,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    require_no_diagnostics(&output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpandedOutput {
    Certificate,
    Json,
    Provenance,
}

fn inspect_expanded(arguments: &[String]) -> Result<(), String> {
    let Some(source) = arguments.first().filter(|value| !value.starts_with('-')) else {
        return Err("inspect expanded requires a .chems path".to_owned());
    };
    let mut catalogue = None;
    let mut evidence = None;
    let mut output = ExpandedOutput::Certificate;
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--catalogue" => {
                index += 1;
                catalogue = arguments.get(index);
            }
            "--evidence" => {
                index += 1;
                evidence = arguments.get(index);
            }
            "--json" if output == ExpandedOutput::Certificate => output = ExpandedOutput::Json,
            "--provenance" if output == ExpandedOutput::Certificate => {
                output = ExpandedOutput::Provenance;
            }
            value => return Err(format!("unknown or conflicting option `{value}`")),
        }
        index += 1;
    }
    let catalogue = catalogue.ok_or("inspect expanded requires `--catalogue <path>`")?;
    let evidence = evidence.ok_or("inspect expanded requires `--evidence <path>`")?;
    let source_path = Path::new(source);
    let source_bytes = fs::read(source_path).map_err(|error| io_error(source_path, &error))?;
    let catalogue_path = Path::new(catalogue);
    let catalogue = ValidatedCatalogueBundle::from_json(
        &fs::read(catalogue_path).map_err(|error| io_error(catalogue_path, &error))?,
    )
    .map_err(|error| error.to_string())?;
    let evidence_path = Path::new(evidence);
    let evidence = fs::read(evidence_path).map_err(|error| io_error(evidence_path, &error))?;
    let expanded = expand_provisional(
        source,
        std::str::from_utf8(&source_bytes).map_err(|error| error.to_string())?,
        &catalogue,
        &evidence,
    )
    .map_err(|error| error.to_string())?;
    match output {
        ExpandedOutput::Certificate => print!("{}", expanded.render_certificate()),
        ExpandedOutput::Json => println!(
            "{}",
            String::from_utf8(
                expanded
                    .semantic_json()
                    .map_err(|error| error.to_string())?
            )
            .map_err(|error| error.to_string())?
        ),
        ExpandedOutput::Provenance => print!("{}", expanded.render_provenance_report()),
    }
    Ok(())
}

fn require_no_diagnostics(output: &serde_json::Value) -> Result<(), String> {
    if output["diagnostics"].as_array().is_some_and(Vec::is_empty) {
        Ok(())
    } else {
        Err("source contains diagnostics".to_owned())
    }
}

fn format_command(arguments: &[String]) -> Result<(), String> {
    let mut check = false;
    let mut write = false;
    let mut paths = Vec::new();
    for argument in arguments {
        match argument.as_str() {
            "--check" => check = true,
            "--write" | "-w" => write = true,
            value if value.starts_with('-') => return Err(format!("unknown option `{value}`")),
            _ => paths.push(argument),
        }
    }
    if check && write {
        return Err("`--check` and `--write` cannot be combined".to_owned());
    }
    if paths.is_empty() {
        return Err("format requires at least one .chems path".to_owned());
    }
    let multiple = paths.len() > 1;
    for path in paths {
        let path = Path::new(path);
        let source = fs::read_to_string(path).map_err(|error| io_error(path, &error))?;
        let formatted = format_source(&source).map_err(|error| {
            let diagnostics = serde_json::to_string_pretty(&error.diagnostics)
                .unwrap_or_else(|_| error.to_string());
            format!("{}:\n{diagnostics}", path.display())
        })?;
        if check {
            if formatted != source {
                return Err(format!("{} is not canonically formatted", path.display()));
            }
        } else if write {
            fs::write(path, formatted).map_err(|error| io_error(path, &error))?;
        } else if !multiple {
            print!("{formatted}");
        } else {
            return Err("multiple paths require `--check` or `--write`".to_owned());
        }
    }
    Ok(())
}

fn exactly_one_path<'a>(arguments: &'a [String], command: &str) -> Result<&'a Path, String> {
    if arguments.len() != 1 {
        return Err(format!("{command} requires exactly one .chems path"));
    }
    Ok(Path::new(&arguments[0]))
}

fn io_error(path: &Path, error: &io::Error) -> String {
    format!("{}: {error}", path.display())
}

fn usage() -> String {
    "usage:\n  chems parse <file.chems>\n  chems format [--check | --write] <file.chems>...\n  chems inspect source <file.chems>\n  chems inspect expanded <file.chems> --catalogue <catalogue.json> --evidence <evidence.json> [--json | --provenance]\n  chems catalogue check --out <directory> <candidate-package>...\n  chems catalogue promote --out <directory> --attestation <review.json> <candidate-package>..."
        .to_owned()
}
