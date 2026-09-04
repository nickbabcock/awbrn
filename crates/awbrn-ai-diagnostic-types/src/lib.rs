//! Versioned, filesystem-free types for deterministic AI diagnostics.
//!
//! This crate contains data contracts only. Native persistence, rendering, and
//! match execution belong to `awbrn-ai-diagnostics`.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use serde::{Deserialize, Serialize};

/// The current run manifest schema.
pub const RUN_MANIFEST_SCHEMA_VERSION: u16 = 1;

/// The event log schema version.
pub const EVENT_LOG_SCHEMA_VERSION: u16 = 1;

/// The seat order used by a paired match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SeatOrderVariant {
    AgentFirst,
    BaselineFirst,
}

impl SeatOrderVariant {
    /// Every supported seat order in stable order.
    pub const ALL: [Self; 2] = [Self::AgentFirst, Self::BaselineFirst];

    /// Return the stable manifest name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AgentFirst => "agent-first",
            Self::BaselineFirst => "baseline-first",
        }
    }
}

/// The identity shared by both seat orders of one paired match.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PairKey {
    pub map_id: u32,
    pub run_seed: u64,
    pub pair_index: u64,
}

impl PairKey {
    /// Create a pair identity.
    pub const fn new(map_id: u32, run_seed: u64, pair_index: u64) -> Self {
        Self {
            map_id,
            run_seed,
            pair_index,
        }
    }
}

/// The immutable identity of one seat-order match.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchIdentity {
    pub pair: PairKey,
    pub match_seed: u64,
    pub seat_order: SeatOrderVariant,
    pub configuration_fingerprint: String,
    pub map_fingerprint: String,
}

/// An explicit reason why a match cannot enter the scored reducer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "detail")]
pub enum Invalidation {
    MissingOutcome,
    HarnessError(String),
    TelemetryError(String),
    Abandoned,
    DuplicateMatch,
    IncompletePair,
    IncompatibleIdentity,
    UnexpectedPair,
}

/// One observed seat-order result. Invalid observations remain in the audit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatchObservation {
    pub identity: MatchIdentity,
    pub valid: bool,
    pub invalidation: Option<Invalidation>,
    pub match_points: Option<f64>,
    pub terminal_day: Option<u32>,
    pub terminal_reason: Option<String>,
}

impl MatchObservation {
    /// Create a valid observation with match points.
    pub fn valid(
        identity: MatchIdentity,
        match_points: f64,
        terminal_day: Option<u32>,
        terminal_reason: Option<String>,
    ) -> Self {
        Self {
            identity,
            valid: true,
            invalidation: None,
            match_points: Some(match_points),
            terminal_day,
            terminal_reason,
        }
    }

    /// Create an explicitly invalid observation.
    pub fn invalid(identity: MatchIdentity, reason: Invalidation) -> Self {
        Self {
            identity,
            valid: false,
            invalidation: Some(reason),
            match_points: None,
            terminal_day: None,
            terminal_reason: None,
        }
    }
}

/// Two compatible seat orders reduced to one paired observation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairObservation {
    pub key: PairKey,
    pub agent_first: MatchObservation,
    pub baseline_first: MatchObservation,
    pub differential: f64,
    pub non_day_limit: bool,
}

/// Coverage counts kept separate for audit and gating.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub expected: usize,
    pub attempted: usize,
    pub valid: usize,
    pub invalid: usize,
    pub missing: usize,
}

/// The status of a reduction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReductionStatus {
    Complete,
    Incomplete,
    Invalid,
}

/// Reduced paired results and their coverage audit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reduction {
    pub status: ReductionStatus,
    pub coverage: Coverage,
    pub observations: Vec<PairObservation>,
    pub map_means: BTreeMap<u32, f64>,
    pub observed_mean: f64,
    pub non_day_limit_mean: Option<f64>,
    pub errors: Vec<String>,
}

/// A reducer with an explicit expected coverage plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReductionPlan {
    expected: BTreeSet<PairKey>,
}

