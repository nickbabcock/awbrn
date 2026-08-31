//! Turn-local objectives, missions, and optional decision traces.
//!
//! Objectives and missions are planning values. They do not change the game
//! state. This module defines their identities and validation; callers create
//! them when they build a plan. Board cell indexes stay inside the planning
//! layer. Trace values use coordinates because a cell index has meaning only
//! for its board shape.

use std::collections::{BTreeMap, BTreeSet};

use awvm::semantic::{CellIdx, Dimensions, Location, Observation, ObservedUnitRef, Pos, UnitId};
use awvm::transition::Command;
use serde::{Deserialize, Serialize};

use crate::fingerprint::fnv1a;

/// A value that may be unknown in an observation or trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum Fact<T> {
    Known(T),
    Unknown,
}

/// The harness path that ended one player turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnEndReason {
    /// The agent returned no play.
    AgentPass,
    /// The agent returned an explicit end-turn command.
    ExplicitEndTurn,
    /// The agent returned a play that could not be resolved against the state.
    UnrealizablePlay,
    /// The harness ended the turn after too many rejected offers.
    RefusalLimit,
}

/// Short name for a turn completion reason.
pub type EndReason = TurnEndReason;

/// A turn-local objective identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectiveId(pub u16);

/// A turn-local mission identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MissionId(pub u16);

/// A turn-local objective.
///
/// The cell index must come from the dimensions of the observation that
/// created this value. Use [`Objective::to_trace`] before serialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Objective {
    pub id: ObjectiveId,
    pub kind: ObjectiveKind,
}

impl Objective {
    /// Convert this board-local objective to a coordinate-based trace value.
    pub fn to_trace(&self, dimensions: Dimensions) -> Result<ObjectiveTrace, TraceError> {
        ObjectiveTrace::from_objective(self, dimensions)
    }
}

/// The work an objective asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectiveKind {
    PreventHqLoss { property: CellIdx },
    CompleteCapture { property: CellIdx, unit: UnitId },
    ProtectCapture { property: CellIdx, unit: UnitId },
    CaptureProperty { property: CellIdx },
}

impl ObjectiveKind {
    /// Return the objective property.
    pub const fn property(self) -> CellIdx {
        match self {
            Self::PreventHqLoss { property }
            | Self::CompleteCapture { property, .. }
            | Self::ProtectCapture { property, .. }
            | Self::CaptureProperty { property } => property,
        }
    }

    /// Return the unit named by the objective, when it has one.
    pub const fn unit(self) -> Option<UnitId> {
        match self {
            Self::PreventHqLoss { .. } | Self::CaptureProperty { .. } => None,
            Self::CompleteCapture { unit, .. } | Self::ProtectCapture { unit, .. } => Some(unit),
        }
    }

    /// Return the fixed objective priority.
    pub const fn priority(self) -> ObjectivePriority {
        match self {
            Self::PreventHqLoss { .. } => ObjectivePriority::PreventHqLoss,
            Self::CompleteCapture { .. } => ObjectivePriority::CompleteCapture,
            Self::ProtectCapture { .. } => ObjectivePriority::ProtectCapture,
            Self::CaptureProperty { .. } => ObjectivePriority::CaptureProperty,
        }
    }
}

/// The fixed priority order for objectives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ObjectivePriority {
    PreventHqLoss,
    CompleteCapture,
    ProtectCapture,
    CaptureProperty,
}

/// A mission assigned to one objective.
///
/// The referenced objective and all cells must come from the same turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mission {
    pub id: MissionId,
    pub objective: ObjectiveId,
    pub kind: MissionKind,
}

impl Mission {
    /// Convert this board-local mission to a coordinate-based trace value.
    pub fn to_trace(
        &self,
        objective: &Objective,
        dimensions: Dimensions,
    ) -> Result<MissionTrace, TraceError> {
        MissionTrace::from_mission(self, objective, dimensions)
    }
}

/// The work assigned to one unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MissionKind {
    Capture { unit: UnitId, property: CellIdx },
    Protect { unit: UnitId, beneficiary: UnitId },
}

impl MissionKind {
    /// Return the unit that performs the mission.
    pub const fn unit(self) -> UnitId {
        match self {
            Self::Capture { unit, .. } | Self::Protect { unit, .. } => unit,
        }
    }

    /// Return the beneficiary for a protection mission.
    pub const fn beneficiary(self) -> Option<UnitId> {
        match self {
            Self::Capture { .. } => None,
            Self::Protect { beneficiary, .. } => Some(beneficiary),
        }
    }

    /// Return the capture property for a capture mission.
    pub const fn property(self) -> Option<CellIdx> {
        match self {
            Self::Capture { property, .. } => Some(property),
            Self::Protect { .. } => None,
        }
    }
}

/// A stable, serializable objective kind.
///
/// Unlike [`ObjectiveKind`], this type uses [`Pos`] for every cell and uses a
/// [`Fact`] for every applicable unit identity. `Unknown` is therefore
/// distinct from a field that does not apply to a variant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectiveTraceKind {
    PreventHqLoss { property: Pos },
    CompleteCapture { property: Pos, unit: Fact<UnitId> },
    ProtectCapture { property: Pos, unit: Fact<UnitId> },
    CaptureProperty { property: Pos },
}

