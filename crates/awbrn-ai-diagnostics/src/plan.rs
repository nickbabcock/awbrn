//! User-authored experiment plans and materialized run manifests.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use awbrn_ai::EvalWeights;
use awbrn_ai::agent::NodeBudget;
use awbrn_ai::baseline::BaselineConfig;
use awbrn_ai_diagnostic_types::{
    CapturePolicy, ExecutionMode, MapIdentity, PairKey, RUN_MANIFEST_SCHEMA_VERSION,
    ReferencedArtifact, RunLimits, RunManifest, SeedDerivation, TelemetryMode, fingerprint_bytes,
};
use serde::{Deserialize, Serialize};

use crate::LearnedFactory;
use crate::feature_analysis::{
    FEATURE_ANALYSIS_SCHEMA_VERSION, FEATURE_NAMES, FeatureAnalysisReport, FeatureMode,
};
use crate::map_registry::MapRegistry;
use crate::producer_diagnostics::ProducerUsabilityPlan;
use crate::tactical::{TacticalFactory, TacticalRerank, TacticalRerankMode};
use crate::tournament::{AgentFactory, SearchFactory, StrategicFactory};

/// The current experiment plan schema.
pub const EXPERIMENT_PLAN_SCHEMA_VERSION: u16 = 1;

/// A candidate agent configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AgentSpec {
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
        let configuration_bytes = serde_json::to_vec(&(
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
        ))?;
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
    fn materialize(
        &self,
        plan_path: &Path,
    ) -> Result<(Box<dyn AgentFactory>, Vec<ReferencedArtifact>), PlanError> {
        match self {
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
        }
    }
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

struct SourceProvenance {
    revision: String,
    dirty: bool,
    fingerprint: String,
}

/// Capture the Git source state that can affect a diagnostic run.
fn source_provenance(plan_path: &Path) -> Result<SourceProvenance, PlanError> {
    let root = git_root(plan_path)?;
    let revision = git_text(&root, &["rev-parse", "HEAD"])?;
    let diff = git_bytes(
        &root,
        &[
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            "HEAD",
            "--",
        ],
    )?;
    let untracked = git_bytes(&root, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut paths = untracked
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect::<Vec<_>>();
    paths.sort();

    let mut fingerprint_input = b"awbrn-source-fingerprint-v1\0".to_vec();
    append_fingerprint_part(&mut fingerprint_input, revision.as_bytes());
    append_fingerprint_part(&mut fingerprint_input, &diff);
    for path in &paths {
        append_fingerprint_part(&mut fingerprint_input, path.as_bytes());
        let bytes = fs::read(root.join(path)).map_err(|error| {
            PlanError::Configuration(format!(
                "cannot read untracked source file {path:?} for source fingerprint: {error}"
            ))
        })?;
        append_fingerprint_part(&mut fingerprint_input, &bytes);
    }
    Ok(SourceProvenance {
        revision,
        dirty: !diff.is_empty() || !paths.is_empty(),
        fingerprint: fingerprint_bytes(&fingerprint_input),
    })
}

fn git_root(plan_path: &Path) -> Result<PathBuf, PlanError> {
    for directory in [plan_directory(plan_path), Path::new(".")] {
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(directory)
            .output();
        if let Ok(output) = output
            && output.status.success()
        {
            let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if !root.is_empty() {
                return Ok(PathBuf::from(root));
            }
        }
    }
    Err(PlanError::Configuration(
        "cannot determine the Git repository for source provenance".into(),
    ))
}

fn git_bytes(root: &Path, arguments: &[&str]) -> Result<Vec<u8>, PlanError> {
    let output = std::process::Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| PlanError::Configuration(format!("cannot run Git: {error}")))?;
    if !output.status.success() {
        return Err(PlanError::Configuration(format!(
            "Git command {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}

fn git_text(root: &Path, arguments: &[&str]) -> Result<String, PlanError> {
    let bytes = git_bytes(root, arguments)?;
    let value = String::from_utf8_lossy(&bytes).trim().to_owned();
    if value.is_empty() {
        return Err(PlanError::Configuration(format!(
            "Git command {arguments:?} returned an empty value"
        )));
    }
    Ok(value)
}

fn append_fingerprint_part(input: &mut Vec<u8>, part: &[u8]) {
    input.extend_from_slice(&(part.len() as u64).to_le_bytes());
    input.extend_from_slice(part);
}

fn plan_directory(plan_path: &Path) -> &Path {
    plan_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
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
