use super::{EngineCost, SageSnapshot};
use crate::engine_process::{reap_child, terminate_and_reap};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{Mutex, MutexGuard},
};

const MAX_REQUEST: usize = 262_144;
const MAX_RESPONSE: u64 = 2_097_152;
/// The profile's thread count is NeonGammon's only parallelism: OpenMP,
/// OpenBLAS and Apple Accelerate (vecLib) must not start their own pools.
/// `tools/probe_open_sage.py` starts its worker with the same values.
const SINGLE_THREADED_MATH: [(&str, &str); 3] = [
    ("OMP_NUM_THREADS", "1"),
    ("OPENBLAS_NUM_THREADS", "1"),
    ("VECLIB_MAXIMUM_THREADS", "1"),
];
// One worker for playing, one for evaluations a person waits for, and one for
// everything else, each behind its own lock. A game retains its general worker,
// but each exchange reserves threads separately so a waiting play or
// interactive request can run between evaluations. Benchmarks still exclude
// all other engine work through the shared engine gate.
static GENERAL: Mutex<Option<Worker>> = Mutex::const_new(None);
static PLAY: Mutex<Option<Worker>> = Mutex::const_new(None);
static INTERACTIVE: Mutex<Option<Worker>> = Mutex::const_new(None);
/// Installation checks run one at a time, each in its own process. They wait
/// for none of these workers, so a game analysis does not delay a new match, the
/// engine list or the submission of another analysis.
static PROBE: Mutex<()> = Mutex::const_new(());
use crate::engine::cpu::{CpuPermit, HostCpuBudget, Workload};

async fn acquire_threads(profile: &SageSnapshot, workload: Workload) -> Result<CpuPermit<'static>> {
    HostCpuBudget::host()
        .await
        .acquire(profile.threads, workload)
        .await
}

#[derive(Clone, Copy)]
pub(super) enum Slot {
    General,
    Play,
    /// Hints, Coach and Tutor evaluations, and Study answers.
    Interactive,
}

impl Slot {
    fn workload(self) -> Workload {
        match self {
            Self::General => Workload::Background,
            Self::Play | Self::Interactive => Workload::Foreground,
        }
    }

    fn worker(self) -> &'static Mutex<Option<Worker>> {
        match self {
            Self::General => &GENERAL,
            Self::Play => &PLAY,
            Self::Interactive => &INTERACTIVE,
        }
    }
}

/// A cached worker belongs to the runtime that started it.
async fn reap_other_runtime(slot: &mut Option<Worker>) -> Result<()> {
    let current = tokio::runtime::Handle::current().id();
    if slot
        .as_ref()
        .is_some_and(|worker| worker.runtime_id != current)
        && let Some(worker) = slot.take()
    {
        worker.shutdown().await?;
    }
    Ok(())
}

struct Worker {
    child: Option<Child>,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr: tokio::task::JoinHandle<()>,
    profile: SageSnapshot,
    runtime_id: tokio::runtime::Id,
    next_id: u64,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stderr.abort();
        if let Some(child) = self.child.take() {
            terminate_and_reap(child);
        }
    }
}

impl Worker {
    async fn checkout(slot: &mut Option<Self>, profile: &SageSnapshot) -> Result<Self> {
        match slot.take() {
            Some(worker)
                if worker.profile == *profile
                    && worker.runtime_id == tokio::runtime::Handle::current().id() =>
            {
                Ok(worker)
            }
            previous => {
                if let Some(previous) = previous {
                    previous.shutdown().await?;
                }
                Self::initialize(profile).await
            }
        }
    }