/// A serializable objective identity and kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveTrace {
    pub id: ObjectiveId,
    pub kind: ObjectiveTraceKind,
}

impl ObjectiveTrace {
    /// Convert a board-local objective to a coordinate-based trace value.
    pub fn from_objective(
        objective: &Objective,
        dimensions: Dimensions,
    ) -> Result<Self, TraceError> {
        let property = position_of(dimensions, objective.kind.property())?;
        let kind = match objective.kind {
            ObjectiveKind::PreventHqLoss { .. } => Self::kind_prevent(property),
            ObjectiveKind::CompleteCapture { unit, .. } => ObjectiveTraceKind::CompleteCapture {
                property,
                unit: Fact::Known(unit),
            },
            ObjectiveKind::ProtectCapture { unit, .. } => ObjectiveTraceKind::ProtectCapture {
                property,
                unit: Fact::Known(unit),
            },
            ObjectiveKind::CaptureProperty { .. } => {
                ObjectiveTraceKind::CaptureProperty { property }
            }
        };
        Ok(Self {
            id: objective.id,
            kind,
        })
    }

    const fn kind_prevent(property: Pos) -> ObjectiveTraceKind {
        ObjectiveTraceKind::PreventHqLoss { property }
    }

    /// Return the property named by this trace value.
    pub const fn property(&self) -> Pos {
        match self.kind {
            ObjectiveTraceKind::PreventHqLoss { property }
            | ObjectiveTraceKind::CompleteCapture { property, .. }
            | ObjectiveTraceKind::ProtectCapture { property, .. }
            | ObjectiveTraceKind::CaptureProperty { property } => property,
        }
    }

    /// Return the fixed priority of this trace objective.
    pub const fn priority(&self) -> ObjectivePriority {
        match self.kind {
            ObjectiveTraceKind::PreventHqLoss { .. } => ObjectivePriority::PreventHqLoss,
            ObjectiveTraceKind::CompleteCapture { .. } => ObjectivePriority::CompleteCapture,
            ObjectiveTraceKind::ProtectCapture { .. } => ObjectivePriority::ProtectCapture,
            ObjectiveTraceKind::CaptureProperty { .. } => ObjectivePriority::CaptureProperty,
        }
    }
}

/// A stable, serializable mission kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MissionTraceKind {
    Capture {
        unit: Fact<UnitId>,
    },
    Protect {
        unit: Fact<UnitId>,
        beneficiary: Fact<UnitId>,
    },
}

/// A serializable mission identity, parent, kind, and concrete target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionTrace {
    pub id: MissionId,
    pub objective: ObjectiveId,
    pub target: Pos,
    pub kind: MissionTraceKind,
}

impl MissionTrace {
    /// Convert a mission to a coordinate-based trace value.
    pub fn from_mission(
        mission: &Mission,
        objective: &Objective,
        dimensions: Dimensions,
    ) -> Result<Self, TraceError> {
        if mission.objective != objective.id {
            return Err(TraceError::ObjectiveMismatch {
                mission: mission.id,
                expected: objective.id,
                found: mission.objective,
            });
        }
        let target = position_of(dimensions, objective.kind.property())?;
        let kind = match mission.kind {
            MissionKind::Capture { property, unit } => {
                let mission_property = position_of(dimensions, property)?;
                if mission_property != target {
                    return Err(TraceError::MissionTargetMismatch {
                        mission: mission.id,
                        objective: objective.id,
                    });
                }
                MissionTraceKind::Capture {
                    unit: Fact::Known(unit),
                }
            }
            MissionKind::Protect { unit, beneficiary } => MissionTraceKind::Protect {
                unit: Fact::Known(unit),
                beneficiary: Fact::Known(beneficiary),
            },
        };
        Ok(Self {
            id: mission.id,
            objective: mission.objective,
            target,
            kind,
        })
    }
}

/// An ordinary reason for assigning a mission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum AssignmentReason {
    ImmediateHqThreat,
    ContinueActiveCapture,
    ProtectCredibleInterruption,
    DurablePropertyControl,
    DivertHostileForce,
}

/// A reason why an objective or mission was not assigned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum RejectionReason {
    UnitAlreadyCommitted { mission: MissionId },
    NoEligibleUnit,
    NoLegalPath,
    NoCredibleInterrupter,
    OutrankedBy { objective: ObjectiveId },
    Unknown { detail: String },
}

/// The result of one assigned mission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", content = "reason", rename_all = "kebab-case")]
pub enum MissionOutcome {
    Completed,
    Interrupted,
    Rejected(RejectionReason),
    Unattempted,
}