impl ReductionPlan {
    /// Build a plan from map ids and an exclusive pair-index range.
    pub fn new(
        map_ids: impl IntoIterator<Item = u32>,
        run_seed: u64,
        pair_indices: Range<u64>,
    ) -> Self {
        let expected = map_ids
            .into_iter()
            .flat_map(|map_id| {
                pair_indices
                    .clone()
                    .map(move |pair_index| PairKey::new(map_id, run_seed, pair_index))
            })
            .collect();
        Self { expected }
    }

    /// Build a plan from exact pair identities.
    pub fn from_pairs(pairs: impl IntoIterator<Item = PairKey>) -> Self {
        Self {
            expected: pairs.into_iter().collect(),
        }
    }

    /// Return expected pairs in stable order.
    pub fn expected_pairs(&self) -> impl Iterator<Item = &PairKey> {
        self.expected.iter()
    }

    /// Reduce observations against this plan.
    pub fn reduce(&self, matches: impl IntoIterator<Item = MatchObservation>) -> Reduction {
        let mut grouped: BTreeMap<PairKey, Vec<MatchObservation>> = BTreeMap::new();
        for observation in matches {
            grouped
                .entry(observation.identity.pair.clone())
                .or_default()
                .push(observation);
        }

        let mut coverage = Coverage {
            expected: self.expected.len(),
            ..Coverage::default()
        };
        let mut observations = Vec::new();
        let mut errors = Vec::new();
        let mut valid_by_map: BTreeMap<u32, Vec<f64>> = BTreeMap::new();
        let mut non_day_by_map: BTreeMap<u32, Vec<f64>> = BTreeMap::new();

        for (key, entries) in &grouped {
            coverage.attempted += 1;
            if !self.expected.contains(key) {
                coverage.invalid += 1;
                errors.push(format!("pair {key:?} is not in the reduction plan"));
                continue;
            }
            if entries.len() != 2 {
                coverage.invalid += 1;
                errors.push(format!("pair {key:?} does not have both seat orders"));
                continue;
            }
            let agent_first = entries
                .iter()
                .find(|entry| entry.identity.seat_order == SeatOrderVariant::AgentFirst);
            let baseline_first = entries
                .iter()
                .find(|entry| entry.identity.seat_order == SeatOrderVariant::BaselineFirst);
            let (Some(agent_first), Some(baseline_first)) = (agent_first, baseline_first) else {
                coverage.invalid += 1;
                errors.push(format!("pair {key:?} is missing a seat order"));
                continue;
            };
            let compatible = agent_first.identity.pair == baseline_first.identity.pair
                && agent_first.identity.match_seed == baseline_first.identity.match_seed
                && !agent_first.identity.configuration_fingerprint.is_empty()
                && agent_first.identity.configuration_fingerprint
                    == baseline_first.identity.configuration_fingerprint
                && !agent_first.identity.map_fingerprint.is_empty()
                && agent_first.identity.map_fingerprint == baseline_first.identity.map_fingerprint;
            if !compatible {
                coverage.invalid += 1;
                errors.push(format!("pair {key:?} has incompatible match identities"));
                continue;
            }
            if !agent_first.valid
                || !baseline_first.valid
                || agent_first.invalidation.is_some()
                || baseline_first.invalidation.is_some()
                || agent_first.match_points.is_none()
                || baseline_first.match_points.is_none()
                || agent_first
                    .match_points
                    .is_some_and(|points| !points.is_finite() || !(0.0..=1.0).contains(&points))
                || baseline_first
                    .match_points
                    .is_some_and(|points| !points.is_finite() || !(0.0..=1.0).contains(&points))
            {
                coverage.invalid += 1;
                errors.push(format!("pair {key:?} has an invalid match observation"));
                continue;
            }
            let differential = agent_first.match_points.unwrap_or_default()
                + baseline_first.match_points.unwrap_or_default()
                - 1.0;
            let non_day_limit = agent_first.terminal_reason.as_deref() != Some("day-limit")
                && baseline_first.terminal_reason.as_deref() != Some("day-limit");
            let pair = PairObservation {
                key: key.clone(),
                agent_first: agent_first.clone(),
                baseline_first: baseline_first.clone(),
                differential,
                non_day_limit,
            };
            coverage.valid += 1;
            valid_by_map
                .entry(key.map_id)
                .or_default()
                .push(differential);
            if non_day_limit {
                non_day_by_map
                    .entry(key.map_id)
                    .or_default()
                    .push(differential);
            }
            observations.push(pair);
        }

        coverage.missing = self
            .expected
            .iter()
            .filter(|key| !grouped.contains_key(*key))
            .count();
        if coverage.missing > 0 {
            errors.push(format!("{} expected pairs are missing", coverage.missing));
        }
        observations.sort_by(|left, right| left.key.cmp(&right.key));

        let map_means = valid_by_map
            .iter()
            .filter(|(_, values)| !values.is_empty())
            .map(|(map_id, values)| (*map_id, mean(values)))
            .collect::<BTreeMap<_, _>>();
        // Each map has one vote. A map with more pairs must not dominate the
        // suite mean.
        let observed_mean = mean(&map_means.values().copied().collect::<Vec<_>>());

        // The companion is complete only when every expected map has an
        // eligible pair. A valid pair on one map is not enough.
        let all_maps_have_non_day_pair = self
            .expected
            .iter()
            .map(|key| key.map_id)
            .collect::<BTreeSet<_>>()
            .iter()
            .all(|map_id| non_day_by_map.contains_key(map_id));
        let non_day_map_means = non_day_by_map
            .values()
            .filter(|values| !values.is_empty())
            .map(|values| mean(values))
            .collect::<Vec<_>>();
        let non_day_limit_mean = all_maps_have_non_day_pair
            .then(|| mean(&non_day_map_means))
            .filter(|_| !non_day_map_means.is_empty());

        let status = if coverage.invalid > 0 {
            ReductionStatus::Invalid
        } else if coverage.missing > 0 || coverage.valid != coverage.expected {
            ReductionStatus::Incomplete
        } else {
            ReductionStatus::Complete
        };
        Reduction {
            status,
            coverage,
            observations,
            map_means,
            observed_mean,
            non_day_limit_mean,
            errors,
        }
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// The execution mode recorded by a run manifest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionMode {
    /// Collect authoritative events and derived diagnostic results.
    #[default]
    Diagnostic,
    /// Measure runtime behavior without diagnostic instrumentation.
    Performance,
}

/// The telemetry mode recorded by a run manifest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TelemetryMode {
    /// Write the authoritative event log.
    #[default]
    Enabled,
    /// Do not write diagnostic events.
    Disabled,
}

