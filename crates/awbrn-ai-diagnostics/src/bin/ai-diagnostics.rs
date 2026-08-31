use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use awbrn_ai_diagnostics::{
    read_manifest, reanalyse_event_log_with_manifest, run_diagnostic, run_review, verify_artifact,
};

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = arguments.first().map(String::as_str) else {
        usage();
        return ExitCode::from(2);
    };
    match command {
        "run" => run(&arguments[1..]),
        "analyze" => analyze(&arguments[1..]),
        "review" => review(&arguments[1..]),
        "verify" => verify(&arguments[1..]),
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn run(arguments: &[String]) -> ExitCode {
    let (manifest, output) = match required_paths(arguments, "run") {
        Ok(paths) => paths,
        Err(code) => return code,
    };
    match run_diagnostic(manifest, output) {
        Ok(summary) => {
            println!(
                "completed {} ({} matches, {} event rows, {} frames, review {})",
                summary.tournament.output.display(),
                summary.tournament.matches,
                summary.verification.event_rows,
                summary.review.frames,
                summary.review.output.display(),
            );
            ExitCode::SUCCESS
        }
        Err(error) => report_error("run", error),
    }
}

fn analyze(arguments: &[String]) -> ExitCode {
    const USAGE: &str = "usage: ai-diagnostics analyze --manifest run.json --events events.jsonl --output target/reanalysis";
    let options = match parse_options(arguments, &["--manifest", "--events", "--output"]) {
        Ok(options) => options,
        Err(message) => return invalid_arguments(&message, USAGE),
    };
    let (Some(manifest_path), Some(events), Some(output)) = (
        options.get("--manifest"),
        options.get("--events"),
        options.get("--output"),
    ) else {
        return invalid_arguments("analyze needs --manifest, --events, and --output", USAGE);
    };
    let output = PathBuf::from(output);
    let result = read_manifest(manifest_path)
        .map_err(|error| error.to_string())
        .and_then(|manifest| {
            reanalyse_event_log_with_manifest(events, &output, &manifest)
                .map_err(|error| error.to_string())
        });
    match result {
        Ok(summary) => {
            println!(
                "analysed {} event rows into {}",
                summary.event_rows,
                output.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => report_error("analyze", error),
    }
}

fn review(arguments: &[String]) -> ExitCode {
    let (manifest, output) = match required_paths(arguments, "review") {
        Ok(paths) => paths,
        Err(code) => return code,
    };
    match run_review(manifest, output) {
        Ok(summary) => {
            println!(
                "published {} ({} maps, {} pairs, {} frames)",
                summary.output.display(),
                summary.maps,
                summary.pairs,
                summary.frames
            );
            ExitCode::SUCCESS
        }
        Err(error) => report_error("review", error),
    }
}

fn verify(arguments: &[String]) -> ExitCode {
    const USAGE: &str =
        "usage: ai-diagnostics verify [--manifest run.json] --output target/experiment";
    let options = match parse_options(arguments, &["--manifest", "--output"]) {
        Ok(options) => options,
        Err(message) => return invalid_arguments(&message, USAGE),
    };
    let Some(output) = options.get("--output") else {
        return invalid_arguments("verify needs --output", USAGE);
    };
    let output = PathBuf::from(output);
    let manifest = options
        .get("--manifest")
        .map(PathBuf::from)
        .unwrap_or_else(|| output.join("manifest.json"));
    match verify_artifact(manifest, &output) {
        Ok(summary) => {
            println!(
                "verified {} ({} event rows, {} matches, {:?})",
                summary.output.display(),
                summary.event_rows,
                summary.matches,
                summary.reduction
            );
            ExitCode::SUCCESS
        }
        Err(error) => report_error("verify", error),
    }
}

fn required_paths(arguments: &[String], command: &str) -> Result<(PathBuf, PathBuf), ExitCode> {
    let usage =
        format!("usage: ai-diagnostics {command} --manifest run.json --output target/experiment");
    let options = parse_options(arguments, &["--manifest", "--output"])
        .map_err(|message| invalid_arguments(&message, &usage))?;
    let (Some(manifest), Some(output)) = (options.get("--manifest"), options.get("--output"))
    else {
        return Err(invalid_arguments(
            &format!("{command} needs --manifest and --output"),
            &usage,
        ));
    };
    Ok((PathBuf::from(manifest), PathBuf::from(output)))
}

/// Read `--name value` pairs and refuse anything else.
fn parse_options(
    arguments: &[String],
    allowed: &[&str],
) -> Result<BTreeMap<String, String>, String> {
    let mut options = BTreeMap::new();
    let mut index = 0;
    while index < arguments.len() {
        let name = arguments[index].as_str();
        if !allowed.contains(&name) {
            return Err(format!("unrecognized argument {name}"));
        }
        let Some(value) = arguments
            .get(index + 1)
            .filter(|value| !value.starts_with('-'))
        else {
            return Err(format!("{name} needs a value"));
        };
        if options.insert(name.to_owned(), value.clone()).is_some() {
            return Err(format!("{name} is given more than once"));
        }
        index += 2;
    }
    Ok(options)
}

fn invalid_arguments(message: &str, usage: &str) -> ExitCode {
    eprintln!("ai-diagnostics: {message}");
    eprintln!("{usage}");
    ExitCode::from(2)
}

fn report_error(command: &str, error: impl std::fmt::Display) -> ExitCode {
    eprintln!("ai-diagnostics {command} stopped: {error}");
    ExitCode::FAILURE
}

fn usage() {
    eprintln!(
        "usage:\n  ai-diagnostics run --manifest experiment.json --output target/experiment\n  ai-diagnostics analyze --manifest run.json --events events.jsonl --output target/reanalysis\n  ai-diagnostics review --manifest run.json --output target/review\n  ai-diagnostics verify [--manifest run.json] --output target/experiment"
    );
}