/// One typed advisory trace record.
///
/// The event order is the order in which the decision layer produced the
/// records. Canonical objective ordering is provided by
/// [`canonicalize_objectives`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "trace_kind", rename_all = "snake_case")]
pub enum DecisionTraceEvent {
    ObjectiveGenerated {
        objective: ObjectiveTrace,
    },
    EligibleUnits {
        objective: ObjectiveTrace,
        units: Vec<Fact<UnitId>>,
    },
    ObjectiveAssignment {
        objective: ObjectiveTrace,
        #[serde(skip_serializing_if = "Option::is_none")]
        mission: Option<MissionTrace>,
        reason: AssignmentReason,
    },
    ObjectiveRejection {
        objective: ObjectiveTrace,
        reason: RejectionReason,
    },
    ObjectiveDisplacement {
        objective: ObjectiveTrace,
        displaced_by: ObjectiveId,
    },
    ObjectiveCompletion {
        objective: ObjectiveTrace,
    },
    ObjectiveAbandonment {
        objective: ObjectiveTrace,
        reason: RejectionReason,
    },
    MissionAssignment {
        mission: MissionTrace,
        reason: AssignmentReason,
    },
    MissionRejection {
        mission: MissionTrace,
        reason: RejectionReason,
    },
    MissionCommand {
        mission: MissionTrace,
        command: Command,
    },
    MissionCompletion {
        mission: MissionTrace,
        outcome: MissionOutcome,
    },
    MissionAbandonment {
        mission: MissionTrace,
        outcome: MissionOutcome,
    },
    TraceTruncated {
        omitted_records: u64,
    },
}

/// Short name for one structured decision trace record.
pub type TraceEvent = DecisionTraceEvent;

impl DecisionTraceEvent {
    /// Return true for details that may be dropped at the per-turn cap.
    pub const fn is_optional_detail(&self) -> bool {
        matches!(
            self,
            Self::EligibleUnits { .. } | Self::MissionCommand { .. }
        )
    }

    fn objective(&self) -> Option<&ObjectiveTrace> {
        match self {
            Self::ObjectiveGenerated { objective }
            | Self::EligibleUnits { objective, .. }
            | Self::ObjectiveAssignment { objective, .. }
            | Self::ObjectiveRejection { objective, .. }
            | Self::ObjectiveDisplacement { objective, .. }
            | Self::ObjectiveCompletion { objective }
            | Self::ObjectiveAbandonment { objective, .. } => Some(objective),
            Self::MissionAssignment { .. }
            | Self::MissionRejection { .. }
            | Self::MissionCommand { .. }
            | Self::MissionCompletion { .. }
            | Self::MissionAbandonment { .. }
            | Self::TraceTruncated { .. } => None,
        }
    }

    fn mission(&self) -> Option<&MissionTrace> {
        match self {
            Self::ObjectiveGenerated { .. }
            | Self::EligibleUnits { .. }
            | Self::ObjectiveAssignment { .. }
            | Self::ObjectiveRejection { .. }
            | Self::ObjectiveDisplacement { .. }
            | Self::ObjectiveCompletion { .. }
            | Self::ObjectiveAbandonment { .. }
            | Self::TraceTruncated { .. } => None,
            Self::MissionAssignment { mission, .. }
            | Self::MissionRejection { mission, .. }
            | Self::MissionCommand { mission, .. }
            | Self::MissionCompletion { mission, .. }
            | Self::MissionAbandonment { mission, .. } => Some(mission),
        }
    }
}

/// The maximum optional detail records kept for one player turn.
pub const MAX_TRACE_DETAIL_RECORDS: usize = 1_024;

/// An optional, typed decision trace.
///
/// Required identity and outcome records are retained beyond the detail cap.
/// The normal strategic agent stores no instance of this type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DecisionTrace {
    records: Vec<DecisionTraceEvent>,
    objectives: BTreeMap<ObjectiveId, ObjectiveTrace>,
    missions: BTreeMap<MissionId, MissionTrace>,
    assigned: BTreeSet<MissionId>,
    outcomes: BTreeSet<MissionId>,
    assigned_units: BTreeMap<UnitId, MissionId>,
    detail_records: usize,
    omitted_records: u64,
    finalized: Option<TurnEndReason>,
}

impl DecisionTrace {
    /// Build an empty enabled trace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a typed record.
    ///
    /// Optional detail is capped. Required identity and outcome records are
    /// always retained.
    pub fn record(&mut self, event: DecisionTraceEvent) -> Result<(), TraceError> {
        if self.finalized.is_some() {
            return Err(TraceError::TraceFinalized);
        }
        self.validate_event(&event)?;
        if event.is_optional_detail() && self.detail_records >= MAX_TRACE_DETAIL_RECORDS {
            self.omitted_records = self.omitted_records.saturating_add(1);
            self.update_truncation_record();
            return Ok(());
        }
        if event.is_optional_detail() {
            self.detail_records += 1;
        }
        self.apply_event(&event);
        self.records.push(event);
        Ok(())
    }

    /// Return retained records in deterministic insertion order.
    pub fn records(&self) -> &[DecisionTraceEvent] {
        &self.records
    }

    /// Remove all records and reuse the trace allocations for the next turn.
    pub fn clear(&mut self) {
        self.records.clear();
        self.objectives.clear();
        self.missions.clear();
        self.assigned.clear();
        self.outcomes.clear();
        self.assigned_units.clear();
        self.detail_records = 0;
        self.omitted_records = 0;
        self.finalized = None;
    }

