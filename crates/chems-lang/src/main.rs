use std::{env, fs, io, path::Path, process::ExitCode};

use chems_lang::{format_source, parse_bytes};
use serde_json::json;

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
        "schemaVersion": 1,
        "cst": result.cst,
        "ast": result.ast,
        "diagnostics": result.diagnostics,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
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
    "usage:\n  chems parse <file.chems>\n  chems format [--check | --write] <file.chems>..."
        .to_owned()
}