    async fn start(profile: &SageSnapshot) -> Result<Self> {
        let probe = include_str!("../../../tools/probe_open_sage.py");
        let script = format!(
            "import json\nintegrity = {{'__name__': 'neondojo_integrity', 'RELEASES': json.loads({})}}\nexec({}, integrity)\n{}\nmain(integrity)\n",
            serde_json::to_string(super::RELEASES_JSON)?,
            serde_json::to_string(probe)?,
            include_str!("worker.py")
        );
        let mut child = Command::new(&profile.python)
            .args(["-I", "-B", "-u", "-c", &script])
            .envs(SINGLE_THREADED_MATH)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("Cannot start NeonGammon Python; install the locked native environment")?;
        let stdin = child.stdin.take().context("Missing NeonGammon stdin")?;
        let stdout = BufReader::new(child.stdout.take().context("Missing NeonGammon stdout")?);
        let mut stderr = child.stderr.take().context("Missing NeonGammon stderr")?;
        // Drain diagnostics with fixed memory. They are never part of the protocol.
        let stderr = tokio::spawn(async move {
            let mut buffer = [0u8; 4096];
            while let Ok(size) = stderr.read(&mut buffer).await {
                if size == 0 {
                    break;
                }
            }
        });
        Ok(Self {
            child: Some(child),
            stdin,
            stdout,
            stderr,
            profile: profile.clone(),
            runtime_id: tokio::runtime::Handle::current().id(),
            next_id: 1,
        })
    }

    async fn initialize(profile: &SageSnapshot) -> Result<Self> {
        let mut worker = Self::start(profile).await?;
        let (identity, _) = worker
            .exchange(json!({"operation":"initialize", "profile":profile}))
            .await?;
        validate_identity(profile, &identity)?;
        Ok(worker)
    }

    async fn shutdown(mut self) -> Result<()> {
        self.stderr.abort();
        if let Some(child) = self.child.as_mut() {
            // Keep ownership inside the drop guard while waiting: cancelling
            // shutdown must still kill/reap instead of abandoning the child.
            child
                .start_kill()
                .context("Cannot terminate NeonGammon worker")?;
            reap_child(child).await?;
        }
        self.child.take();
        Ok(())
    }

    async fn exchange(&mut self, mut request: Value) -> Result<(Value, Option<EngineCost>)> {
        let id = self.next_id;
        self.next_id = id.checked_add(1).context("Sidecar request ID exhausted")?;
        request["id"] = json!(id);
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= MAX_REQUEST,
            "NeonGammon request exceeds limit"
        );
        self.stdin.write_all(&bytes).await?;
        self.stdin.flush().await?;
        let mut line = Vec::new();
        (&mut self.stdout)
            .take(MAX_RESPONSE + 1)
            .read_until(b'\n', &mut line)
            .await?;
        ensure!(
            !line.is_empty() && line.len() as u64 <= MAX_RESPONSE && line.last() == Some(&b'\n'),
            "NeonGammon crashed or exceeded its output limit"
        );
        let response: Value =
            serde_json::from_slice(&line).context("Invalid NeonGammon protocol response")?;
        ensure!(
            response.get("id").and_then(Value::as_u64) == Some(id),
            "NeonGammon response ID mismatch"
        );
        if let Some(error) = response.get("error").and_then(Value::as_str) {
            anyhow::bail!("NeonGammon: {error}");
        }
        let cost = response
            .get("cost")
            .map(|cost| serde_json::from_value::<EngineCost>(cost.clone()))
            .transpose()
            .context("Invalid NeonGammon engine cost")?;
        let result = response
            .get("result")
            .cloned()
            .context("NeonGammon response omitted result")?;
        Ok((result, cost))
    }

    async fn validated_exchange(&mut self, payload: Value) -> Result<(Value, Option<EngineCost>)> {
        let (result, cost) = self.exchange(payload.clone()).await?;
        // Framing alone is insufficient: reject invalid typed results before
        // either a single request or a game block can reuse this worker.
        validate_payload(&self.profile, &payload, &result)?;
        Ok((result, cost))
    }

    async fn batch_exchange(self, payload: Value) -> Result<(Self, (Value, Option<EngineCost>))> {
        let _threads = acquire_threads(&self.profile, Workload::Background).await?;
        // The worker is dropped before its reservation if an exchange fails or is cancelled.
        let mut worker = self;
        let result = worker.validated_exchange(payload).await?;
        Ok((worker, result))
    }
}

