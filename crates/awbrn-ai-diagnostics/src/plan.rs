//! User-authored experiment plans and materialized run manifests.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use awbrn_ai::EvalWeights;
use awbrn_ai::agent::NodeBudget;
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai_diagnostic_types::{
    AgentSeedProtocol, CapturePolicy, ExecutionMode, MapIdentity, PairKey,
    RUN_MANIFEST_SCHEMA_VERSION, ReferencedArtifact, RunLimits, RunManifest, SeedDerivation,
    TelemetryMode, fingerprint_bytes,
};
use serde::{Deserialize, Serialize};

use crate::LearnedFactory;
use crate::feature_analysis::{
    FEATURE_ANALYSIS_SCHEMA_VERSION, FEATURE_NAMES, FeatureAnalysisReport, FeatureMode,
};
use crate::map_registry::MapRegistry;
use crate::producer_diagnostics::ProducerUsabilityPlan;
use crate::source::source_provenance;
use crate::tactical::{TacticalFactory, TacticalRerank, TacticalRerankMode};
use crate::tournament::{
    AgentFactory, AiProfileFactory, PlannerFactory, SearchFactory, StrategicFactory,
};
use awbrn_ai::planner::PlannerConfig;

/// The current experiment plan schema.
pub const EXPERIMENT_PLAN_SCHEMA_VERSION: u16 = 1;

/// A candidate agent configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AgentSpec {
    /// A versioned opponent profile stored in match records.
    AiProfile { profile_id: String },
    /// A production or locked strategic configuration.
    Strategic { configuration: String },
    /// A search configuration built from a named weight preset.
    Search {
        identifier: String,
        preset: String,
        node_budget: u32,
    },
    /// A learned reranker backed by an offline fog-visible report.
    LearnedRerank {
        model: PathBuf,
        baseline_configuration: String,
        top_k: usize,
    },
    /// The whole-turn planner. This is not a production profile.
    ///
    /// The optional fields change one value of the named configuration.
    Planner {
        identifier: String,
        configuration: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_weight: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        front: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_work: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plans_per_turn: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kill_plans: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        safety_plans: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        block_plans: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seed_margin: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hard_reply_top: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        power_first: Option<bool>,
        /// Greedy weights that replace those of the seed policy, by name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        baseline_weights: Option<serde_json::Map<String, serde_json::Value>>,
    },
    /// An opt-in tactical reranker. This is not a production profile.
    TacticalRerank {
        identifier: String,
        configuration: String,
        top_k: usize,
        mode: TacticalMode,
        penalty_percent: u16,
    },
}

/// The tactical exposure scope in an experiment plan.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TacticalMode {
    Collateral,
    CaptureOnly,
}

/// Analysis stages requested after the run.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnalysisStage {
    OutcomeFeatures,
    ProducerUsability,
    Review,
    Verification,
}

/// One user-authored diagnostic experiment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentPlan {
    pub schema_version: u16,
    pub run_id: String,
    pub candidate: AgentSpec,
    pub baseline: AgentSpec,
    pub maps: Vec<u32>,
    pub run_seed: u64,
    pub pairs_per_map: u64,
    /// Assign agent random streams by role or by physical seat.
    #[serde(default, skip_serializing_if = "is_role_seeded")]
    pub agent_seed_protocol: AgentSeedProtocol,
    pub limits: RunLimits,
    #[serde(default)]
    pub telemetry: TelemetryMode,
    #[serde(default)]
    pub capture_policy: CapturePolicy,
    #[serde(default)]
    pub analyses: Vec<AnalysisStage>,
    /// Fixture and threshold configuration for producer usability analysis.
    #[serde(default)]
    pub producer_usability: Option<ProducerUsabilityPlan>,
    #[serde(default)]
    pub annotations: Option<String>,
}

fn is_role_seeded(protocol: &AgentSeedProtocol) -> bool {
    *protocol == AgentSeedProtocol::RoleSeeded
}

