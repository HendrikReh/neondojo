//! Anki packages, rendered cards, and local AnkiConnect integration.
mod apkg;
pub(super) mod auto_sync;
pub(super) mod connect;
mod deck_export;
pub mod flashcards;
pub(super) mod import;
pub(super) mod review_sync;

pub(super) use deck_export::build_deck_package;

use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub(crate) struct OperationLimits {
    decision_imports: Arc<Semaphore>,
    package_import: Arc<Semaphore>,
    exports: Arc<Semaphore>,
    connect: Arc<Semaphore>,
}

impl Default for OperationLimits {
    fn default() -> Self {
        Self {
            decision_imports: Arc::new(Semaphore::new(2)),
            package_import: Arc::new(Semaphore::new(1)),
            exports: Arc::new(Semaphore::new(2)),
            connect: Arc::new(Semaphore::new(1)),
        }
    }
}
