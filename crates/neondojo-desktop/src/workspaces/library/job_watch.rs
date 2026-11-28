//! Project committed backend progress into the current Library page.
use super::LibraryWorkspace;
use crate::tokio_bridge::{ReadTask, TokioBridge};
use gpui_kit::{Context, Task};
use neondojo::facade::{LibraryJobScope, LibraryJobSnapshot};
use std::{collections::BTreeSet, sync::Arc};
use tokio::sync::mpsc;

pub(super) struct LibraryJobWatch {
    scope: Arc<LibraryJobScope>,
    _reader: ReadTask<()>,
    _delivery: Task<()>,
}

impl LibraryWorkspace {
    pub(super) fn sync_job_watcher(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone().filter(|_| self.visible) else {
            self.job_watch = None;
            return;
        };
        let scope = Arc::new(LibraryJobScope {
            rows: self
                .matches
                .iter()
                .map(|m| m.id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            bulk: self.bulk_tracker.active_job_ids(),
            pending: self
                .matches
                .iter()
                .any(|m| matches!(m.analysis_status.as_deref(), Some("queued" | "running"))),
        });
        if scope.rows.is_empty() && scope.bulk.is_empty() {
            self.job_watch = None;
            return;
        }
        if self
            .job_watch
            .as_ref()
            .is_some_and(|watch| watch.scope == scope)
        {
            return;
        }

        let (tx, mut rx) = mpsc::channel(1);
        let watched = scope.clone();
        let reader =
            TokioBridge::spawn_read(
                cx,
                async move { facade.follow_library_jobs(&watched, tx).await },
            );
        let delivery_scope = scope.clone();
        let delivery = cx.spawn(async move |this, cx| {
            while let Some(snapshot) = rx.recv().await {
                if this
                    .update(cx, |ws, cx| {
                        if ws
                            .job_watch
                            .as_ref()
                            .is_some_and(|w| Arc::ptr_eq(&w.scope, &delivery_scope))
                        {
                            ws.apply_job_snapshot(snapshot, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.job_watch = Some(LibraryJobWatch {
            scope,
            _reader: reader,
            _delivery: delivery,
        });
    }

    fn apply_job_snapshot(&mut self, snapshot: LibraryJobSnapshot, cx: &mut Context<Self>) {
        let mut reload = snapshot.missing_row
            || snapshot.rows.iter().any(|(id, status)| {
                self.matches
                    .iter()
                    .any(|m| m.id == *id && m.analysis_status != *status)
            });
        let (changed, finished) = self.bulk_tracker.apply_batch(snapshot.bulk);
        if finished {
            reload = true;
            self.sync_job_watcher(cx);
        }
        if reload {
            if self.loading_matches {
                // Let the in-flight query finish, then cover any later commit.
                self.pending_job_refresh = true;
            } else {
                self.load_matches(cx);
            }
        }
        if changed {
            cx.notify();
        }
    }
}
