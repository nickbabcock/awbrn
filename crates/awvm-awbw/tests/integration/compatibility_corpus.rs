use std::collections::BTreeMap;
use std::fmt::Write as _;

use awbrn_map::AwbwMapData;
use awbw_replay::{ReplayParser, turn_models::Action};
use awvm_awbw::{LocalCompatibility, RecordedAdapter, diagnose_local_compatibility_until_match};
use highway::{HighwayHash, HighwayHasher, Key};

use crate::common::map_path;

#[test]
#[ignore = "advisory 12,929-action differential; run explicitly for phase 5.3 diagnostics"]
fn archived_actions_have_an_advisory_local_compatibility_report() {
    let prefix_only = std::env::var_os("AWVM_COMPAT_PREFIX_ONLY").is_some();
    let mut prefix = Report::new("fog-off pre-power prefix");
    let mut fog_off = Report::new("all fog-off");
    let mut fog = Report::new("all fog");

    insta::glob!("../../../../assets/replays", "*.zip", |replay_path| {
        let replay_file = replay_path.file_name().unwrap().to_string_lossy();
        let replay = ReplayParser::new()
            .parse(&std::fs::read(replay_path).unwrap())
            .unwrap();
        let game = replay.games.first().expect("archived replay has a game");
        if prefix_only && game.fog {
            return;
        }
        let map: AwbwMapData = serde_json::from_slice(
            &std::fs::read(map_path(&format!("{}.json", game.maps_id.as_u32()))).unwrap(),
        )
        .unwrap();
        let mut adapter = RecordedAdapter::new(&replay, &map).unwrap();
        let prefix_len = if game.fog {
            0
        } else {
            replay
                .turns
                .iter()
                .position(|action| matches!(action, Action::Power(_) | Action::Tag { .. }))
                .unwrap_or(replay.turns.len())
        };
        let action_limit = if prefix_only {
            prefix_len
        } else {
            replay.turns.len()
        };

        for (index, action) in replay.turns.iter().take(action_limit).enumerate() {
            let prior = adapter.state().clone();
            let transition = adapter.advance(action).unwrap_or_else(|error| {
                panic!(
                    "{replay_file} action {index} ({}): {error}",
                    action.kind_name()
                )
            });
            let result =
                diagnose_local_compatibility_until_match(&prior, action, transition.post_state());
            if !game.fog && index < prefix_len {
                prefix.record(&replay_file, index, action, &result);
            }
            if !prefix_only {
                if game.fog {
                    fog.record(&replay_file, index, action, &result);
                } else {
                    fog_off.record(&replay_file, index, action, &result);
                }
            }
        }
    });

    assert_eq!(prefix.actions, 1_981, "fog-off prefix definition drifted");
    if prefix_only {
        println!("{}", prefix.render());
        return;
    }
    assert_eq!(fog_off.actions, 7_226, "fog-off archive size drifted");
    assert_eq!(fog.actions, 5_703, "fog archive size drifted");

    let snapshot = format!(
        "{}\n\n{}\n\n{}",
        prefix.render(),
        fog_off.render(),
        fog.render()
    );
    if std::env::var_os("INSTA_GLOB_FILTER").is_none() {
        insta::assert_snapshot!("compatibility_corpus", snapshot);
    } else {
        println!("{snapshot}");
    }
}

struct Report {
    name: &'static str,
    actions: usize,
    classes: [usize; 3],
    by_action: BTreeMap<&'static str, [usize; 3]>,
    insufficient: BTreeMap<String, usize>,
    divergences: BTreeMap<String, usize>,
    samples: BTreeMap<String, Vec<String>>,
    sequences: BTreeMap<String, HighwayHasher>,
}

impl Report {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            actions: 0,
            classes: [0; 3],
            by_action: BTreeMap::new(),
            insufficient: BTreeMap::new(),
            divergences: BTreeMap::new(),
            samples: BTreeMap::new(),
            sequences: BTreeMap::new(),
        }
    }

    fn record(&mut self, replay: &str, index: usize, action: &Action, result: &LocalCompatibility) {
        let (class, detail) = match result {
            LocalCompatibility::LocallyCompatible(_) => (0, None),
            LocalCompatibility::LocallyDivergent(divergence) => {
                let detail = if !divergence.first_mismatched_components.is_empty() {
                    format!(
                        "mismatch:{}",
                        divergence.first_mismatched_components.join("+")
                    )
                } else if let Some(rejection) = &divergence.first_rejection {
                    format!(
                        "rejected:{}",
                        rejection.split([' ', '{']).next().unwrap_or(rejection)
                    )
                } else if divergence.first_execution_error.is_some() {
                    "execution-error".into()
                } else {
                    "no-execution".into()
                };
                (1, Some(detail))
            }
            LocalCompatibility::InsufficientReplayData(insufficient) => {
                (2, Some(insufficient_bucket(&insufficient.reason)))
            }
        };
        self.actions += 1;
        self.classes[class] += 1;
        self.by_action.entry(action.kind_name()).or_default()[class] += 1;
        if let Some(detail) = detail {
            let histogram = if class == 1 {
                &mut self.divergences
            } else {
                &mut self.insufficient
            };
            *histogram.entry(detail.clone()).or_default() += 1;
            let sample_key = format!("{}:{detail}", if class == 1 { "D" } else { "I" });
            let samples = self.samples.entry(sample_key).or_default();
            if samples.len() < 3 {
                samples.push(format!("{replay}:{index}:{}", action.kind_name()));
            }
        }

        let hasher = self
            .sequences
            .entry(replay.into())
            .or_insert_with(|| HighwayHasher::new(Key::default()));
        hasher.append(&u64::try_from(index).unwrap().to_le_bytes());
        hasher.append(&[u8::try_from(class).unwrap()]);
    }

    fn render(&self) -> String {
        let mut output = String::new();
        writeln!(output, "[{}]", self.name).unwrap();
        writeln!(
            output,
            "total={} compatible={} divergent={} insufficient={}",
            self.actions, self.classes[0], self.classes[1], self.classes[2]
        )
        .unwrap();
        writeln!(output, "by-action (compatible/divergent/insufficient):").unwrap();
        for (action, counts) in &self.by_action {
            writeln!(
                output,
                "  {action:<10} {:>5}/{:>5}/{:>5}",
                counts[0], counts[1], counts[2]
            )
            .unwrap();
        }
        writeln!(output, "divergence-buckets:").unwrap();
        for (reason, count) in &self.divergences {
            writeln!(output, "  {count:>5} {reason}").unwrap();
        }
        writeln!(output, "insufficient-buckets:").unwrap();
        for (reason, count) in &self.insufficient {
            writeln!(output, "  {count:>5} {reason}").unwrap();
        }
        writeln!(output, "bucket-samples:").unwrap();
        for (bucket, samples) in &self.samples {
            writeln!(output, "  {bucket} {}", samples.join(", ")).unwrap();
        }
        writeln!(output, "per-action-classification-digests:").unwrap();
        for (replay, hasher) in &self.sequences {
            writeln!(output, "  {replay} {:016x}", hasher.clone().finalize64()).unwrap();
        }
        output.pop();
        output
    }
}

fn insufficient_bucket(reason: &str) -> String {
    if reason.contains("has no movement payload") {
        "missing-move".into()
    } else if reason.contains("absent from the graphical pre-state") {
        "unit-absent".into()
    } else if reason.contains("beyond the bounded local candidate set") {
        "broad-exact-hp-dependence".into()
    } else if reason.contains("advisory search limit") {
        "advisory-search-limit".into()
    } else {
        reason.into()
    }
}