/// The match selection used by a diagnostic run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CaptureSelection {
    /// Capture every match in the event log.
    #[default]
    All,
    /// Capture only the listed map, pair, and seat-order selections.
    ExplicitPairs { pairs: Vec<PairSelection> },
}

/// The frame cadence used by a diagnostic run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FramePolicy {
    /// Do not render visual frames.
    #[default]
    Disabled,
    /// Capture the initial state, every turn end, and the terminal state.
    EveryTurn,
    /// Capture the terminal state and a fixed window of turns before it.
    TerminalWindow { before: u32, after: u32 },
    /// Capture selected turn numbers in addition to the initial and terminal states.
    SelectedDays { days: Vec<u64> },
    /// Let the caller capture a state when it reports an anomaly.
    AnomalyTriggered,
}

/// The match selection and frame cadence used by a diagnostic run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturePolicy {
    #[serde(default)]
    pub selection: CaptureSelection,
    #[serde(default)]
    pub frame_policy: FramePolicy,
}

impl CapturePolicy {
    /// Return whether one match is selected for capture.
    pub fn selects(
        &self,
        map_id: u32,
        run_seed: u64,
        pair_index: u64,
        seat_order: SeatOrderVariant,
    ) -> bool {
        match &self.selection {
            CaptureSelection::All => true,
            CaptureSelection::ExplicitPairs { pairs } => pairs.iter().any(|pair| {
                pair.map_id == map_id
                    && pair.run_seed == run_seed
                    && pair.pair_index == pair_index
                    && pair.seat_order == seat_order
            }),
        }
    }
}

/// One explicit visual match selection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairSelection {
    pub map_id: u32,
    pub run_seed: u64,
    pub pair_index: u64,
    pub seat_order: SeatOrderVariant,
}

/// A source map identity recorded in a run manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapIdentity {
    pub map_id: u32,
    pub name: String,
    pub source: String,
    pub source_fingerprint: String,
    pub normalized_fingerprint: String,
}

