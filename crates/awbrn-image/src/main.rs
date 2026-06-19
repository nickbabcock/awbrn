//! CLI: render an AWBW text or JSON map to a PNG.
//!
//! Usage:
//!   awbrn-image <input.(txt|json)> [-o <output.png>] [--assets-dir <dir>]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use awbrn_image::{Tilesets, render_map};
use awbrn_map::{AwbwMap, AwbwMapData, PredeployedUnit};

fn main() -> Result<()> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut assets_dir: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                output = Some(next_value(&mut args, &arg)?);
            }
            "--assets-dir" => {
                assets_dir = Some(next_value(&mut args, &arg)?);
            }
            "-h" | "--help" => {
                print_usage();
                return Ok(());
            }
            flag if flag.starts_with('-') => bail!("unknown flag: {flag}"),
            positional => {
                if input.is_some() {
                    bail!("unexpected extra argument: {positional}");
                }
                input = Some(PathBuf::from(positional));
            }
        }
    }

    let input = input.context("missing <input> map file (.txt or .json)\n\n(run with --help)")?;
    let output = output.unwrap_or_else(|| input.with_extension("png"));
    let assets_dir = assets_dir.unwrap_or_else(default_assets_dir);

    let data = std::fs::read(&input).with_context(|| format!("reading {}", input.display()))?;

    let (map, units) = if is_json(&input) {
        let map_data: AwbwMapData = serde_json::from_slice(&data)
            .with_context(|| format!("parsing JSON map {}", input.display()))?;
        let map = AwbwMap::try_from(&map_data)?;
        (map, map_data.predeployed_units)
    } else {
        let text =
            String::from_utf8(data).with_context(|| format!("{} is not UTF-8", input.display()))?;
        let map = AwbwMap::parse_txt(&text)?;
        (map, Vec::<PredeployedUnit>::new())
    };

    let tilesets = Tilesets::load_from_dir(&assets_dir).with_context(|| {
        format!(
            "loading tiles.png / units.png from {} (override with --assets-dir)",
            assets_dir.display()
        )
    })?;

    let image = render_map(&map, &units, &tilesets);
    image
        .save(&output)
        .with_context(|| format!("writing {}", output.display()))?;

    eprintln!(
        "wrote {} ({}x{})",
        output.display(),
        image.width(),
        image.height()
    );
    Ok(())
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<PathBuf> {
    args.next()
        .map(PathBuf::from)
        .with_context(|| format!("missing value for {flag}"))
}

fn is_json(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

/// Default to the workspace's bundled textures, resolved relative to this crate.
fn default_assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| Path::new("."))
        .join("assets/textures")
}

fn print_usage() {
    eprintln!(
        "awbrn-image — render an AWBW map to PNG\n\n\
         USAGE:\n    awbrn-image <input.(txt|json)> [-o <output.png>] [--assets-dir <dir>]\n\n\
         ARGS:\n    <input>            AWBW text (.txt) or map_info JSON (.json) map\n\n\
         OPTIONS:\n    -o, --output <file>    Output PNG path (default: input with .png)\n    \
         --assets-dir <dir>     Directory containing tiles.png and units.png\n                           \
         (default: bundled assets/textures)\n    -h, --help             Print this help"
    );
}