    /// Return the number of optional records omitted by the cap.
    pub const fn omitted_records(&self) -> u64 {
        self.omitted_records
    }

    /// Return the reason that finalized this trace.
    pub const fn end_reason(&self) -> Option<TurnEndReason> {
        self.finalized
    }

    /// Return true when the trace is finalized.
    pub const fn is_finalized(&self) -> bool {
        self.finalized.is_some()
    }

    /// Finalize the trace and add an outcome for each unresolved mission.
    pub fn finalize(&mut self, reason: TurnEndReason) -> Result<(), TraceError> {
        if self.finalized.is_some() {
            return Ok(());
        }
        let missions: Vec<_> = self
            .assigned
            .iter()
            .filter(|mission| !self.outcomes.contains(mission))
            .filter_map(|mission| self.missions.get(mission).cloned())
            .collect();
        for mission in missions {
            self.record(DecisionTraceEvent::MissionAbandonment {
                mission,
                outcome: MissionOutcome::Unattempted,
            })?;
        }
        self.finalized = Some(reason);
        Ok(())
    }

    /// Check that every assigned mission has one outcome.
    pub fn validate_complete(&self) -> Result<(), TraceError> {
        let missing: Vec<_> = self
            .assigned
            .iter()
            .filter(|mission| !self.outcomes.contains(mission))
            .copied()
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(TraceError::MissingMissionOutcomes { missions: missing })
        }
    }

    /// Validate trace cells and unit identities against an observation.
    ///
    /// This check is separate from [`DecisionTrace::validate_complete`] so a
    /// trace can be assembled before the observation is available.
    pub fn validate_observation(&self, observation: &Observation) -> Result<(), TraceError> {
        self.validate_complete()?;
        let dimensions = observation_dimensions(observation);
        for objective in self.objectives.values() {
            if !dimensions.contains(objective.property()) {
                return Err(TraceError::InvalidTraceTarget {
                    target: objective.property(),
                });
            }
            if let Some(unit) = objective_unit(objective)?
                && !observed_unit(observation, unit)
            {
                return Err(TraceError::UnitNotObserved { unit });
            }
        }
        for mission in self.missions.values() {
            if !dimensions.contains(mission.target) {
                return Err(TraceError::InvalidTraceTarget {
                    target: mission.target,
                });
            }
            if !self.objectives.contains_key(&mission.objective) {
                return Err(TraceError::UnknownObjective {
                    objective: mission.objective,
                });
            }
            if self
                .objectives
                .get(&mission.objective)
                .is_some_and(|objective| objective.property() != mission.target)
            {
                return Err(TraceError::MissionTargetMismatch {
                    mission: mission.id,
                    objective: mission.objective,
                });
            }
            let unit = known_mission_unit(mission).ok_or(TraceError::UnknownMissionUnit {
                mission: mission.id,
            })?;
            if !legal_observed_unit(observation, unit) {
                return Err(TraceError::UnitNotObserved { unit });
            }
            if let MissionTraceKind::Protect { beneficiary, .. } = &mission.kind {
                let beneficiary = match beneficiary {
                    Fact::Known(beneficiary) => *beneficiary,
                    Fact::Unknown => {
                        return Err(TraceError::UnknownMissionUnit {
                            mission: mission.id,
                        });
                    }
                };
                if unit == beneficiary {
                    return Err(TraceError::SelfProtection {
                        mission: mission.id,
                    });
                }
                if !legal_observed_unit(observation, beneficiary) {
                    return Err(TraceError::UnitNotObserved { unit: beneficiary });
                }
            }
        }
        Ok(())
    }

    /// Return a deterministic fingerprint of the retained trace.
    pub fn fingerprint(&self) -> u64 {
        let bytes = serde_json::to_vec(&self.records).expect("decision trace serializes");
        fnv1a(&bytes)
    }

    fn validate_event(&self, event: &DecisionTraceEvent) -> Result<(), TraceError> {
        if matches!(event, DecisionTraceEvent::TraceTruncated { .. }) {
            return Err(TraceError::ManagedTruncationRecord);
        }

        if let Some(objective) = event.objective() {
            self.validate_objective_identity(objective)?;
        }
        if let Some(mission) = event.mission() {
            self.validate_mission_identity(mission)?;
            if !self.objectives.contains_key(&mission.objective) {
                return Err(TraceError::UnknownObjective {
                    objective: mission.objective,
                });
            }
        }

        match event {
            DecisionTraceEvent::ObjectiveAssignment {
                mission, objective, ..
            } => {
                if let Some(mission) = mission
                    && mission.objective != objective.id
                {
                    return Err(TraceError::ObjectiveMissionMismatch {
                        objective: objective.id,
                        mission: mission.id,
                    });
                }
            }
            DecisionTraceEvent::ObjectiveDisplacement {
                objective,
                displaced_by,
            } => {
                let Some(displaced) = self.objectives.get(displaced_by) else {
                    return Err(TraceError::UnknownObjective {
                        objective: *displaced_by,
                    });
                };
                if displaced.priority() >= objective.priority() {
                    return Err(TraceError::InvalidDisplacement {
                        objective: objective.id,
                        displaced_by: *displaced_by,
                    });
                }
            }
            DecisionTraceEvent::MissionAssignment { mission, .. } => {
                if self.assigned.contains(&mission.id) {
                    return Err(TraceError::DuplicateMission {
                        mission: mission.id,
                    });
                }
                let Some(unit) = known_mission_unit(mission) else {
                    return Err(TraceError::UnknownMissionUnit {
                        mission: mission.id,
                    });
                };
                if let Some(existing) = self.assigned_units.get(&unit) {
                    return Err(TraceError::UnitAlreadyAssigned {
                        unit,
                        mission: *existing,
                    });
                }
                if let MissionTraceKind::Protect { unit, beneficiary } = &mission.kind
                    && let (Fact::Known(unit), Fact::Known(beneficiary)) = (unit, beneficiary)
                    && unit == beneficiary
                {
                    return Err(TraceError::SelfProtection {
                        mission: mission.id,
                    });
                }
            }
            DecisionTraceEvent::MissionCompletion { mission, .. }
            | DecisionTraceEvent::MissionAbandonment { mission, .. } => {
                if !self.assigned.contains(&mission.id) {
                    return Err(TraceError::OutcomeWithoutAssignment {
                        mission: mission.id,
                    });
                }
                if self.outcomes.contains(&mission.id) {
                    return Err(TraceError::DuplicateMissionOutcome {
                        mission: mission.id,
                    });
                }
            }
            DecisionTraceEvent::MissionCommand { mission, .. } => {
                if !self.assigned.contains(&mission.id) {
                    return Err(TraceError::CommandWithoutAssignment {
                        mission: mission.id,
                    });
                }
            }
            DecisionTraceEvent::ObjectiveGenerated { .. }
            | DecisionTraceEvent::EligibleUnits { .. }
            | DecisionTraceEvent::ObjectiveRejection { .. }
            | DecisionTraceEvent::ObjectiveCompletion { .. }
            | DecisionTraceEvent::ObjectiveAbandonment { .. }
            | DecisionTraceEvent::MissionRejection { .. }
            | DecisionTraceEvent::TraceTruncated { .. } => {}
        }
        Ok(())
    }

    fn validate_objective_identity(&self, objective: &ObjectiveTrace) -> Result<(), TraceError> {
        if let Some(existing) = self.objectives.get(&objective.id)
            && existing != objective
        {
            return Err(TraceError::ConflictingObjective {
                objective: objective.id,
            });
        }
        Ok(())
    }

    fn validate_mission_identity(&self, mission: &MissionTrace) -> Result<(), TraceError> {
        if let Some(existing) = self.missions.get(&mission.id)
            && existing != mission
        {
            return Err(TraceError::ConflictingMission {
                mission: mission.id,
            });
        }
        Ok(())
    }

    fn apply_event(&mut self, event: &DecisionTraceEvent) {
        if let Some(objective) = event.objective() {
            self.objectives.insert(objective.id, objective.clone());
        }
        if let Some(mission) = event.mission() {
            self.missions.insert(mission.id, mission.clone());
        }
        if let DecisionTraceEvent::ObjectiveAssignment {
            mission: Some(mission),
            ..
        } = event
        {
            self.missions.insert(mission.id, mission.clone());
        }
        match event {
            DecisionTraceEvent::MissionAssignment { mission, .. } => {
                self.assigned.insert(mission.id);
                if let Some(unit) = known_mission_unit(mission) {
                    self.assigned_units.insert(unit, mission.id);
                }
            }
            DecisionTraceEvent::MissionCompletion { mission, .. }
            | DecisionTraceEvent::MissionAbandonment { mission, .. } => {
                self.outcomes.insert(mission.id);
            }
            _ => {}
        }
    }

    fn update_truncation_record(&mut self) {
        if let Some(DecisionTraceEvent::TraceTruncated { omitted_records }) = self
            .records
            .iter_mut()
            .find(|event| matches!(event, DecisionTraceEvent::TraceTruncated { .. }))
        {
            *omitted_records = self.omitted_records;
        } else {
            self.records.push(DecisionTraceEvent::TraceTruncated {
                omitted_records: self.omitted_records,
            });
        }
    }
}