/// Exclusive game-scoped ownership. Callers keep provisional results private
/// until finish succeeds and hold the shared engine gate throughout this scope.
/// Dropping an unfinished block kills/reaps its worker rather than caching it.
pub(crate) struct VerifiedBatch<'a> {
    worker: Option<Worker>,
    cache: MutexGuard<'a, Option<Worker>>,
}

impl<'a> VerifiedBatch<'a> {
    pub(super) async fn begin(profile: &SageSnapshot) -> Result<Self> {
        Self::begin_in(&GENERAL, profile).await
    }

    async fn begin_in(slot: &'a Mutex<Option<Worker>>, profile: &SageSnapshot) -> Result<Self> {
        let mut cache = slot.lock().await;
        let worker = tokio::time::timeout(Duration::from_secs(120), async {
            let _threads = acquire_threads(profile, Workload::Background).await?;
            reap_other_runtime(&mut cache).await?;
            let mut worker = Worker::checkout(&mut cache, profile).await?;
            worker
                .validated_exchange(json!({"operation":"begin_batch"}))
                .await?;
            Ok::<_, anyhow::Error>(worker)
        })
        .await
        .context("NeonGammon batch initialization timed out")??;
        Ok(Self {
            worker: Some(worker),
            cache,
        })
    }

    pub(super) async fn request_costed(
        &mut self,
        payload: Value,
        timeout: Duration,
    ) -> Result<(Value, Option<EngineCost>)> {
        let worker = self.worker.take().context("NeonGammon batch was aborted")?;
        ensure!(
            matches!(
                payload["operation"].as_str(),
                Some("checker" | "cube" | "checker_batch" | "cube_batch_nply")
            ),
            "Unsupported NeonGammon game operation"
        );
        // Removing ownership also poisons the block if this individual future
        // is cancelled while its caller still retains the batch object.
        let (worker, result) = tokio::time::timeout(timeout, worker.batch_exchange(payload))
            .await
            .context("NeonGammon evaluation timed out; worker terminated")??;
        self.worker = Some(worker);
        Ok(result)
    }

    pub(super) async fn finish(mut self) -> Result<()> {
        let worker = self.worker.take().context("NeonGammon batch was aborted")?;
        let (worker, _) = tokio::time::timeout(
            Duration::from_secs(120),
            worker.batch_exchange(json!({"operation":"end_batch"})),
        )
        .await
        .context("NeonGammon final integrity check timed out; worker terminated")??;
        *self.cache = Some(worker);
        Ok(())
    }
}

/// Each running duel owns its workers for one match (or money block). Equal
/// immutable profiles share one worker, including a single integrity boundary.
pub(crate) struct DuelWorkers<'a> {
    profiles: [Option<&'a SageSnapshot>; 2],
    slots: [Mutex<Option<Worker>>; 2],
}

impl<'a> DuelWorkers<'a> {
    pub(crate) fn new(profiles: [Option<&'a SageSnapshot>; 2]) -> Self {
        Self {
            profiles,
            slots: Default::default(),
        }
    }

    pub(crate) async fn begin_game(&self) -> Result<DuelBatches<'_>> {
        let shared = self.profiles[0].is_some() && self.profiles[0] == self.profiles[1];
        let mut batches = [None, None];
        for (side, &profile) in self.profiles.iter().enumerate() {
            if let Some(profile) = profile
                && !(shared && side == 1)
            {
                batches[side] = Some(VerifiedBatch::begin_in(&self.slots[side], profile).await?);
            }
        }
        Ok(DuelBatches { batches, shared })
    }
}

pub(crate) struct DuelBatches<'a> {
    batches: [Option<VerifiedBatch<'a>>; 2],
    shared: bool,
}

impl<'a> DuelBatches<'a> {
    pub(crate) fn for_side(&mut self, side: usize) -> Result<&mut VerifiedBatch<'a>> {
        self.batches[if self.shared { 0 } else { side }]
            .as_mut()
            .context("Missing verified NeonGammon duel worker")
    }

    pub(crate) async fn finish(self) -> Result<()> {
        for batch in self.batches.into_iter().flatten() {
            batch.finish().await?;
        }
        Ok(())
    }
}

