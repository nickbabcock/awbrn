//! Opt-in rendering and offline review tools for headless AI runs.

pub mod capture;
pub mod events;
pub mod manifest;
pub mod map_registry;
pub mod pipeline;
pub mod review;
pub mod tournament;
pub mod verify;

pub use capture::{VisualCapture, VisualCaptureIdentity};
pub use events::{
    EventKind, EventLogError, EventLogWriter, EventMetadata, EventRow, MatchEventRow,
    ReanalysisSummary, observations_from_event_log, read_event_log, reanalyse_event_log,
    reanalyse_event_log_with_manifest, row_for_state, verify_expected_fingerprints,
    write_derived_outputs,
};
pub use manifest::{
    ManifestError, read_manifest, resolve_event_log_path, write_manifest,
    write_or_validate_manifest,
};
pub use map_registry::{
    CANONICAL_SEATS, MapManifest, MapManifestEntry, MapRegistry, MapRegistryError, RegisteredMap,
};
pub use pipeline::{DiagnosticError, DiagnosticSummary, run_diagnostic};
pub use review::{ReviewError, ReviewSummary, run_review, run_review_with_tilesets};
pub use tournament::{
    AgentFactory, STRATEGIC_EXECUTABLE_FINGERPRINT, StrategicFactory, TournamentError,
    TournamentSummary, run_manifest, run_paired_tournament,
};
pub use verify::{VerificationSummary, VerifyError, verify_artifact};