/// Errors from objective, mission, and trace validation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TraceError {
    #[error("cell {cell} is outside the {width}x{height} board")]
    InvalidCell { cell: u16, width: u8, height: u8 },
    #[error("trace target {target} is outside the observation board")]
    InvalidTraceTarget { target: Pos },
    #[error(
        "mission {mission:?} refers to objective {expected:?}, but the mission stores {found:?}"
    )]
    ObjectiveMismatch {
        mission: MissionId,
        expected: ObjectiveId,
        found: ObjectiveId,
    },
    #[error("mission {mission:?} does not target its objective {objective:?}")]
    MissionTargetMismatch {
        mission: MissionId,
        objective: ObjectiveId,
    },
    #[error("mission {mission:?} does not use the unit named by objective {objective:?}")]
    MissionUnitMismatch {
        mission: MissionId,
        objective: ObjectiveId,
    },
    #[error("objective {objective:?} has conflicting trace data")]
    ConflictingObjective { objective: ObjectiveId },
    #[error("mission {mission:?} has conflicting trace data")]
    ConflictingMission { mission: MissionId },
    #[error("mission {mission:?} has no known unit identity")]
    UnknownMissionUnit { mission: MissionId },
    #[error("objective {objective:?} is not present in the trace")]
    UnknownObjective { objective: ObjectiveId },
    #[error("objective {objective:?} has no known unit identity")]
    UnknownObjectiveUnit { objective: ObjectiveId },
    #[error("objective {objective:?} and mission {mission:?} do not join")]
    ObjectiveMissionMismatch {
        objective: ObjectiveId,
        mission: MissionId,
    },
    #[error(
        "objective {objective:?} was displaced by non-higher-priority objective {displaced_by:?}"
    )]
    InvalidDisplacement {
        objective: ObjectiveId,
        displaced_by: ObjectiveId,
    },
    #[error("mission {mission:?} is assigned more than once")]
    DuplicateMission { mission: MissionId },
    #[error("unit {unit} already has mission {mission:?}")]
    UnitAlreadyAssigned { unit: UnitId, mission: MissionId },
    #[error("mission {mission:?} protects itself")]
    SelfProtection { mission: MissionId },
    #[error("mission {mission:?} has an outcome without an assignment")]
    OutcomeWithoutAssignment { mission: MissionId },
    #[error("mission {mission:?} has more than one outcome")]
    DuplicateMissionOutcome { mission: MissionId },
    #[error("mission {mission:?} has a command without an assignment")]
    CommandWithoutAssignment { mission: MissionId },
    #[error("trace truncation records are managed by DecisionTrace")]
    ManagedTruncationRecord,
    #[error("decision trace is finalized")]
    TraceFinalized,
    #[error("assigned missions have no outcome: {missions:?}")]
    MissingMissionOutcomes { missions: Vec<MissionId> },
    #[error("objective {objective:?} is duplicated")]
    DuplicateObjective { objective: ObjectiveId },
    #[error("mission {mission:?} is duplicated")]
    DuplicateMissionIdentity { mission: MissionId },
    #[error("mission {mission:?} refers to an unknown objective {objective:?}")]
    MissionObjectiveMissing {
        mission: MissionId,
        objective: ObjectiveId,
    },
    #[error("unit {unit} is not a legal observed unit")]
    UnitNotObserved { unit: UnitId },
    #[error("mission {mission:?} uses unit {unit}, which already has a mission")]
    MissionUnitConflict { mission: MissionId, unit: UnitId },
    #[error("mission {mission:?} protects beneficiary {beneficiary}, which is also its protector")]
    MissionProtectsItself {
        mission: MissionId,
        beneficiary: UnitId,
    },
}

