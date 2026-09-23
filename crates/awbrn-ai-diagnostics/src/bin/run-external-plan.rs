use std::path::PathBuf;

use anyhow::{Context, Result};
use awbrn_ai_diagnostics::map_registry::MapRegistry;
use awbrn_ai_diagnostics::plan::read_plan;
use awbrn_ai_diagnostics::tournament::run_paired_tournament;

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let maps = PathBuf::from(args.next().context("missing external map manifest")?);
    let plan_path = PathBuf::from(args.next().context("missing experiment plan")?);
    let output = PathBuf::from(args.next().context("missing output path")?);
    let registry =
        MapRegistry::load_path(&maps).context("load the separate external map manifest")?;
    let plan = read_plan(&plan_path).context("read the experiment plan")?;
    let materialized = plan
        .materialize(&plan_path, &registry)
        .context("materialize the run with the external maps")?;
    let result = run_paired_tournament(
        &materialized.manifest,
        &registry,
        materialized.candidate.as_ref(),
        materialized.baseline.as_ref(),
        &output,
    )
    .context("run the paired tournament")?;
    println!(
        "completed {}: {} matches, {} valid pairs, mean {}",
        output.display(),
        result.matches,
        result.reduction.coverage.valid,
        result.reduction.observed_mean,
    );
    Ok(())
}
