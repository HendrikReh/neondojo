//! Local analysis adapters. GNUbg remains the independent import/rules engine.
mod context;
mod dispatch;
mod game;
mod process;
mod provider;
pub(crate) mod replay;
mod sage;

pub use dispatch::{AnalysisEngine, EngineCost};
pub(crate) use dispatch::{forced_pass, validate_evaluation};
pub(crate) use game::GAME_TIMEOUT;
#[cfg(all(test, unix))]
pub(crate) use process::tests::{hold_general_worker, unstartable_engine};
pub(crate) use process::{DuelWorkers, VerifiedBatch};
pub(crate) use provider::{EngineId, display_name};
use sage::{CheckerScope, CubeScope, RELEASES_JSON, cube_request};
pub use sage::{
    EngineFork, SUPPORTED_SAGE_PACKAGE_VERSIONS, SageCheckerCandidate, SageCubeDecision,
    SageCubeEvaluation, SageDifficulty, SageEngine, SageSnapshot,
};
pub(crate) use sage::{SageOpponentCube, sage_version_has_cubeful_depth, valid_sage_depth};