/// A plan with its immutable manifest and resolved factories.
pub struct MaterializedPlan {
    pub manifest: RunManifest,
    pub candidate: Box<dyn AgentFactory>,
    pub baseline: Box<dyn AgentFactory>,
    pub analyses: Vec<AnalysisStage>,
    pub producer_usability: Option<ProducerUsabilityPlan>,
}

impl fmt::Debug for MaterializedPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MaterializedPlan")
            .field("manifest", &self.manifest)
            .field("candidate", &self.candidate.identity())
            .field("baseline", &self.baseline.identity())
            .field("analyses", &self.analyses)
            .finish()
    }
}

/// Errors while loading or resolving a plan.
#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("experiment plan I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("experiment plan JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("experiment plan map error: {0}")]
    Map(#[from] crate::map_registry::MapRegistryError),
    #[error("experiment plan configuration error: {0}")]
    Configuration(String),
}

/// Read and validate a plan.
pub fn read_plan(path: impl AsRef<Path>) -> Result<ExperimentPlan, PlanError> {
    let plan: ExperimentPlan = serde_json::from_slice(&fs::read(path)?)?;
    plan.validate()?;
    Ok(plan)
}

impl ExperimentPlan {
    /// Validate fields that do not depend on the map registry.
    pub fn validate(&self) -> Result<(), PlanError> {
        if self.schema_version != EXPERIMENT_PLAN_SCHEMA_VERSION {
            return Err(PlanError::Configuration(format!(
                "unsupported experiment plan schema {}",
                self.schema_version
            )));
        }
        if self.run_id.is_empty() {
            return Err(PlanError::Configuration(
                "experiment plan needs a run id".into(),
            ));
        }
        if self.maps.is_empty() {
            return Err(PlanError::Configuration(
                "experiment plan needs at least one map".into(),
            ));
        }
        if self.pairs_per_map == 0 {
            return Err(PlanError::Configuration(
                "experiment plan needs a positive pairs_per_map".into(),
            ));
        }
        let mut maps = BTreeSet::new();
        if self.maps.iter().any(|map| !maps.insert(*map)) {
            return Err(PlanError::Configuration(
                "experiment plan repeats a map".into(),
            ));
        }
        if self.limits.day_limit == 0
            || self.limits.node_budget == 0
            || self.limits.refusal_limit == 0
        {
            return Err(PlanError::Configuration(
                "experiment plan limits must be positive".into(),
            ));
        }
        let mut stages = BTreeSet::new();
        if self.analyses.iter().any(|stage| !stages.insert(*stage)) {
            return Err(PlanError::Configuration(
                "experiment plan repeats an analysis stage".into(),
            ));
        }
        if self.analyses.contains(&AnalysisStage::ProducerUsability)
            && self.candidate != self.baseline
        {
            return Err(PlanError::Configuration(
                "producer usability requires the same agent configuration on both sides".into(),
            ));
        }
        if self.analyses.contains(&AnalysisStage::ProducerUsability)
            && self.producer_usability.is_none()
        {
            return Err(PlanError::Configuration(
                "producer usability needs materialized plan settings".into(),
            ));
        }
        if let Some(producer_usability) = &self.producer_usability {
            producer_usability
                .validate()
                .map_err(PlanError::Configuration)?;
            if !self.analyses.contains(&AnalysisStage::ProducerUsability) {
                return Err(PlanError::Configuration(
                    "producer usability settings need the producer-usability analysis stage".into(),
                ));
            }
        }
        Ok(())
    }

