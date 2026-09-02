use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use awbrn_ai_diagnostic_types::RunManifest;
use awbrn_ai_diagnostics::{
    AnalysisStage, analyze_event_log, read_manifest, reanalyse_event_log_with_manifest,
    resolve_event_log_path, run_plan, run_review, run_search_sweep, verify_artifact,
};

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = arguments.first().map(String::as_str) else {
        usage();
        return ExitCode::from(2);
    };
    match command {
        "run" => run(&arguments[1..]),
        "search-sweep" => search_sweep(&arguments[1..]),
        "analyze" => analyze(&arguments[1..]),
        "features" | "feature-analysis" => features(&arguments[1..]),
        "review" => review(&arguments[1..]),
        "verify" => verify(&arguments[1..]),
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn search_sweep(arguments: &[String]) -> ExitCode {
    const USAGE: &str = "usage: ai-diagnostics search-sweep --plan search-budget-sweep-plan.json --output target/search-sweep";
    let options = match parse_options(arguments, &["--plan", "--output"]) {
        Ok(options) => options,
        Err(message) => return invalid_arguments(&message, USAGE),
    };
    let (Some(plan), Some(output)) = (options.get("--plan"), options.get("--output")) else {
        return invalid_arguments("search-sweep needs --plan and --output", USAGE);
    };
    match run_search_sweep(plan, output) {
        Ok(summary) => {
            println!(
                "completed Search sweep {} ({})",
                summary.output.display(),
                summary.decision.search_coverage_decision
            );
            ExitCode::SUCCESS
        }
        Err(error) => report_error("search-sweep", error),
    }
}

fn run(arguments: &[String]) -> ExitCode {
    const USAGE: &str = "usage: ai-diagnostics run --plan experiment.json --output target/run";
    let options = match parse_options(arguments, &["--plan", "--output"]) {
        Ok(options) => options,
        Err(message) => return invalid_arguments(&message, USAGE),
    };
    let (Some(plan), Some(output)) = (options.get("--plan"), options.get("--output")) else {
        return invalid_arguments("run needs --plan and --output", USAGE);
    };
    match run_plan(plan, output) {
        Ok(summary) => {
            println!(
                "completed {} ({} matches, {} valid pairs)",
                summary.tournament.output.display(),
                summary.tournament.matches,
                summary.tournament.reduction.coverage.valid,
            );
            if let Some(features) = summary.feature_analysis {
                println!("  outcome features: {}", features.output.display());
            }
            if let Some(review) = summary.review {
                println!(
                    "  review: {} ({} frames)",
                    review.output.display(),
                    review.frames
                );
            }
            if let Some(verification) = summary.verification {
                println!("  verification: {:?}", verification.reduction);
            }
            ExitCode::SUCCESS
        }
        Err(error) => report_error("run", error),
    }
}

fn analyze(arguments: &[String]) -> ExitCode {
    const USAGE: &str =
        "usage: ai-diagnostics analyze --run target/run [--analysis outcome-features]";
    let options = match parse_options(arguments, &["--run", "--analysis"]) {
        Ok(options) => options,
        Err(message) => return invalid_arguments(&message, USAGE),
    };
    let Some(run) = options.get("--run") else {
        return invalid_arguments("analyze needs --run", USAGE);
    };
    let analyses = match options.get("--analysis").map_or_else(
        || Ok(vec![AnalysisStage::OutcomeFeatures]),
        |value| parse_analyses(value),
    ) {
        Ok(analyses) => analyses,
        Err(message) => return invalid_arguments(&message, USAGE),
    };
    let run = PathBuf::from(run);
    let result = read_manifest(run.join("manifest.json"))
        .map_err(|error| error.to_string())
        .and_then(|manifest| {
            resolve_event_log_path(&run, &manifest)
                .map_err(|error| error.to_string())
                .and_then(|events| analyze_stages(&run, &events, &manifest, &analyses))
        });
    match result {
        Ok(outputs) => {
            for output in outputs {
                println!("analysed {output}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => report_error("analyze", error),
    }
}

fn parse_analyses(value: &str) -> Result<Vec<AnalysisStage>, String> {
    let mut stages = Vec::new();
    for name in value
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let stage = match name {
            "outcome-features" => AnalysisStage::OutcomeFeatures,
            "review" => AnalysisStage::Review,
            "verification" => AnalysisStage::Verification,
            other => return Err(format!("unknown analysis stage {other:?}")),
        };
        if stages.contains(&stage) {
            return Err(format!("analysis stage {name:?} is given more than once"));
        }
        stages.push(stage);
    }
    if stages.is_empty() {
        return Err("--analysis needs at least one stage".into());
    }
    Ok(stages)
}

fn analyze_stages(
    run: &std::path::Path,
    events: &std::path::Path,
    manifest: &RunManifest,
    analyses: &[AnalysisStage],
) -> Result<Vec<String>, String> {
    let mut outputs = Vec::new();
    if analyses
        .iter()
        .any(|stage| !matches!(stage, AnalysisStage::Verification))
    {
        let rebuilt = reanalyse_event_log_with_manifest(events, run, manifest)
            .map_err(|error| error.to_string())?;
        outputs.push(format!(
            "{} ({} event rows, {} matches)",
            run.display(),
            rebuilt.event_rows,
            rebuilt.matches
        ));
    }
    for stage in analyses {
        match stage {
            AnalysisStage::OutcomeFeatures => {
                let summary = analyze_event_log(events, run.join("feature-analysis"))
                    .map_err(|error| error.to_string())?;
                if !summary.report.sufficient_corpus {
                    eprintln!(
                        "warning: {} matches with rows; collect at least {} for the planned corpus",
                        summary.report.matches_with_rows, summary.report.minimum_matches
                    );
                }
                outputs.push(format!(
                    "{} ({} matches, {} turn rows)",
                    summary.output.display(),
                    summary.extraction.matches_with_rows,
                    summary.extraction.rows.len()
                ));
            }
            AnalysisStage::Review => {
                let summary = run_review(run.join("manifest.json"), run.join("review"))
                    .map_err(|error| error.to_string())?;
                outputs.push(format!(
                    "{} ({} frames)",
                    summary.output.display(),
                    summary.frames
                ));
            }
            AnalysisStage::Verification => {
                let summary = verify_artifact(run.join("manifest.json"), run)
                    .map_err(|error| error.to_string())?;
                outputs.push(format!(
                    "{} ({:?})",
                    summary.output.display(),
                    summary.reduction
                ));
            }
        }
    }
    Ok(outputs)
}

fn features(arguments: &[String]) -> ExitCode {
    const USAGE: &str =
        "usage: ai-diagnostics features --states states.jsonl --output target/features";
    let options = match parse_options(arguments, &["--states", "--events", "--output"]) {
        Ok(options) => options,
        Err(message) => return invalid_arguments(&message, USAGE),
    };
    if options.contains_key("--states") && options.contains_key("--events") {
        return invalid_arguments("features accepts only one input path", USAGE);
    }
    let input = options.get("--states").or_else(|| options.get("--events"));
    let Some((input, output)) = input.zip(options.get("--output")) else {
        return invalid_arguments("features needs --states (or --events) and --output", USAGE);
    };
    match analyze_event_log(input, output) {
        Ok(summary) => {
            println!(
                "analysed {} event rows into {} ({} matches, {} turn rows)",
                summary.extraction.event_rows,
                summary.output.display(),
                summary.extraction.matches_with_rows,
                summary.extraction.rows.len(),
            );
            for mode in &summary.report.modes {
                println!(
                    "  {:?}: reduced CV log loss {:.4} [{:.4}, {:.4}], baseline {:.4}, selected {}",
                    mode.mode,
                    mode.model.cross_validation.log_loss.mean,
                    mode.model.cross_validation.log_loss.ci95_low,
                    mode.model.cross_validation.log_loss.ci95_high,
                    mode.model.cross_validation.baseline_log_loss.mean,
                    mode.model
                        .weights
                        .iter()
                        .filter(|weight| weight.selected)
                        .count(),
                );
            }
            if !summary.report.sufficient_corpus {
                eprintln!(
                    "warning: {} matches with rows; collect at least {} for the planned corpus",
                    summary.report.matches_with_rows, summary.report.minimum_matches
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => report_error("features", error),
    }
}

fn review(arguments: &[String]) -> ExitCode {
    const USAGE: &str = "usage: ai-diagnostics review --output target/run";
    let options = match parse_options(arguments, &["--output"]) {
        Ok(options) => options,
        Err(message) => return invalid_arguments(&message, USAGE),
    };
    let Some(output) = options.get("--output") else {
        return invalid_arguments("review needs --output", USAGE);
    };
    let output = PathBuf::from(output);
    match run_review(output.join("manifest.json"), output.join("review")) {
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
    const USAGE: &str = "usage: ai-diagnostics verify --output target/run";
    let options = match parse_options(arguments, &["--output"]) {
        Ok(options) => options,
        Err(message) => return invalid_arguments(&message, USAGE),
    };
    let Some(output) = options.get("--output") else {
        return invalid_arguments("verify needs --output", USAGE);
    };
    let output = PathBuf::from(output);
    match verify_artifact(output.join("manifest.json"), &output) {
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
        "usage:\n  ai-diagnostics search-sweep --plan search-budget-sweep-plan.json --output target/search-sweep\n  ai-diagnostics run --plan experiment.json --output target/run\n  ai-diagnostics analyze --run target/run [--analysis outcome-features,review,verification]\n  ai-diagnostics features --states states.jsonl --output target/features\n  ai-diagnostics review --output target/run\n  ai-diagnostics verify --output target/run"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_names_are_registered_and_repeatable() {
        let stages = parse_analyses("outcome-features,verification").expect("stages parse");
        assert_eq!(
            stages,
            vec![AnalysisStage::OutcomeFeatures, AnalysisStage::Verification]
        );
        parse_analyses("unknown").unwrap_err();
        parse_analyses("review,review").unwrap_err();
    }

    #[test]
    fn option_parser_rejects_unknown_and_duplicate_arguments() {
        let arguments = vec![
            "--run".into(),
            "target/run".into(),
            "--run".into(),
            "other".into(),
        ];
        parse_options(&arguments, &["--run"]).unwrap_err();
        parse_options(&["--bad".into(), "value".into()], &["--run"]).unwrap_err();
    }
}