/// The derivation inputs for match seeds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedDerivation {
    pub run_seed: u64,
    pub algorithm: String,
    pub pair_index_domain: String,
}

/// Limits that affect a match outcome or its measured cost.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunLimits {
    pub day_limit: u32,
    pub node_budget: u32,
    pub refusal_limit: u32,
}

/// The identity of one agent configuration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub identifier: String,
    pub configuration_fingerprint: String,
    pub executable_fingerprint: String,
}

/// A non-map file that affects a diagnostic run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferencedArtifact {
    /// The path as written relative to the plan.
    pub path: String,
    /// The content fingerprint used for identity.
    pub fingerprint: String,
}

/// Expected output fingerprints used by a reducer or review runner.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedFingerprints {
    /// Final cumulative command fingerprints keyed by stable match identity.
    #[serde(default)]
    pub command: BTreeMap<String, String>,
    /// Fingerprint of the immutable JSONL event log.
    #[serde(default)]
    pub event_log: Option<String>,
    /// Derived file expectations in `file=fingerprint` form.
    #[serde(default)]
    pub derived_tables: BTreeSet<String>,
}

/// One complete, versioned description of a deterministic run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunManifest {
    pub schema_version: u16,
    pub run_id: String,
    #[serde(default)]
    pub mode: ExecutionMode,
    #[serde(default)]
    pub telemetry: TelemetryMode,
    pub source_revision: String,
    pub dirty_worktree: bool,
    pub source_fingerprint: String,
    pub executable_fingerprint: String,
    pub configuration_fingerprint: String,
    /// Fingerprint of the user-authored experiment plan.
    #[serde(default)]
    pub experiment_plan_fingerprint: String,
    /// Materialized producer-usability settings for exact reanalysis.
    #[serde(default)]
    pub producer_usability_plan: Option<serde_json::Value>,
    pub maps: Vec<MapIdentity>,
    pub seed_derivation: SeedDerivation,
    pub limits: RunLimits,
    pub agents: Vec<AgentIdentity>,
    /// Model and weight files resolved before execution.
    #[serde(default)]
    pub referenced_artifacts: Vec<ReferencedArtifact>,
    /// Event log path relative to the manifest. Review defaults to events.jsonl.
    #[serde(default)]
    pub event_log: Option<String>,
    #[serde(default)]
    pub capture_policy: CapturePolicy,
    #[serde(default)]
    pub annotations: Option<String>,
    #[serde(default)]
    pub expected: ExpectedFingerprints,
    #[serde(default)]
    pub pairs: Vec<PairKey>,
}