    /// Resolve agents and materialize the immutable run manifest.
    pub fn materialize(
        &self,
        plan_path: impl AsRef<Path>,
        registry: &MapRegistry,
    ) -> Result<MaterializedPlan, PlanError> {
        self.validate()?;
        let (candidate, mut artifacts) = self.candidate.materialize(plan_path.as_ref())?;
        let (baseline, baseline_artifacts) = self.baseline.materialize(plan_path.as_ref())?;
        artifacts.extend(baseline_artifacts);
        let maps = self
            .maps
            .iter()
            .map(|map_id| {
                let map = registry.get(*map_id).ok_or_else(|| {
                    PlanError::Configuration(format!("map {map_id} is not in the fixed registry"))
                })?;
                Ok(MapIdentity {
                    map_id: map.id,
                    name: map.name.clone(),
                    source: map.source_path.clone(),
                    source_fingerprint: map.source_fingerprint.clone(),
                    normalized_fingerprint: map.normalized_fingerprint.clone(),
                    fog: map.fog,
                })
            })
            .collect::<Result<Vec<_>, PlanError>>()?;
        let pairs = self
            .maps
            .iter()
            .flat_map(|map_id| {
                (0..self.pairs_per_map)
                    .map(move |pair_index| PairKey::new(*map_id, self.run_seed, pair_index))
            })
            .collect::<Vec<_>>();
        let source = source_provenance(plan_path.as_ref())?;
        let experiment_plan_fingerprint = fingerprint_bytes(&serde_json::to_vec(self)?);
        let legacy_configuration = (
            candidate.identity(),
            baseline.identity(),
            &maps,
            &self.maps,
            self.run_seed,
            self.pairs_per_map,
            &self.limits,
            self.telemetry,
            &self.capture_policy,
            &self.analyses,
            &self.producer_usability,
            &source.fingerprint,
            artifacts
                .iter()
                .map(|artifact| artifact.fingerprint.as_str())
                .collect::<Vec<_>>(),
        );
        let configuration_bytes = match self.agent_seed_protocol {
            AgentSeedProtocol::RoleSeeded => serde_json::to_vec(&legacy_configuration)?,
            AgentSeedProtocol::SeatSeeded => {
                serde_json::to_vec(&(&legacy_configuration, self.agent_seed_protocol))?
            }
        };
        let configuration_fingerprint = fingerprint_bytes(&configuration_bytes);
        artifacts.sort_by(|left, right| left.path.cmp(&right.path));
        let manifest = RunManifest {
            schema_version: RUN_MANIFEST_SCHEMA_VERSION,
            run_id: self.run_id.clone(),
            mode: ExecutionMode::Diagnostic,
            telemetry: self.telemetry,
            source_revision: source.revision,
            dirty_worktree: source.dirty,
            source_fingerprint: source.fingerprint,
            executable_fingerprint: format!(
                "{}+{}",
                candidate.identity().executable_fingerprint,
                baseline.identity().executable_fingerprint
            ),
            configuration_fingerprint,
            experiment_plan_fingerprint,
            producer_usability_plan: self
                .producer_usability
                .as_ref()
                .map(serde_json::to_value)
                .transpose()?,
            maps,
            seed_derivation: SeedDerivation {
                agent_seed_protocol: self.agent_seed_protocol,
                run_seed: self.run_seed,
                algorithm: "baseline-game-seed-v1".into(),
                pair_index_domain: format!("0..{}", self.pairs_per_map),
            },
            limits: self.limits.clone(),
            agents: vec![candidate.identity().clone(), baseline.identity().clone()],
            referenced_artifacts: artifacts,
            event_log: None,
            capture_policy: self.capture_policy.clone(),
            annotations: self.annotations.clone(),
            expected: Default::default(),
            pairs,
        };
        manifest.validate().map_err(PlanError::Configuration)?;
        Ok(MaterializedPlan {
            manifest,
            candidate,
            baseline,
            analyses: self.analyses.clone(),
            producer_usability: self.producer_usability.clone(),
        })
    }
}