pub(super) async fn request(
    profile: &SageSnapshot,
    payload: Value,
    timeout: Duration,
    slot: Slot,
) -> Result<Value> {
    Ok(request_costed(profile, payload, timeout, slot).await?.0)
}

/// A single request together with the engine cost the worker measured.
pub(super) async fn request_costed(
    profile: &SageSnapshot,
    payload: Value,
    timeout: Duration,
    slot: Slot,
) -> Result<(Value, Option<EngineCost>)> {
    let operation = payload["operation"].as_str();
    ensure!(
        matches!(
            operation,
            Some("identity" | "checker" | "cube" | "resignation")
        ) || matches!(
            (slot, operation),
            (Slot::Play, Some("opponent_checker" | "opponent_cube"))
        ),
        "Unsupported NeonGammon single-request operation"
    );
    crate::engine::with_shared_access_timeout(timeout, async {
        let mut cache = slot.worker().lock().await;
        let _threads = acquire_threads(profile, slot.workload()).await?;
        reap_other_runtime(&mut cache).await?;
        // Own the worker while awaiting I/O so cancellation discards its stream.
        let mut worker = Worker::checkout(&mut cache, profile).await?;
        let result = worker.validated_exchange(payload).await?;
        *cache = Some(worker);
        Ok(result)
    })
    .await
    .context("NeonGammon request timed out, including the wait for its worker, CPU budget and engine access")?
}

pub(super) async fn probe(profile: &SageSnapshot) -> Result<Value> {
    crate::engine::with_shared_access(async {
        tokio::time::timeout(Duration::from_secs(120), async {
            let _single = PROBE.lock().await;
            let _threads = acquire_threads(profile, Workload::Foreground).await?;
            let mut worker = Worker::start(profile).await?;
            let (identity, _) = worker
                .exchange(json!({"operation":"initialize", "profile":profile}))
                .await?;
            worker.shutdown().await?;
            Ok(identity)
        })
        .await
        .context("NeonGammon installation check timed out, including the wait for another check and the CPU budget")?
    })
    .await
}

/// Start a genuinely cold worker while respecting the same application gate.
/// The old child is reaped before the new interpreter is started.
pub(super) async fn restart(profile: &SageSnapshot) -> Result<()> {
    crate::engine::with_shared_access(async {
        let mut cache = GENERAL.lock().await;
        tokio::time::timeout(Duration::from_secs(120), async {
            let _threads = acquire_threads(profile, Workload::Background).await?;
            reap_other_runtime(&mut cache).await?;
            if let Some(previous) = cache.take() {
                previous.shutdown().await?;
            }
            let worker = Worker::initialize(profile).await?;
            *cache = Some(worker);
            Ok(())
        })
        .await
        .context("NeonGammon restart timed out")?
    })
    .await
}

/// Prepare the exact saved play profile. Checkout validates the identity and
/// thread budget on a cold start. Every subsequent evaluation still performs
/// the usual before/after integrity checks.
pub(super) async fn warm_play(profile: &SageSnapshot) -> Result<()> {
    crate::engine::with_shared_access(async {
        let mut cache = PLAY.lock().await;
        tokio::time::timeout(Duration::from_secs(120), async {
            reap_other_runtime(&mut cache).await?;
            if cache
                .as_ref()
                .is_some_and(|worker| worker.profile == *profile)
            {
                return Ok(());
            }
            let _threads = acquire_threads(profile, Workload::Foreground).await?;
            let worker = Worker::checkout(&mut cache, profile).await?;
            *cache = Some(worker);
            Ok(())
        })
        .await
        .context("NeonGammon play warm-up timed out")?
    })
    .await
}