impl RunManifest {
    /// Validate the identity and coverage fields of a manifest.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != RUN_MANIFEST_SCHEMA_VERSION {
            return Err(format!(
                "unsupported run manifest schema {}",
                self.schema_version
            ));
        }
        if self.run_id.is_empty() {
            return Err("run manifest has an empty run id".into());
        }
        if self.source_revision.is_empty() {
            return Err("run manifest has no source revision".into());
        }
        if self.source_fingerprint.is_empty() {
            return Err("run manifest has no source fingerprint".into());
        }
        if self.executable_fingerprint.is_empty() || self.configuration_fingerprint.is_empty() {
            return Err(
                "run manifest is missing an executable or configuration fingerprint".into(),
            );
        }
        if self.maps.is_empty() {
            return Err("run manifest has no maps".into());
        }
        if self.seed_derivation.algorithm.is_empty()
            || self.seed_derivation.pair_index_domain.is_empty()
        {
            return Err("run manifest is missing seed derivation details".into());
        }
        let mut map_ids = BTreeSet::new();
        for map in &self.maps {
            if map.name.is_empty() || map.source.is_empty() {
                return Err(format!("map {} has an empty name or source", map.map_id));
            }
            if !map_ids.insert(map.map_id) {
                return Err(format!("run manifest repeats map {}", map.map_id));
            }
            if map.source_fingerprint.is_empty() || map.normalized_fingerprint.is_empty() {
                return Err(format!("map {} is missing a fingerprint", map.map_id));
            }
        }
        if self.agents.len() < 2 {
            return Err("run manifest needs at least two agents".into());
        }
        for artifact in &self.referenced_artifacts {
            if artifact.path.is_empty() || artifact.fingerprint.is_empty() {
                return Err("run manifest has an incomplete referenced artifact".into());
            }
            let path = std::path::Path::new(&artifact.path);
            if path.is_absolute()
                || path.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::CurDir | std::path::Component::ParentDir
                    )
                })
            {
                return Err(format!(
                    "run manifest has an unsafe referenced artifact path {:?}",
                    artifact.path
                ));
            }
        }
        if self.agents.iter().any(|agent| {
            agent.identifier.is_empty()
                || agent.configuration_fingerprint.is_empty()
                || agent.executable_fingerprint.is_empty()
        }) {
            return Err("run manifest has an incomplete agent identity".into());
        }
        let mut pairs = BTreeSet::new();
        for pair in &self.pairs {
            if !map_ids.contains(&pair.map_id)
                || pair.run_seed != self.seed_derivation.run_seed
                || !pairs.insert(pair.clone())
            {
                return Err(format!(
                    "run manifest has an invalid or repeated pair {pair:?}"
                ));
            }
        }
        if let CaptureSelection::ExplicitPairs { pairs } = &self.capture_policy.selection
            && pairs.iter().any(|selection| {
                !map_ids.contains(&selection.map_id)
                    || selection.run_seed != self.seed_derivation.run_seed
            })
        {
            return Err("run manifest has an invalid explicit capture pair".into());
        }
        if self.event_log.as_deref().is_some_and(str::is_empty) {
            return Err("run manifest has an empty event log path".into());
        }
        Ok(())
    }

    /// Return a stable fingerprint of the validated manifest contents.
    pub fn fingerprint(&self) -> Result<String, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        Ok(format!("{:016x}", stable_hash(&bytes)))
    }

    /// Parse and validate a manifest from JSON bytes.
    pub fn from_json(bytes: &[u8]) -> Result<Self, RunManifestError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| RunManifestError::Json(error.to_string()))?;
        manifest
            .validate()
            .map_err(RunManifestError::Invalid)
            .map(|()| manifest)
    }

    /// Serialize a validated manifest to pretty JSON bytes.
    pub fn to_json(&self) -> Result<Vec<u8>, RunManifestError> {
        self.validate().map_err(RunManifestError::Invalid)?;
        serde_json::to_vec_pretty(self).map_err(|error| RunManifestError::Json(error.to_string()))
    }

    /// Return the expected pair keys in deterministic order.
    pub fn expected_pairs(&self) -> Vec<PairKey> {
        let mut pairs = self.pairs.clone();
        pairs.sort();
        pairs
    }
}

/// Errors while parsing or serializing a run manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunManifestError {
    Json(String),
    Invalid(String),
}

impl std::fmt::Display for RunManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(f, "run manifest JSON failed: {error}"),
            Self::Invalid(error) => write!(f, "invalid run manifest: {error}"),
        }
    }
}