impl AgentSpec {
    /// Build the agent factory for this specification.
    ///
    /// `plan_path` resolves relative model paths.
    pub fn materialize(
        &self,
        plan_path: &Path,
    ) -> Result<(Box<dyn AgentFactory>, Vec<ReferencedArtifact>), PlanError> {
        match self {
            Self::AiProfile { profile_id } => Ok((
                Box::new(AiProfileFactory::new(profile_id).map_err(PlanError::Configuration)?),
                Vec::new(),
            )),
            Self::Strategic { configuration } => {
                if configuration.is_empty() {
                    return Err(PlanError::Configuration(
                        "strategic configuration must not be empty".into(),
                    ));
                }
                Ok((
                    Box::new(StrategicFactory::new(configuration_name(configuration)?)),
                    Vec::new(),
                ))
            }
            Self::Search {
                identifier,
                preset,
                node_budget,
            } => {
                let config = configuration_name(preset)?;
                let budget = NodeBudget::new(*node_budget).ok_or_else(|| {
                    PlanError::Configuration("search node_budget must be positive".into())
                })?;
                if identifier.is_empty() {
                    return Err(PlanError::Configuration(
                        "search identifier must not be empty".into(),
                    ));
                }
                Ok((
                    Box::new(SearchFactory::new(
                        identifier,
                        config.weights,
                        EvalWeights::STANDARD,
                        budget,
                    )),
                    Vec::new(),
                ))
            }
            Self::LearnedRerank {
                model,
                baseline_configuration,
                top_k,
            } => {
                let model_path = resolve_artifact_path(plan_path, model)?;
                let model_bytes = fs::read(&model_path)?;
                let report: FeatureAnalysisReport = serde_json::from_slice(&model_bytes)?;
                if report.schema_version != FEATURE_ANALYSIS_SCHEMA_VERSION {
                    return Err(PlanError::Configuration(format!(
                        "learned model schema {} is not supported; expected {}",
                        report.schema_version, FEATURE_ANALYSIS_SCHEMA_VERSION
                    )));
                }
                if *top_k == 0 {
                    return Err(PlanError::Configuration(
                        "learned top_k must be positive".into(),
                    ));
                }
                let visible = report
                    .modes
                    .iter()
                    .find(|mode| mode.mode == FeatureMode::FogVisible)
                    .ok_or_else(|| {
                        PlanError::Configuration(
                            "learned rerank needs a fog-visible feature model".into(),
                        )
                    })?;
                validate_learned_model(&report, visible)?;
                if !visible.model.converged || !visible.model.reduced_converged {
                    return Err(PlanError::Configuration(
                        "learned model did not converge; it cannot be used as a candidate".into(),
                    ));
                }
                let factory = LearnedFactory::from_report_with_content_fingerprint(
                    visible,
                    configuration_name(baseline_configuration)?,
                    *top_k,
                    fingerprint_bytes(&model_bytes),
                )
                .map_err(PlanError::Configuration)?;
                Ok((
                    Box::new(factory),
                    vec![ReferencedArtifact {
                        path: normalized_artifact_path(model)?,
                        fingerprint: fingerprint_bytes(&model_bytes),
                    }],
                ))
            }
            Self::TacticalRerank {
                identifier,
                configuration,
                top_k,
                mode,
                penalty_percent,
            } => {
                let mode = match mode {
                    TacticalMode::Collateral => TacticalRerankMode::Collateral,
                    TacticalMode::CaptureOnly => TacticalRerankMode::CaptureOnly,
                };
                if identifier.is_empty() {
                    return Err(PlanError::Configuration(
                        "tactical identifier must not be empty".into(),
                    ));
                }
                let rerank = TacticalRerank::configured(*top_k, mode, *penalty_percent)
                    .ok_or_else(|| {
                        PlanError::Configuration("tactical rerank top_k must be positive".into())
                    })?;
                Ok((
                    Box::new(TacticalFactory::new(
                        identifier,
                        configuration_name(configuration)?,
                        rerank,
                    )),
                    Vec::new(),
                ))
            }
            Self::Planner {
                identifier,
                configuration,
                reply_weight,
                front,
                turn_work,
                plans_per_turn,
                kill_plans,
                safety_plans,
                block_plans,
                seed_margin,
                hard_reply_top,
                power_first,
                baseline_weights,
            } => {
                if identifier.is_empty() {
                    return Err(PlanError::Configuration(
                        "planner identifier must not be empty".into(),
                    ));
                }
                let mut config = match configuration.as_str() {
                    "v1" => PlannerConfig::V1,
                    "v2" => PlannerConfig::V2,
                    "v3" => PlannerConfig::V3,
                    "v4" => PlannerConfig::V4,
                    "v5" => PlannerConfig::V5,
                    other => {
                        return Err(PlanError::Configuration(format!(
                            "unknown planner configuration {other}"
                        )));
                    }
                };
                if let Some(value) = reply_weight {
                    config.reply_weight = *value;
                }
                if let Some(value) = front {
                    config.eval_weights.front = *value;
                }
                if let Some(value) = turn_work {
                    config.turn_work = (*value > 0).then_some(*value);
                }
                if let Some(value) = plans_per_turn {
                    if *value == 0 {
                        return Err(PlanError::Configuration(
                            "planner plans_per_turn must be positive".into(),
                        ));
                    }
                    config.plans_per_turn = *value;
                }
                if let Some(value) = kill_plans {
                    config.kill_plans = *value;
                }
                if let Some(value) = safety_plans {
                    config.safety_plans = *value;
                }
                if let Some(value) = block_plans {
                    config.block_plans = *value;
                }
                if let Some(value) = seed_margin {
                    config.seed_margin = *value;
                }
                if let Some(value) = hard_reply_top {
                    config.hard_reply_top = *value;
                }
                if let Some(value) = power_first {
                    config.power_first = *value;
                }
                if let Some(overrides) = baseline_weights {
                    let mut weights = serde_json::to_value(config.baseline.weights)
                        .map_err(|error| PlanError::Configuration(error.to_string()))?;
                    let fields = weights
                        .as_object_mut()
                        .expect("greedy weights serialize as an object");
                    // The weights deny unknown fields, so a misspelled name
                    // fails when they are read back.
                    for (name, value) in overrides {
                        fields.insert(name.clone(), value.clone());
                    }
                    config.baseline.weights = serde_json::from_value(weights)
                        .map_err(|error| PlanError::Configuration(error.to_string()))?;
                }
                if [
                    config.reply_weight,
                    config.eval_weights.front,
                    config.seed_margin,
                ]
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
                {
                    return Err(PlanError::Configuration(
                        "planner score overrides must be finite and nonnegative".into(),
                    ));
                }
                Ok((
                    Box::new(PlannerFactory::new(identifier, config)),
                    Vec::new(),
                ))
            }
        }
    }
}

