//! Generic offline review staging and atomic publication.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use crate::capture::load_tilesets;
use crate::events::{
    EventLogError, EventRow, latest_attempt_rows, read_event_log, verify_expected_fingerprints,
};
use crate::manifest::{ManifestError, read_manifest, resolve_event_log_path};
use crate::map_registry::{CANONICAL_SEATS, MapRegistry, MapRegistryError};
use crate::{VisualCapture, VisualCaptureIdentity};
use awbrn_ai_diagnostic_types::{PairKey, RunManifest, SeatOrderVariant};
use awbrn_image::Tilesets;
use awbrn_map::AwbrnMap;
use serde::Serialize;

/// Errors from the generic review runner.
#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("review manifest error: {0}")]
    Manifest(#[from] ManifestError),
    #[error("review I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("review JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("review event log error: {0}")]
    Event(#[from] EventLogError),
    #[error("review map registry error: {0}")]
    Map(#[from] MapRegistryError),
    #[error("review capture error: {0}")]
    Capture(#[from] anyhow::Error),
    #[error("review configuration error: {0}")]
    Configuration(String),
}

/// The result of publishing a static review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewSummary {
    pub output: PathBuf,
    pub maps: usize,
    pub pairs: usize,
    pub frames: usize,
}

#[derive(Serialize)]
struct ReviewConfiguration<'a> {
    schema_version: u16,
    manifest_fingerprint: String,
    manifest: &'a RunManifest,
}

#[derive(Serialize)]
struct Annotation {
    map_id: u32,
    pair_index: u64,
    classification: String,
    note: String,
}

#[derive(Serialize)]
struct ReviewAnnotations {
    schema_version: u16,
    manifest_fingerprint: String,
    annotations: Vec<Annotation>,
}

#[derive(Clone, Debug)]
struct CapturedFrame {
    frame: u32,
    day: u64,
    image: String,
    terminal: bool,
    turn_end: bool,
}

#[derive(Clone, Debug)]
struct CapturedMatch {
    directory: String,
    match_id: String,
    pair: PairKey,
    seat_order: SeatOrderVariant,
    frames: Vec<CapturedFrame>,
}

/// Verify a manifest and atomically publish an offline review directory.
pub fn run_review(
    manifest_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<ReviewSummary, ReviewError> {
    review(manifest_path, output, None)
}

/// Publish a review whose frames are rendered with the given sprite atlases.
///
/// A caller that does not check how the frames look gives synthetic atlases
/// here, so it does not need the generated `assets/textures` files.
pub fn run_review_with_tilesets(
    manifest_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    tilesets: Tilesets,
) -> Result<ReviewSummary, ReviewError> {
    review(manifest_path, output, Some(tilesets))
}

fn review(
    manifest_path: impl AsRef<Path>,
    output: impl AsRef<Path>,
    tilesets: Option<Tilesets>,
) -> Result<ReviewSummary, ReviewError> {
    let manifest_path = manifest_path.as_ref().to_owned();
    let manifest = read_manifest(&manifest_path)?;
    let manifest_fingerprint = manifest.fingerprint().map_err(ReviewError::Configuration)?;
    let output = output.as_ref().to_owned();
    refuse_unmarked_output(&output)?;
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("review");
    let staging = output.with_file_name(format!("{name}-staging"));
    if staging.exists() {
        return Err(ReviewError::Configuration(format!(
            "staging directory already exists: {}",
            staging.display()
        )));
    }
    fs::create_dir_all(&staging)?;
    let result = write_review(
        &staging,
        &manifest,
        &manifest_fingerprint,
        &manifest_path,
        tilesets,
    );
    match result {
        Ok(summary) => {
            let publish = publish_atomically(&output, &staging, summary);
            if publish.is_err() {
                let _ = fs::remove_dir_all(&staging);
            }
            publish
        }
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

fn publish_atomically(
    output: &Path,
    staging: &Path,
    summary: ReviewSummary,
) -> Result<ReviewSummary, ReviewError> {
    refuse_unmarked_output(output)?;
    let previous = output.with_file_name(format!(
        "{}-previous",
        output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("review")
    ));
    if previous.exists() {
        return Err(ReviewError::Configuration(format!(
            "previous review directory already exists: {}",
            previous.display()
        )));
    }
    let had_previous = output.exists();
    if had_previous {
        fs::rename(output, &previous)?;
    }
    match fs::rename(staging, output) {
        Ok(()) => {
            if had_previous {
                fs::remove_dir_all(previous)?;
            }
            Ok(ReviewSummary {
                output: output.to_owned(),
                ..summary
            })
        }
        Err(error) => {
            if had_previous {
                let _ = fs::rename(&previous, output);
            }
            Err(error.into())
        }
    }
}

fn write_review(
    staging: &Path,
    manifest: &RunManifest,
    fingerprint: &str,
    manifest_path: &Path,
    tilesets: Option<Tilesets>,
) -> Result<ReviewSummary, ReviewError> {
    let captures = render_captures(staging, manifest, manifest_path, tilesets)?;
    fs::write(
        staging.join("configuration.json"),
        serde_json::to_vec_pretty(&ReviewConfiguration {
            schema_version: 1,
            manifest_fingerprint: fingerprint.to_owned(),
            manifest,
        })?,
    )?;
    let annotations = manifest
        .pairs
        .iter()
        .map(|pair| Annotation {
            map_id: pair.map_id,
            pair_index: pair.pair_index,
            classification: "unreviewed".into(),
            note: String::new(),
        })
        .collect::<Vec<_>>();
    fs::write(
        staging.join("annotations.json"),
        serde_json::to_vec_pretty(&ReviewAnnotations {
            schema_version: 1,
            manifest_fingerprint: fingerprint.to_owned(),
            annotations,
        })?,
    )?;
    fs::write(staging.join("findings.md"), findings_template())?;
    fs::write(
        staging.join("index.html"),
        render_index(manifest, fingerprint, &captures),
    )?;
    Ok(ReviewSummary {
        output: staging.to_owned(),
        maps: manifest.maps.len(),
        pairs: manifest.pairs.len(),
        frames: captures.iter().map(|capture| capture.frames.len()).sum(),
    })
}

fn refuse_unmarked_output(output: &Path) -> Result<(), ReviewError> {
    let Some(metadata) = fs::symlink_metadata(output).ok() else {
        return Ok(());
    };
    if !metadata.file_type().is_dir() || !is_marked_output(output) {
        return Err(ReviewError::Configuration(format!(
            "refusing to replace existing unmarked directory {}",
            output.display()
        )));
    }
    Ok(())
}

fn is_marked_output(output: &Path) -> bool {
    let Ok(data) = fs::read(output.join("configuration.json")) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) else {
        return false;
    };
    value["schema_version"] == 1 && value["manifest_fingerprint"].is_string()
}

fn render_captures(
    staging: &Path,
    manifest: &RunManifest,
    manifest_path: &Path,
    mut tilesets: Option<Tilesets>,
) -> Result<Vec<CapturedMatch>, ReviewError> {
    let event_path = event_log_path(manifest_path, manifest)?;
    let registry = MapRegistry::load_checked_in()?;
    validate_manifest_maps(manifest, &registry)?;
    if !event_path.exists() {
        if manifest.event_log.is_some()
            || manifest.expected.event_log.is_some()
            || !manifest.expected.command.is_empty()
            || !manifest.expected.derived_tables.is_empty()
        {
            return Err(ReviewError::Configuration(format!(
                "review event log does not exist: {}",
                event_path.display()
            )));
        }
        return Ok(Vec::new());
    }
    let event_rows = read_event_log(&event_path)?;
    verify_expected_fingerprints(
        manifest,
        &event_path,
        event_path.parent().unwrap_or(Path::new(".")),
    )?;
    if matches!(
        manifest.capture_policy.frame_policy,
        awbrn_ai_diagnostic_types::FramePolicy::Disabled
    ) {
        return Ok(Vec::new());
    }
    let rows = latest_attempt_rows(&event_rows);
    let mut grouped = BTreeMap::<String, Vec<&EventRow>>::new();
    for row in &rows {
        grouped.entry(row.match_id.clone()).or_default().push(row);
    }
    let mut captures = Vec::new();
    for (index, (match_id, rows)) in grouped.into_iter().enumerate() {
        let first = rows.first().ok_or_else(|| {
            ReviewError::Configuration(format!("event match {match_id} has no rows"))
        })?;
        let pair = first.pair.clone();
        let seat_order = first.seat_order;
        let match_seed = first.match_seed;
        if !manifest
            .capture_policy
            .selects(pair.map_id, pair.run_seed, pair.pair_index, seat_order)
        {
            continue;
        }
        let map = registry.get(pair.map_id).ok_or_else(|| {
            ReviewError::Configuration(format!(
                "event match {match_id} names unknown map {}",
                pair.map_id
            ))
        })?;
        let directory = format!("{:04}-{}", index, safe_component(&match_id));
        let directory_path = staging.join("captures").join(&directory);
        // The production atlases are decoded once, for the first match that
        // renders, and then lent to every match after it.
        let tilesets = match &mut tilesets {
            Some(tilesets) => tilesets,
            empty => empty.insert(load_tilesets()?),
        };
        let mut capture = VisualCapture::new(
            AwbrnMap::from_map(&map.normalized),
            CANONICAL_SEATS,
            &directory_path,
            VisualCaptureIdentity {
                map_id: pair.map_id,
                run_seed: pair.run_seed,
                pair_index: pair.pair_index,
                attempt: first.attempt,
                seat_order,
                match_seed,
            },
            manifest.capture_policy.clone(),
            tilesets,
        )?;
        for row in rows {
            capture.observe(&row.state, row.command.as_ref())?;
        }
        capture.finish()?;
        let frames = read_captured_frames(&directory_path)?;
        captures.push(CapturedMatch {
            directory,
            match_id,
            pair,
            seat_order,
            frames,
        });
    }
    Ok(captures)
}

fn event_log_path(manifest_path: &Path, manifest: &RunManifest) -> Result<PathBuf, ReviewError> {
    resolve_event_log_path(manifest_path.parent().unwrap_or(Path::new(".")), manifest)
        .map_err(ReviewError::Manifest)
}

fn validate_manifest_maps(
    manifest: &RunManifest,
    registry: &MapRegistry,
) -> Result<(), ReviewError> {
    for expected in &manifest.maps {
        let Some(map) = registry.get(expected.map_id) else {
            return Err(ReviewError::Configuration(format!(
                "manifest map {} is not in the fixed registry",
                expected.map_id
            )));
        };
        if expected.source_fingerprint != map.source_fingerprint
            || expected.normalized_fingerprint != map.normalized_fingerprint
        {
            return Err(ReviewError::Configuration(format!(
                "manifest fingerprints differ for map {}",
                expected.map_id
            )));
        }
    }
    Ok(())
}

fn read_captured_frames(path: &Path) -> Result<Vec<CapturedFrame>, ReviewError> {
    let data = fs::read_to_string(path.join("frames.jsonl"))?;
    data.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value: serde_json::Value = serde_json::from_str(line)?;
            Ok(CapturedFrame {
                frame: value["frame"].as_u64().ok_or_else(|| {
                    ReviewError::Configuration("capture frame has no frame number".into())
                })? as u32,
                day: value["day"]
                    .as_u64()
                    .ok_or_else(|| ReviewError::Configuration("capture frame has no day".into()))?,
                image: value["image"]
                    .as_str()
                    .ok_or_else(|| ReviewError::Configuration("capture frame has no image".into()))?
                    .to_owned(),
                terminal: value["terminal"].as_bool().ok_or_else(|| {
                    ReviewError::Configuration("capture frame has no terminal flag".into())
                })?,
                turn_end: value["turn_end"].as_bool().ok_or_else(|| {
                    ReviewError::Configuration("capture frame has no turn-end flag".into())
                })?,
            })
        })
        .collect()
}

fn safe_component(value: &str) -> String {
    let component = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if component.is_empty() {
        "match".into()
    } else {
        component
    }
}

fn render_index(manifest: &RunManifest, fingerprint: &str, captures: &[CapturedMatch]) -> String {
    let mut html = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>AI review</title><style>body{font:16px system-ui,sans-serif;margin:2rem}table{border-collapse:collapse}td,th{border:1px solid #999;padding:.4rem}</style></head><body>",
    );
    let _ = writeln!(html, "<h1>AI review: {}</h1>", escape(&manifest.run_id));
    let _ = writeln!(
        html,
        "<p>Manifest fingerprint: <code>{fingerprint}</code></p>"
    );
    html.push_str(
        "<h2>Maps</h2><table><tr><th>ID</th><th>Name</th><th>Normalized fingerprint</th></tr>",
    );
    for map in &manifest.maps {
        let _ = writeln!(
            html,
            "<tr><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
            map.map_id,
            escape(&map.name),
            escape(&map.normalized_fingerprint)
        );
    }
    html.push_str(
        "</table><h2>Selected pairs</h2><table><tr><th>Map</th><th>Seed</th><th>Pair</th></tr>",
    );
    for pair in &manifest.pairs {
        let _ = writeln!(
            html,
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            pair.map_id, pair.run_seed, pair.pair_index
        );
    }
    html.push_str("</table><h2>Captured frames</h2>");
    if captures.is_empty() {
        html.push_str("<p>No event-log frames were selected.</p>");
    } else {
        html.push_str(
            "<table><tr><th>Match</th><th>Map</th><th>Pair</th><th>Seat order</th><th>Frames</th><th>Sidecar</th></tr>",
        );
        for capture in captures {
            let _ = write!(
                html,
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>",
                escape(&capture.match_id),
                capture.pair.map_id,
                capture.pair.pair_index,
                capture.seat_order.as_str(),
            );
            for frame in &capture.frames {
                let _ = write!(
                    html,
                    "<a href=\"captures/{}/{}\">frame {} (day {}{})</a> ",
                    escape(&capture.directory),
                    escape(&frame.image),
                    frame.frame,
                    frame.day,
                    if frame.terminal {
                        ", terminal"
                    } else if frame.turn_end {
                        ", turn end"
                    } else {
                        ""
                    },
                );
            }
            let _ = writeln!(
                html,
                "</td><td><a href=\"captures/{}/frames.jsonl\">JSONL</a></td></tr>",
                escape(&capture.directory),
            );
        }
        html.push_str("</table>");
    }
    html.push_str(
        "<p>Machine-readable annotations are in <code>annotations.json</code>.</p></body></html>\n",
    );
    html
}

fn findings_template() -> &'static str {
    "# AI review findings\n\nRecord the visible board fact, relevant commands, repeated evidence, and classification for each finding.\n"
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temporary_directory() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "awbrn-ai-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn publishes_twice_only_when_the_previous_output_is_marked() {
        let output = temporary_directory();
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/smoke-manifest.json");
        let summary = run_review(&manifest, &output).expect("the review publishes");
        assert_eq!(summary.output, output);
        assert!(output.join("index.html").is_file());
        run_review(&manifest, &output).expect("the marked review can be replaced");
        fs::write(output.join("unmarked.txt"), "do not replace this directory").unwrap();
        fs::remove_file(output.join("configuration.json")).unwrap();
        assert!(matches!(
            run_review(&manifest, &output),
            Err(ReviewError::Configuration(_))
        ));
        fs::remove_dir_all(output).unwrap();
    }
}