impl std::error::Error for RunManifestError {}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// Return a stable fingerprint for raw bytes.
pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    format!("{:016x}", stable_hash(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> RunManifest {
        RunManifest {
            schema_version: RUN_MANIFEST_SCHEMA_VERSION,
            run_id: "run".into(),
            mode: ExecutionMode::Diagnostic,
            telemetry: TelemetryMode::Enabled,
            source_revision: "revision".into(),
            dirty_worktree: false,
            source_fingerprint: "source".into(),
            executable_fingerprint: "executable".into(),
            configuration_fingerprint: "configuration".into(),
            experiment_plan_fingerprint: String::new(),
            producer_usability_plan: None,
            maps: vec![MapIdentity {
                map_id: 1,
                name: "map".into(),
                source: "map.json".into(),
                source_fingerprint: "source".into(),
                normalized_fingerprint: "normalized".into(),
            }],
            seed_derivation: SeedDerivation {
                run_seed: 1,
                algorithm: "algorithm".into(),
                pair_index_domain: "0..1".into(),
            },
            limits: RunLimits {
                day_limit: 1,
                node_budget: 1,
                refusal_limit: 1,
            },
            agents: vec![
                AgentIdentity {
                    identifier: "one".into(),
                    configuration_fingerprint: "one-config".into(),
                    executable_fingerprint: "one-exe".into(),
                },
                AgentIdentity {
                    identifier: "two".into(),
                    configuration_fingerprint: "two-config".into(),
                    executable_fingerprint: "two-exe".into(),
                },
            ],
            referenced_artifacts: Vec::new(),
            event_log: None,
            capture_policy: CapturePolicy::default(),
            annotations: None,
            expected: ExpectedFingerprints::default(),
            pairs: vec![PairKey::new(1, 1, 0)],
        }
    }

    #[test]
    fn manifest_round_trips_without_filesystem() {
        let manifest = manifest();
        let bytes = manifest.to_json().expect("manifest serializes");
        let parsed = RunManifest::from_json(&bytes).expect("manifest parses");
        assert_eq!(parsed, manifest);
        assert!(
            !manifest
                .fingerprint()
                .expect("manifest fingerprints")
                .is_empty()
        );
    }

    #[test]
    fn capture_policy_defaults_without_legacy_shape() {
        let policy: CapturePolicy = serde_json::from_str("{}").expect("the policy parses");
        assert_eq!(policy, CapturePolicy::default());
        assert_eq!(policy.frame_policy, FramePolicy::Disabled);
    }

    #[test]
    fn missing_expected_pairs_are_incomplete() {
        let plan = ReductionPlan::new([1, 2], 9, 0..2);
        let reduction = plan.reduce([]);
        assert_eq!(reduction.status, ReductionStatus::Incomplete);
        assert_eq!(reduction.coverage.missing, 4);
        assert!(reduction.non_day_limit_mean.is_none());
    }

    fn observation(
        pair: PairKey,
        seat_order: SeatOrderVariant,
        match_points: f64,
    ) -> MatchObservation {
        MatchObservation::valid(
            MatchIdentity {
                pair,
                match_seed: 7,
                seat_order,
                configuration_fingerprint: "configuration".into(),
                map_fingerprint: "map".into(),
            },
            match_points,
            Some(3),
            Some("terminal".into()),
        )
    }

    #[test]
    fn suite_means_give_each_map_one_vote() {
        let pairs = [
            PairKey::new(1, 1, 0),
            PairKey::new(1, 1, 1),
            PairKey::new(2, 1, 0),
        ];
        let plan = ReductionPlan::from_pairs(pairs.iter().cloned());
        let mut matches = Vec::new();
        for pair in pairs.iter().take(2) {
            matches.push(observation(pair.clone(), SeatOrderVariant::AgentFirst, 1.0));
            matches.push(observation(
                pair.clone(),
                SeatOrderVariant::BaselineFirst,
                1.0,
            ));
        }
        let pair = pairs[2].clone();
        matches.push(observation(pair.clone(), SeatOrderVariant::AgentFirst, 0.0));
        matches.push(observation(pair, SeatOrderVariant::BaselineFirst, 0.0));
        let reduction = plan.reduce(matches);
        assert_eq!(reduction.status, ReductionStatus::Complete);
        assert_eq!(reduction.map_means.get(&1), Some(&1.0));
        assert_eq!(reduction.map_means.get(&2), Some(&-1.0));
        assert_eq!(reduction.observed_mean, 0.0);
        assert_eq!(reduction.non_day_limit_mean, Some(0.0));
    }

    #[test]
    fn explicit_invalidation_is_not_scored() {
        let pair = PairKey::new(1, 1, 0);
        let plan = ReductionPlan::from_pairs([pair.clone()]);
        let identity = MatchIdentity {
            pair: pair.clone(),
            match_seed: 7,
            seat_order: SeatOrderVariant::AgentFirst,
            configuration_fingerprint: "configuration".into(),
            map_fingerprint: "map".into(),
        };
        let invalid =
            MatchObservation::invalid(identity, Invalidation::HarnessError("stop".into()));
        let baseline = observation(pair, SeatOrderVariant::BaselineFirst, 0.0);
        let reduction = plan.reduce([invalid, baseline]);
        assert_eq!(reduction.status, ReductionStatus::Invalid);
        assert_eq!(reduction.coverage.attempted, 1);
        assert_eq!(reduction.coverage.valid, 0);
        assert_eq!(reduction.coverage.invalid, 1);
        assert_eq!(reduction.coverage.missing, 0);
    }
}