/// Sort, deduplicate, and assign turn-local objective identifiers.
pub fn canonicalize_objectives(objectives: impl IntoIterator<Item = Objective>) -> Vec<Objective> {
    let mut objectives: Vec<_> = objectives.into_iter().collect();
    objectives.sort_by_key(|objective| {
        (
            objective.kind.priority(),
            objective.kind.property(),
            objective.kind.unit(),
        )
    });
    objectives.dedup_by(|left, right| left.kind == right.kind);
    for (index, objective) in objectives.iter_mut().enumerate() {
        objective.id = ObjectiveId(
            u16::try_from(index).expect("a turn cannot have more than 65,536 objectives"),
        );
    }
    objectives
}

/// Validate objective and mission identity, unit, target, and uniqueness rules.
pub fn validate_missions(
    objectives: &[Objective],
    missions: &[Mission],
    observation: &Observation,
) -> Result<(), TraceError> {
    let mut objective_ids = BTreeSet::new();
    for objective in objectives {
        if !objective_ids.insert(objective.id) {
            return Err(TraceError::DuplicateObjective {
                objective: objective.id,
            });
        }
        validate_objective(objective, observation)?;
    }

    let mut mission_ids = BTreeSet::new();
    let mut units = BTreeSet::new();
    for mission in missions {
        if !mission_ids.insert(mission.id) {
            return Err(TraceError::DuplicateMissionIdentity {
                mission: mission.id,
            });
        }
        let Some(objective) = objectives
            .iter()
            .find(|candidate| candidate.id == mission.objective)
        else {
            return Err(TraceError::MissionObjectiveMissing {
                mission: mission.id,
                objective: mission.objective,
            });
        };
        let unit = mission.kind.unit();
        if !units.insert(unit) {
            return Err(TraceError::MissionUnitConflict {
                mission: mission.id,
                unit,
            });
        }
        if !legal_observed_unit(observation, unit) {
            return Err(TraceError::UnitNotObserved { unit });
        }
        match mission.kind {
            MissionKind::Capture { property, .. } => {
                if property != objective.kind.property()
                    || position_of(observation_dimensions(observation), property).is_err()
                {
                    return Err(TraceError::MissionTargetMismatch {
                        mission: mission.id,
                        objective: objective.id,
                    });
                }
                if let ObjectiveKind::CompleteCapture {
                    unit: objective_unit,
                    ..
                } = objective.kind
                    && mission.kind.unit() != objective_unit
                {
                    return Err(TraceError::MissionUnitMismatch {
                        mission: mission.id,
                        objective: objective.id,
                    });
                }
            }
            MissionKind::Protect { beneficiary, .. } => {
                if unit == beneficiary {
                    return Err(TraceError::MissionProtectsItself {
                        mission: mission.id,
                        beneficiary,
                    });
                }
                if !legal_observed_unit(observation, beneficiary) {
                    return Err(TraceError::UnitNotObserved { unit: beneficiary });
                }
                if let ObjectiveKind::ProtectCapture {
                    unit: objective_unit,
                    ..
                } = objective.kind
                    && beneficiary != objective_unit
                {
                    return Err(TraceError::MissionUnitMismatch {
                        mission: mission.id,
                        objective: objective.id,
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_objective(objective: &Objective, observation: &Observation) -> Result<(), TraceError> {
    position_of(
        observation_dimensions(observation),
        objective.kind.property(),
    )?;
    if let Some(unit) = objective.kind.unit()
        && !observed_unit(observation, unit)
    {
        return Err(TraceError::UnitNotObserved { unit });
    }
    Ok(())
}

fn observed_unit(observation: &Observation, unit: UnitId) -> bool {
    observation.units.iter().any(|observed| {
        matches!(
            observed.reference,
            ObservedUnitRef::Friendly { unit: observed_unit } if observed_unit == unit
        ) && observed.owner == observation.recipient
    })
}

fn observation_dimensions(observation: &Observation) -> Dimensions {
    Dimensions::new(observation.board.width(), observation.board.height())
}

fn legal_observed_unit(observation: &Observation, unit: UnitId) -> bool {
    observation.units.iter().any(|observed| {
        matches!(
            observed.reference,
            ObservedUnitRef::Friendly { unit: observed_unit } if observed_unit == unit
        ) && observed.owner == observation.recipient
            && matches!(observed.location, Location::Board { .. })
    })
}

fn known_mission_unit(mission: &MissionTrace) -> Option<UnitId> {
    match &mission.kind {
        MissionTraceKind::Capture { unit } | MissionTraceKind::Protect { unit, .. } => match unit {
            Fact::Known(unit) => Some(*unit),
            Fact::Unknown => None,
        },
    }
}

fn objective_unit(objective: &ObjectiveTrace) -> Result<Option<UnitId>, TraceError> {
    match &objective.kind {
        ObjectiveTraceKind::PreventHqLoss { .. } | ObjectiveTraceKind::CaptureProperty { .. } => {
            Ok(None)
        }
        ObjectiveTraceKind::CompleteCapture { unit, .. }
        | ObjectiveTraceKind::ProtectCapture { unit, .. } => match unit {
            Fact::Known(unit) => Ok(Some(*unit)),
            Fact::Unknown => Err(TraceError::UnknownObjectiveUnit {
                objective: objective.id,
            }),
        },
    }
}

fn position_of(dimensions: Dimensions, cell: CellIdx) -> Result<Pos, TraceError> {
    dimensions.position_of(cell).ok_or(TraceError::InvalidCell {
        cell: cell.get(),
        width: dimensions.width(),
        height: dimensions.height(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::arena;
    use awvm::semantic::{AwbwVisibility, observe};

    fn objective(kind: ObjectiveKind) -> Objective {
        Objective {
            id: ObjectiveId(u16::MAX),
            kind,
        }
    }

    #[test]
    fn canonical_objectives_are_sorted_deduplicated_and_reindexed() {
        let property = CellIdx::from_raw(4);
        let objectives = canonicalize_objectives([
            objective(ObjectiveKind::CaptureProperty { property }),
            objective(ObjectiveKind::PreventHqLoss { property }),
            objective(ObjectiveKind::CaptureProperty { property }),
        ]);

        assert_eq!(objectives.len(), 2);
        assert_eq!(objectives[0].id, ObjectiveId(0));
        assert_eq!(objectives[1].id, ObjectiveId(1));
        assert!(matches!(
            objectives[0].kind,
            ObjectiveKind::PreventHqLoss { .. }
        ));
    }

    #[test]
    fn trace_preserves_required_records_when_optional_detail_is_capped() {
        let dimensions = Dimensions::new(4, 4);
        let objective = objective(ObjectiveKind::CaptureProperty {
            property: dimensions.cell_index(Pos::new(1, 1)).expect("cell"),
        });
        let trace_objective =
            ObjectiveTrace::from_objective(&objective, dimensions).expect("trace");
        let mut trace = DecisionTrace::new();
        trace
            .record(DecisionTraceEvent::ObjectiveGenerated {
                objective: trace_objective.clone(),
            })
            .expect("objective record");
        for _ in 0..=MAX_TRACE_DETAIL_RECORDS {
            trace
                .record(DecisionTraceEvent::EligibleUnits {
                    objective: trace_objective.clone(),
                    units: vec![Fact::Unknown],
                })
                .expect("optional record");
        }

        assert_eq!(trace.omitted_records(), 1);
        assert_eq!(
            trace
                .records()
                .iter()
                .filter(|event| matches!(event, DecisionTraceEvent::EligibleUnits { .. }))
                .count(),
            MAX_TRACE_DETAIL_RECORDS
        );
        assert!(matches!(
            trace.records().last(),
            Some(DecisionTraceEvent::TraceTruncated { omitted_records: 1 })
        ));
    }

    #[test]
    fn assigned_mission_requires_one_outcome_and_has_a_stable_fingerprint() {
        let dimensions = Dimensions::new(4, 4);
        let property = dimensions.cell_index(Pos::new(1, 1)).expect("cell");
        let objective = Objective {
            id: ObjectiveId(0),
            kind: ObjectiveKind::CaptureProperty { property },
        };
        let mission = Mission {
            id: MissionId(0),
            objective: objective.id,
            kind: MissionKind::Capture {
                unit: UnitId::new(7),
                property,
            },
        };
        let trace_objective =
            ObjectiveTrace::from_objective(&objective, dimensions).expect("objective trace");
        let trace_mission =
            MissionTrace::from_mission(&mission, &objective, dimensions).expect("mission trace");
        let mut trace = DecisionTrace::new();
        trace
            .record(DecisionTraceEvent::ObjectiveGenerated {
                objective: trace_objective.clone(),
            })
            .expect("objective");
        trace
            .record(DecisionTraceEvent::MissionAssignment {
                mission: trace_mission.clone(),
                reason: AssignmentReason::DurablePropertyControl,
            })
            .expect("assignment");
        assert!(trace.validate_complete().is_err());
        trace
            .record(DecisionTraceEvent::MissionCompletion {
                mission: trace_mission,
                outcome: MissionOutcome::Completed,
            })
            .expect("outcome");
        trace
            .validate_complete()
            .expect("all assigned missions have outcomes");
        assert_eq!(trace.fingerprint(), 11_503_255_697_506_146_354);
    }

    #[test]
    fn mission_validation_rejects_duplicate_units_and_self_protection() {
        let state = arena(false, 1);
        let second_player = state
            .players
            .seats()
            .nth(1)
            .expect("second seat")
            .1
            .id()
            .clone();
        let view = observe(&AwbwVisibility, &state, &second_player).expect("observation");
        let units: Vec<_> = view
            .units
            .iter()
            .filter_map(|unit| match unit.reference {
                ObservedUnitRef::Friendly { unit } => Some(unit),
                ObservedUnitRef::Enemy { .. } => None,
            })
            .take(1)
            .collect();
        let property = Dimensions::new(view.board.width(), view.board.height())
            .cell_index(Pos::new(0, 0))
            .expect("cell");
        let objective = Objective {
            id: ObjectiveId(0),
            kind: ObjectiveKind::CaptureProperty { property },
        };
        let capture = Mission {
            id: MissionId(9),
            objective: objective.id,
            kind: MissionKind::Capture {
                unit: units[0],
                property,
            },
        };
        let trace_objective = objective
            .to_trace(Dimensions::new(view.board.width(), view.board.height()))
            .expect("objective trace");
        let trace_mission = capture
            .to_trace(
                &objective,
                Dimensions::new(view.board.width(), view.board.height()),
            )
            .expect("mission trace");
        let mut trace = DecisionTrace::new();
        trace
            .record(DecisionTraceEvent::ObjectiveGenerated {
                objective: trace_objective,
            })
            .expect("objective record");
        trace
            .record(DecisionTraceEvent::MissionAssignment {
                mission: trace_mission.clone(),
                reason: AssignmentReason::DurablePropertyControl,
            })
            .expect("mission record");
        trace
            .record(DecisionTraceEvent::MissionCompletion {
                mission: trace_mission,
                outcome: MissionOutcome::Completed,
            })
            .expect("outcome record");
        trace
            .validate_observation(&view)
            .expect("trace names observed legal units and board cells");
        let duplicate = Mission {
            id: MissionId(0),
            objective: objective.id,
            kind: MissionKind::Capture {
                unit: units[0],
                property,
            },
        };
        let duplicate_again = Mission {
            id: MissionId(1),
            objective: objective.id,
            kind: MissionKind::Capture {
                unit: units[0],
                property,
            },
        };
        assert!(matches!(
            validate_missions(&[objective], &[duplicate, duplicate_again], &view),
            Err(TraceError::MissionUnitConflict { .. })
        ));
        let self_protection = Mission {
            id: MissionId(2),
            objective: objective.id,
            kind: MissionKind::Protect {
                unit: units[0],
                beneficiary: units[0],
            },
        };
        assert!(matches!(
            validate_missions(&[objective], &[self_protection], &view),
            Err(TraceError::MissionProtectsItself { .. })
        ));
    }
}