fn validate_payload(profile: &SageSnapshot, request: &Value, result: &Value) -> Result<()> {
    match request.get("operation").and_then(Value::as_str) {
        Some("identity" | "begin_batch" | "end_batch") => validate_identity(profile, result),
        Some(operation @ ("checker_batch" | "cube_batch_nply")) => {
            let rows = request["rows"]
                .as_array()
                .context("Missing NeonGammon batch rows")?;
            ensure!(
                !rows.is_empty() && rows.len() <= 64,
                "Invalid NeonGammon batch size"
            );
            if rows.len() > 1 && result == &json!({"split":true}) {
                return Ok(());
            }
            let values = result
                .as_array()
                .context("Invalid NeonGammon batch response")?;
            ensure!(
                rows.len() == values.len(),
                "Incomplete NeonGammon batch response"
            );
            let action = if operation == "checker_batch" {
                "checker"
            } else {
                "cube"
            };
            for (row, value) in rows.iter().zip(values) {
                ensure!(
                    row["operation"] == action,
                    "Invalid NeonGammon batch action"
                );
                validate_payload(profile, row, value)?;
            }
            Ok(())
        }
        Some("checker") => {
            let candidates: Vec<super::SageCheckerCandidate> =
                serde_json::from_value(result.clone())
                    .context("Invalid NeonGammon checker response")?;
            ensure!(
                !candidates.is_empty(),
                "NeonGammon returned no checker candidates"
            );
            let before = request
                .get("board")
                .map(|board| serde_json::from_value::<[i32; 26]>(board.clone()))
                .transpose()?;
            for candidate in &candidates {
                super::validate_evaluation(candidate.equity, &candidate.probabilities)?;
                super::context::from_sage(&candidate.board, 0)?;
                if let Some(before) = before {
                    if candidate.board == before {
                        ensure!(
                            candidate.notation.is_empty(),
                            "Invalid forced-pass notation"
                        );
                        let absolute = super::context::from_sage(&before, 0)?;
                        let dice = serde_json::from_value::<[i32; 2]>(request["dice"].clone())?;
                        crate::backgammon::apply_checker_turn(&absolute, 0, dice, &[])?;
                    } else {
                        ensure!(
                            !candidate.notation.is_empty(),
                            "NeonGammon omitted checker notation"
                        );
                    }
                }
            }
            if let Some(forced) = request.get("force_board").filter(|value| !value.is_null()) {
                let forced: [i32; 26] = serde_json::from_value(forced.clone())?;
                ensure!(
                    candidates.iter().any(|candidate| candidate.board == forced),
                    "NeonGammon omitted the played candidate"
                );
            }
            Ok(())
        }
        Some("cube") if request.get("decision_only") == Some(&Value::Bool(true)) => {
            serde_json::from_value::<super::SageCubeDecision>(result.clone())
                .context("Invalid NeonGammon cube decision response")?
                .validate()
        }
        Some("cube") => {
            let cube: super::SageCubeEvaluation = serde_json::from_value(result.clone())
                .context("Invalid NeonGammon cube response")?;
            for equity in [cube.no_double, cube.take, cube.pass] {
                super::validate_evaluation(equity, &cube.probabilities)?;
            }
            Ok(())
        }
        Some("opponent_checker") => {
            let board: [i32; 26] = serde_json::from_value(result.clone())
                .context("Invalid NeonGammon opponent board")?;
            super::context::from_sage(&board, 0)?;
            Ok(())
        }
        Some("opponent_cube") => {
            serde_json::from_value::<super::SageOpponentCube>(result.clone())
                .context("Invalid NeonGammon opponent cube response")?;
            Ok(())
        }
        Some("resignation") => {
            ensure!(
                result.get("accept").and_then(Value::as_bool).is_some(),
                "NeonGammon omitted resignation decision"
            );
            for field in ["continuation", "offered"] {
                ensure!(
                    result
                        .get(field)
                        .and_then(Value::as_f64)
                        .is_some_and(f64::is_finite),
                    "NeonGammon returned invalid resignation equity"
                );
            }
            Ok(())
        }
        _ => anyhow::bail!("Unsupported NeonGammon response operation"),
    }
}

pub(super) fn validate_identity(profile: &SageSnapshot, identity: &Value) -> Result<()> {
    ensure!(
        identity["package_version"].as_str() == Some(&profile.package_version)
            && identity["artifact_fingerprint"].as_str() == Some(&profile.artifact_fingerprint),
        "NeonGammon installation differs from the immutable job snapshot"
    );
    Ok(())
}

#[cfg(all(test, unix))]
#[path = "process_tests.rs"]
pub(super) mod tests;
