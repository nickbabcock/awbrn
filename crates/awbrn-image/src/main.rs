//! CLI: render an AWBW text or JSON map to a PNG or a text grid.
//!
//! Usage:
//!   awbrn-image <input.(txt|json)> [-o <output>] [--format png|text|awbw] [--assets-dir <dir>]

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use awbrn_image::{Tilesets, render_map};
use awbrn_map::{AwbwMap, AwbwMapData, PredeployedUnit};

/// Output representation for a parsed map.
#[derive(Debug, Clone, Copy)]
enum Format {
    /// Rendered PNG image.
    Png,
    /// Lossless Unicode glyph grid (every tile a unique character) plus a legend.
    Text,
    /// AWBW text-format ASCII grid, with factions collapsed to the canonical set.
    Awbw,
}

impl std::str::FromStr for Format {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "png" => Ok(Format::Png),
            "text" => Ok(Format::Text),
            "awbw" => Ok(Format::Awbw),
            other => bail!("unknown format: {other} (expected png, text, or awbw)"),
        }
    }
}

fn main() -> Result<()> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut assets_dir: Option<PathBuf> = None;
    let mut format: Option<Format> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                output = Some(next_value(&mut args, &arg)?.into());
            }
            "-f" | "--format" => {
                format = Some(next_value(&mut args, &arg)?.parse()?);
            }
            "--assets-dir" => {
                assets_dir = Some(next_value(&mut args, &arg)?.into());
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

    // Default to PNG, but infer text output from a `.txt` output path.
    let format = format.unwrap_or_else(|| infer_format(output.as_deref()));

    match format {
        Format::Png => {
            let output = output.unwrap_or_else(|| input.with_extension("png"));
            let assets_dir = assets_dir.unwrap_or_else(default_assets_dir);
            let tilesets = Tilesets::load_from_dir(&assets_dir).with_context(|| {
                format!(
                    "loading tiles.png / units.png / ui.png from {} (override with --assets-dir)",
                    assets_dir.display()
                )
            })?;

            let image = render_map(&map, &units, &tilesets)?;
            image
                .save(&output)
                .with_context(|| format!("writing {}", output.display()))?;
            eprintln!(
                "wrote {} ({}x{})",
                output.display(),
                image.width(),
                image.height()
            );
        }
        Format::Text => {
            let map = map.collapse_factions();
            let mut content = map.lossless().to_string();
            let legend = map.legend().to_string();
            if !legend.is_empty() {
                content.push_str("\n\nLegend:\n");
                content.push_str(&legend);
            }
            write_text(content, output)?;
        }
        Format::Awbw => {
            write_text(map.collapse_factions().awbw().to_string(), output)?;
        }
    }

    Ok(())
}

/// Write a text rendering (with a trailing newline) to `output` if given, else
/// to stdout.
fn write_text(mut content: String, output: Option<PathBuf>) -> Result<()> {
    // Append the trailing newline in place rather than reallocating the whole
    // (potentially map-sized) buffer with `format!`.
    content.push('\n');
    match output {
        Some(path) => {
            std::fs::write(&path, &content)
                .with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
        None => print!("{content}"),
    }
    Ok(())
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("missing value for {flag}"))
}

fn is_json(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

/// Infer the output format from the output path's extension (`.txt` → text).
fn infer_format(output: Option<&Path>) -> Format {
    match output.and_then(|path| path.extension()) {
        Some(ext) if ext.eq_ignore_ascii_case("txt") => Format::Text,
        _ => Format::Png,
    }
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
        "awbrn-image — render an AWBW map to PNG or text\n\n\
         USAGE:\n    awbrn-image <input.(txt|json)> [-o <output>] [--format png|text|awbw] [--assets-dir <dir>]\n\n\
         ARGS:\n    <input>            AWBW text (.txt) or map_info JSON (.json) map\n\n\
         OPTIONS:\n    -o, --output <file>    Output path (default: input with .png; stdout for text)\n    \
         -f, --format <fmt>     png (default), text (lossless Unicode + legend), or awbw\n                           \
         (collapsed ASCII). Inferred as text when --output ends in .txt\n    \
         --assets-dir <dir>     Directory containing tiles.png, units.png, and ui.png\n                           \
         (ui_atlas.json must be here or in sibling data/; default: bundled assets/textures)\n    \
    -h, --help             Print this help"
    );
}