fn plan_directory(plan_path: &Path) -> &Path {
    plan_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn resolve_artifact_path(plan_path: &Path, artifact: &Path) -> Result<PathBuf, PlanError> {
    if artifact.as_os_str().is_empty()
        || artifact.is_absolute()
        || artifact.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return Err(PlanError::Configuration(format!(
            "referenced artifact path must be a safe plan-relative path: {artifact:?}"
        )));
    }
    Ok(plan_directory(plan_path).join(artifact))
}

fn normalized_artifact_path(path: &Path) -> Result<String, PlanError> {
    if path.as_os_str().is_empty() {
        return Err(PlanError::Configuration(
            "referenced artifact path must not be empty".into(),
        ));
    }
    Ok(path.to_string_lossy().replace('\\', "/"))
}

fn configuration_name(name: &str) -> Result<BaselineConfig, PlanError> {
    match name {
        "locked" => Ok(BaselineConfig::LOCKED),
        "production" => Ok(BaselineConfig::PRODUCTION),
        other => Err(PlanError::Configuration(format!(
            "unknown baseline configuration {other:?}; use locked or production"
        ))),
    }
}

fn validate_learned_model(
    report: &FeatureAnalysisReport,
    visible: &crate::feature_analysis::ModeAnalysisReport,
) -> Result<(), PlanError> {
    let expected = FEATURE_NAMES
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    if visible.model.feature_names != expected {
        return Err(PlanError::Configuration(
            "learned model feature schema is incompatible".into(),
        ));
    }
    if report.corpus_fingerprint.is_empty() {
        return Err(PlanError::Configuration(
            "learned model has no corpus fingerprint".into(),
        ));
    }
    if !report.sufficient_corpus {
        return Err(PlanError::Configuration(format!(
            "learned model corpus is insufficient: {} matches with rows; need at least {}",
            report.matches_with_rows, report.minimum_matches
        )));
    }
    if visible.model.reduced_weights.is_empty() {
        return Err(PlanError::Configuration(
            "learned model has no reduced feature weights".into(),
        ));
    }
    let mut names = BTreeSet::new();
    if visible.model.reduced_weights.iter().any(|weight| {
        !FEATURE_NAMES.contains(&weight.name.as_str())
            || !weight.coefficient.is_finite()
            || !names.insert(weight.name.as_str())
    }) {
        return Err(PlanError::Configuration(
            "learned model has invalid reduced feature weights".into(),
        ));
    }
    if !visible.model.reduced_intercept.is_finite() {
        return Err(PlanError::Configuration(
            "learned model has an invalid reduced intercept".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn planner_limits_and_score_overrides_are_validated() {
        let mut input = serde_json::json!({
            "kind": "planner", "identifier": "validation", "configuration": "v3",
            "kill_plans": 0, "safety_plans": 0, "block_plans": 0,
            "hard_reply_top": 0, "reply_weight": 0.0
        });
        let spec: super::AgentSpec = serde_json::from_value(input.clone()).unwrap();
        spec.materialize(std::path::Path::new("plan.json")).unwrap();
        for (key, value) in [
            ("plans_per_turn", serde_json::json!(0)),
            ("reply_weight", serde_json::json!(-1.0)),
            ("front", serde_json::json!(-1.0)),
            ("seed_margin", serde_json::json!(-1.0)),
        ] {
            input[key] = value;
            let spec: super::AgentSpec = serde_json::from_value(input.clone()).unwrap();
            assert!(spec.materialize(std::path::Path::new("plan.json")).is_err());
            input.as_object_mut().unwrap().remove(key);
        }
    }

    use super::*;

    #[test]
    fn ai_profile_plan_resolves_the_current_hard_profile() {
        let spec = AgentSpec::AiProfile {
            profile_id: "ai-hard-v2".into(),
        };
        let (factory, artifacts) = spec
            .materialize(Path::new("/tmp/profile-plan.json"))
            .expect("the current profile resolves");

        assert!(artifacts.is_empty());
        assert_eq!(factory.identity().identifier, "ai-hard-v2");
        assert!(!factory.identity().configuration_fingerprint.is_empty());
        assert_eq!(
            factory.identity().executable_fingerprint,
            crate::tournament::AI_PROFILE_EXECUTABLE_FINGERPRINT
        );
    }

    #[test]
    fn ai_profile_plan_rejects_unknown_identifiers() {
        let spec = AgentSpec::AiProfile {
            profile_id: "ai-hard-v99".into(),
        };
        assert!(
            spec.materialize(Path::new("/tmp/profile-plan.json"))
                .is_err()
        );
    }
    #[test]
    fn old_plans_keep_role_seeded_identity() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/smoke-plan.json");
        let original: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let plan = read_plan(&path).unwrap();
        assert_eq!(plan.agent_seed_protocol, AgentSeedProtocol::RoleSeeded);
        let encoded = serde_json::to_value(&plan).unwrap();
        assert!(encoded.get("agent_seed_protocol").is_none());
        let mut explicit = original;
        explicit["agent_seed_protocol"] = serde_json::json!("role-seeded");
        let restored: ExperimentPlan = serde_json::from_value(explicit).unwrap();
        assert_eq!(
            serde_json::to_vec(&restored).unwrap(),
            serde_json::to_vec(&plan).unwrap()
        );
    }

    #[test]
    fn seat_seeded_configuration_has_a_distinct_identity() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/ai-diagnostics/smoke-plan.json");
        let registry = MapRegistry::load_checked_in().unwrap();
        let mut plan = read_plan(&path).unwrap();
        let role = plan.materialize(&path, &registry).unwrap().manifest;
        plan.agent_seed_protocol = AgentSeedProtocol::SeatSeeded;
        let seat = plan.materialize(&path, &registry).unwrap().manifest;
        assert_ne!(
            role.experiment_plan_fingerprint,
            seat.experiment_plan_fingerprint
        );
        assert_ne!(
            role.configuration_fingerprint,
            seat.configuration_fingerprint
        );
        assert_eq!(
            seat.seed_derivation.agent_seed_protocol,
            AgentSeedProtocol::SeatSeeded
        );
    }
}
