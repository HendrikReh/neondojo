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
pub(super) struct VerifiedBatch {
    worker: Option<Worker>,
    cache: MutexGuard<'static, Option<Worker>>,
}

impl VerifiedBatch {
    pub(super) async fn begin(profile: &SageSnapshot) -> Result<Self> {
        let mut cache = GENERAL.lock().await;
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
pub(super) mod tests {
    use super::super::{AnalysisEngine, SageEngine};
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    async fn request(profile: &SageSnapshot, payload: Value, timeout: Duration) -> Result<Value> {
        super::request(profile, payload, timeout, Slot::General).await
    }

    static TEST_LOCK: Mutex<()> = Mutex::const_new(());

    /// A NeonGammon engine that passes validation but whose process cannot start.
    pub(crate) fn unstartable_engine(plies: i32) -> crate::settings::EngineSnapshot {
        crate::settings::EngineSnapshot {
            engine_id: "bgsage".into(),
            gnubg_path: "/missing/gnubg".into(),
            engine_version: None,
            gnubg_threads: None,
            gnubg_move_filter: None,
            open_sage: Some(SageSnapshot {
                python: "/missing/python".into(),
                package_version: "3.1.1".into(),
                artifact_fingerprint: "a".repeat(64),
                threads: 2,
                plies,
                adapter_version: 1,
                one_ply_verified: false,
                cubeless: false,
            }),
        }
    }

    /// Occupies the general worker, as a game analysis does, until dropped.
    pub(crate) struct GeneralWorkerHeld {
        _general: MutexGuard<'static, Option<Worker>>,
        _serial: MutexGuard<'static, ()>,
    }

    pub(crate) async fn hold_general_worker() -> GeneralWorkerHeld {
        let serial = TEST_LOCK.lock().await;
        GeneralWorkerHeld {
            _general: GENERAL.lock().await,
            _serial: serial,
        }
    }

    struct FakeWorker {
        directory: tempfile::TempDir,
        profile: SageSnapshot,
    }

    impl FakeWorker {
        fn new(mode: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let executable = directory.path().join("fake-python");
            let source = r#"#!/usr/bin/env python3
import json, os, pathlib, sys, time
root = pathlib.Path(__file__).parent
(root / 'pid').write_text(str(os.getpid()))
(root / 'environment').write_text(json.dumps({key: os.environ.get(key) for key in
    ('OMP_NUM_THREADS', 'OPENBLAS_NUM_THREADS', 'VECLIB_MAXIMUM_THREADS')}))
mode = MODE
for line in sys.stdin:
    request = json.loads(line)
    with (root / 'operations').open('a') as log:
        log.write(request['operation'] + '\n')
    if request['operation'] == 'initialize':
        profile = request['profile']
        result = {key: profile[key] for key in ('package_version', 'artifact_fingerprint')}
        print(json.dumps({'id': request['id'], 'result': result}), flush=True)
        continue
    (root / 'started').write_text('ready')
    if mode.startswith('batch_'):
        operation = request['operation']
        if mode == 'batch_end_block' and operation == 'end_batch':
            time.sleep(60)
        if operation in ('checker', 'checker_batch'):
            (root / 'checker_started').write_text('ready')
            if mode == 'batch_block':
                time.sleep(60)
            if mode == 'batch_gated':
                while not (root / 'checker_release').exists():
                    time.sleep(.005)
            def checker(row):
                moves = [{'board': row.get('force_board', [0] * 26), 'notation': '8/5 6/5',
                          'equity': sum(row.get('dice', [0, 0])) / 10,
                          'probabilities': [.6, .1, .01, .4, .1, .01]}]
                if mode == 'batch_bad_checker':
                    del moves[0]['probabilities']
                return moves
            result = [checker(row) for row in request['rows']] if operation == 'checker_batch' else checker(request)
            if operation == 'checker_batch' and mode == 'batch_split' and len(request['rows']) > 12:
                result = {'split': True}
            if operation == 'checker_batch' and mode == 'batch_short':
                result = result[:-1]
            if mode == 'batch_bad_json':
                print('{invalid}', flush=True)
                continue
        else:
            result = {key: profile[key] for key in ('package_version', 'artifact_fingerprint')}
            if (mode == 'batch_bad_end' and operation == 'end_batch') or (mode == 'batch_bad_begin' and operation == 'begin_batch'):
                result['artifact_fingerprint'] = 'changed'
        print(json.dumps({'id': request['id'], 'result': result}), flush=True)
        continue
    if mode == 'block':
        time.sleep(60)
    elif mode == 'bad_checker':
        result = [{'board': [0] * 26, 'notation': '1/off', 'equity': 0}]
    elif mode == 'bad_cube':
        result = {'no_double': 0, 'take': 0, 'pass': 1, 'probabilities': [.6, .9, 0, .4, 0, 0]}
    elif mode == 'bad_resignation':
        result = {'accept': 'yes', 'continuation': 0, 'offered': 1}
    elif mode == 'bad_identity':
        result = {'package_version': 'changed', 'artifact_fingerprint': 'changed'}
    elif mode == 'wrong_id':
        print(json.dumps({'id': request['id'] + 1, 'result': None}), flush=True)
        continue
    elif mode == 'bad_json':
        print('{not json}', flush=True)
        continue
    elif mode == 'oversize':
        print('x' * 2097153, flush=True)
        continue
    else:
        result = {key: profile[key] for key in ('package_version', 'artifact_fingerprint')}
    print(json.dumps({'id': request['id'], 'result': result}), flush=True)
"#;
            std::fs::write(
                &executable,
                source.replace("MODE", &serde_json::to_string(mode).unwrap()),
            )
            .unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                profile: SageSnapshot {
                    python: executable.to_str().unwrap().into(),
                    package_version: "3.0.0".into(),
                    artifact_fingerprint: "a".repeat(64),
                    threads: 2,
                    plies: 2,
                    adapter_version: 1,
                    one_ply_verified: false,
                    cubeless: false,
                },
                directory,
            }
        }

        fn pid(&self) -> String {
            std::fs::read_to_string(self.directory.path().join("pid")).unwrap()
        }

        async fn wait_started(&self) {
            tokio::time::timeout(Duration::from_secs(5), async {
                while !self.directory.path().join("started").is_file() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("Fake worker did not receive its request");
        }
    }

    async fn assert_reaped(pid: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let alive = Command::new("/bin/kill")
                    .args(["-0", pid])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await
                    .unwrap()
                    .success();
                if !alive {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Sidecar process survived or was not reaped");
    }

    const SINGLE_GAME: &str = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";

    #[tokio::test]
    async fn ordinary_requests_cannot_cache_worker_with_open_batch() {
        let _serial = TEST_LOCK.lock().await;
        let fake = FakeWorker::new("batch_healthy");
        let result = request(
            &fake.profile,
            json!({"operation":"begin_batch"}),
            Duration::from_secs(5),
        )
        .await;
        let cached = GENERAL.lock().await.take();
        let was_cached = cached.is_some();
        drop(cached);
        if fake.directory.path().join("pid").exists() {
            assert_reaped(&fake.pid()).await;
        }
        assert!(
            result.is_err(),
            "An ordinary request started an unowned batch"
        );
        assert!(!was_cached, "Cached a worker with an unfinished batch");
    }

    #[tokio::test]
    async fn game_rejects_final_integrity_failure_without_caching_worker() {
        let _serial = TEST_LOCK.lock().await;
        for mode in ["batch_bad_begin", "batch_bad_end"] {
            let fake = FakeWorker::new(mode);
            let engine = super::super::SageEngine::new(fake.profile.clone()).unwrap();
            let result = engine.analyze_game(SINGLE_GAME, 1, 2).await;
            let cached = GENERAL.lock().await.take();
            let was_cached = cached.is_some();
            drop(cached);
            assert_reaped(&fake.pid()).await;
            assert!(
                result.is_err(),
                "Released a game before its final integrity check"
            );
            assert!(
                !was_cached,
                "Cached a worker after failed final verification"
            );
        }
    }

    #[tokio::test]
    async fn complete_games_have_exclusive_integrity_boundaries() {
        let _serial = TEST_LOCK.lock().await;
        let fake = FakeWorker::new("batch_healthy");
        let engine = super::super::SageEngine::new(fake.profile.clone()).unwrap();
        let (first, second) = tokio::join!(
            engine.analyze_game(SINGLE_GAME, 1, 2),
            engine.analyze_game(SINGLE_GAME, 1, 2)
        );
        engine.version().await.unwrap();
        let operations = std::fs::read_to_string(fake.directory.path().join("operations")).unwrap();
        drop(GENERAL.lock().await.take());
        assert_reaped(&fake.pid()).await;
        assert_eq!(first.unwrap().decisions.len(), 1);
        assert_eq!(second.unwrap().decisions.len(), 1);
        assert_eq!(
            operations,
            "initialize\nbegin_batch\nchecker_batch\nend_batch\nbegin_batch\nchecker_batch\nend_batch\nidentity\n"
        );
    }

    #[tokio::test]
    async fn rolls_preserve_order_and_progress_with_bounded_batch_frames() {
        let _serial = TEST_LOCK.lock().await;
        let rolls: Vec<_> = (1..=6).flat_map(|a| (a..=6).map(move |b| [a, b])).collect();
        for (mode, frames) in [("batch_healthy", 1), ("batch_split", 3)] {
            let fake = FakeWorker::new(mode);
            let engine = SageEngine::new(fake.profile.clone()).unwrap();
            let mut progress = Vec::new();
            let analyses = engine
                .analyze_rolls("4HPwATDgc/ABMA", "cAkHAAAAAAAE", 2, &rolls, |n| {
                    progress.push(n);
                    std::future::ready(Ok(()))
                })
                .await
                .unwrap();
            assert_eq!(progress, (1..=21).collect::<Vec<_>>());
            for (roll, analysis) in rolls.iter().zip(analyses) {
                assert_eq!(
                    analysis.candidates[0].equity,
                    f64::from(roll[0] + roll[1]) / 10.0
                );
            }
            let operations =
                std::fs::read_to_string(fake.directory.path().join("operations")).unwrap();
            assert_eq!(
                operations
                    .lines()
                    .filter(|op| *op == "checker_batch")
                    .count(),
                frames
            );
            assert_eq!(operations.lines().last(), Some("end_batch"));
            drop(GENERAL.lock().await.take());
            assert_reaped(&fake.pid()).await;
        }
    }

    #[tokio::test]
    async fn invalid_batch_frames_and_typed_results_discard_worker() {
        let _serial = TEST_LOCK.lock().await;
        for mode in ["batch_bad_checker", "batch_bad_json", "batch_short"] {
            let fake = FakeWorker::new(mode);
            let engine = super::super::SageEngine::new(fake.profile.clone()).unwrap();
            assert!(engine.analyze_game(SINGLE_GAME, 1, 2).await.is_err());
            assert!(GENERAL.lock().await.is_none());
            assert_reaped(&fake.pid()).await;
        }
    }

    #[tokio::test]
    async fn cancelling_game_discards_active_batch() {
        let _serial = TEST_LOCK.lock().await;
        let fake = FakeWorker::new("batch_block");
        let engine = super::super::SageEngine::new(fake.profile.clone()).unwrap();
        let task = tokio::spawn(async move { engine.analyze_game(SINGLE_GAME, 1, 2).await });
        tokio::time::timeout(Duration::from_secs(5), async {
            while !fake.directory.path().join("checker_started").is_file() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(GENERAL.lock().await.is_none());
        assert_reaped(&fake.pid()).await;
    }

    #[tokio::test]
    async fn game_timeout_discards_active_batch() {
        let _serial = TEST_LOCK.lock().await;
        for mode in ["batch_block", "batch_end_block"] {
            let fake = FakeWorker::new(mode);
            let engine = super::super::SageEngine::new(fake.profile.clone()).unwrap();
            // Start Python before measuring the active batch timeout. Process
            // startup can exceed the short limit on a busy test machine.
            request(
                &fake.profile,
                json!({"operation": "identity"}),
                Duration::from_secs(5),
            )
            .await
            .unwrap();
            let pid = fake.pid();
            let error = engine
                .analyze_game_with_limit(SINGLE_GAME, 1, 2, Some(Duration::from_millis(250)))
                .await
                .unwrap_err();
            assert!(error.to_string().contains("game analysis timed out"));
            assert!(GENERAL.lock().await.is_none());
            assert_reaped(&pid).await;
        }
    }

    #[tokio::test]
    async fn cancelling_one_request_prevents_finishing_retained_batch() {
        let _serial = TEST_LOCK.lock().await;
        let fake = FakeWorker::new("batch_block");
        let mut batch = VerifiedBatch::begin(&fake.profile).await.unwrap();
        let cancelled = tokio::time::timeout(
            Duration::from_millis(100),
            batch.request_costed(
                json!({"operation":"checker", "force_board":vec![0; 26]}),
                Duration::from_secs(30),
            ),
        )
        .await;
        assert!(cancelled.is_err());
        assert!(batch.finish().await.is_err());
        assert!(GENERAL.lock().await.is_none());
        assert_reaped(&fake.pid()).await;
    }

    #[tokio::test]
    async fn invalid_typed_payload_is_rejected_and_worker_reaped() {
        let _serial = TEST_LOCK.lock().await;
        for (mode, operation) in [
            ("bad_checker", "checker"),
            ("bad_cube", "cube"),
            ("bad_identity", "identity"),
            ("bad_resignation", "resignation"),
        ] {
            let fake = FakeWorker::new(mode);
            let result = request(
                &fake.profile,
                json!({"operation":operation}),
                Duration::from_secs(5),
            )
            .await;
            // Clean up even on the red baseline, which incorrectly caches it.
            let cached = GENERAL.lock().await.take();
            let was_cached = cached.is_some();
            drop(cached);
            assert_reaped(&fake.pid()).await;
            assert!(result.is_err(), "Accepted invalid {operation} payload");
            assert!(
                !was_cached,
                "Cached worker after invalid {operation} payload"
            );
        }
    }

    #[test]
    fn only_decision_only_cube_responses_may_omit_probabilities() {
        let profile = FakeWorker::new("clean").profile;
        let decision = json!({"no_double":0.2,"take":0.3,"pass":1.0});
        let full = json!({"no_double":0.2,"take":0.3,"pass":1.0,
                          "probabilities":[0.6,0.1,0.01,0.4,0.1,0.01]});
        let scoped = json!({"operation":"cube","decision_only":true});
        for request in [
            json!({"operation":"cube"}),
            json!({"operation":"cube","decision_only":false}),
        ] {
            validate_payload(&profile, &request, &full).unwrap();
            assert!(validate_payload(&profile, &request, &decision).is_err());
        }
        validate_payload(&profile, &scoped, &decision).unwrap();
        assert!(
            validate_payload(
                &profile,
                &scoped,
                &json!({"no_double":0.2,"take":null,"pass":1.0})
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn framing_faults_discard_and_reap_worker() {
        let _serial = TEST_LOCK.lock().await;
        for mode in ["wrong_id", "bad_json", "oversize"] {
            let fake = FakeWorker::new(mode);
            assert!(
                request(
                    &fake.profile,
                    json!({"operation":"identity"}),
                    Duration::from_secs(5)
                )
                .await
                .is_err()
            );
            assert!(GENERAL.lock().await.is_none());
            assert_reaped(&fake.pid()).await;
        }
    }

    #[tokio::test]
    async fn timeout_discards_and_reaps_active_worker() {
        let _serial = TEST_LOCK.lock().await;
        let fake = FakeWorker::new("block");
        let result = request(
            &fake.profile,
            json!({"operation":"identity"}),
            Duration::from_millis(250),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("timed out"));
        assert!(GENERAL.lock().await.is_none());
        assert_reaped(&fake.pid()).await;
    }

    #[tokio::test]
    async fn cancelled_request_discards_and_reaps_active_worker() {
        let _serial = TEST_LOCK.lock().await;
        let fake = FakeWorker::new("block");
        let profile = fake.profile.clone();
        let task = tokio::spawn(async move {
            request(
                &profile,
                json!({"operation":"identity"}),
                Duration::from_secs(30),
            )
            .await
        });
        fake.wait_started().await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(GENERAL.lock().await.is_none());
        assert_reaped(&fake.pid()).await;
    }

    #[tokio::test]
    async fn healthy_worker_is_reused_for_matching_snapshot() {
        let _serial = TEST_LOCK.lock().await;
        let fake = FakeWorker::new("healthy");
        request(
            &fake.profile,
            json!({"operation":"identity"}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        let pid = fake.pid();
        request(
            &fake.profile,
            json!({"operation":"identity"}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(fake.pid(), pid);
        drop(GENERAL.lock().await.take());
        assert_reaped(&pid).await;
    }

    #[test]
    fn diagnostic_probe_starts_its_worker_with_the_same_math_threads() {
        let probe = include_str!("../../../tools/probe_open_sage.py");
        let line = probe
            .lines()
            .find(|line| line.starts_with("SINGLE_THREADED_MATH = "))
            .expect("probe lists its math library threads");
        let listed: Vec<&str> = line.split('\'').skip(1).step_by(2).collect();
        let expected: Vec<&str> = SINGLE_THREADED_MATH
            .iter()
            .flat_map(|(key, value)| [*key, *value])
            .collect();
        assert_eq!(listed, expected);
    }

    #[tokio::test]
    async fn workers_and_probes_disable_native_math_thread_pools() {
        let _serial = TEST_LOCK.lock().await;
        let single = json!({"OMP_NUM_THREADS":"1","OPENBLAS_NUM_THREADS":"1",
                            "VECLIB_MAXIMUM_THREADS":"1"});
        let worker = FakeWorker::new("healthy");
        request(
            &worker.profile,
            json!({"operation":"identity"}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        let check = FakeWorker::new("healthy");
        probe(&check.profile).await.unwrap();
        for fake in [&worker, &check] {
            let environment: Value = serde_json::from_str(
                &std::fs::read_to_string(fake.directory.path().join("environment")).unwrap(),
            )
            .unwrap();
            assert_eq!(environment, single);
        }
        drop(GENERAL.lock().await.take());
        assert_reaped(&worker.pid()).await;
        assert_reaped(&check.pid()).await;
    }

    #[tokio::test]
    async fn installation_probe_leaves_verified_worker_available() {
        let _serial = TEST_LOCK.lock().await;
        let cached = FakeWorker::new("healthy");
        let check = FakeWorker::new("healthy");
        let payload = json!({"operation":"identity"});
        request(&cached.profile, payload.clone(), Duration::from_secs(5))
            .await
            .unwrap();
        let pid = cached.pid();
        probe(&check.profile).await.unwrap();
        assert_reaped(&check.pid()).await;
        request(&cached.profile, payload, Duration::from_secs(5))
            .await
            .unwrap();
        let reused = cached.pid();
        drop(GENERAL.lock().await.take());
        assert_reaped(&reused).await;
        assert_eq!(reused, pid, "An isolated probe evicted a healthy sidecar");
    }

    #[tokio::test]
    async fn warm_play_survives_probes_analysis_profiles_and_cold_benchmark() {
        let _serial = TEST_LOCK.lock().await;
        let game = FakeWorker::new("healthy");
        let analysis = FakeWorker::new("healthy");
        let check = FakeWorker::new("healthy");
        let payload = json!({"operation":"identity"});
        let limit = Duration::from_secs(5);
        warm_play(&game.profile).await.unwrap();
        let pid = game.pid();
        warm_play(&game.profile).await.unwrap();
        assert_eq!(game.pid(), pid);
        let operations = std::fs::read_to_string(game.directory.path().join("operations")).unwrap();
        assert_eq!(operations, "initialize\n");

        probe(&check.profile).await.unwrap();
        assert_reaped(&check.pid()).await;
        let mut profile = analysis.profile.clone();
        profile.threads = 4;
        for depth in [2, 1] {
            profile.plies = depth;
            let batch = VerifiedBatch::begin(&profile).await.unwrap();
            batch.finish().await.unwrap();
        }
        restart(&profile).await.unwrap();
        super::request(&game.profile, payload, limit, Slot::Play)
            .await
            .unwrap();
        assert_eq!(game.pid(), pid, "Other work displaced the playing worker");

        drop(PLAY.lock().await.take());
        drop(GENERAL.lock().await.take());
        assert_reaped(&pid).await;
        assert_reaped(&analysis.pid()).await;
    }

    #[tokio::test]
    async fn invalid_play_response_is_reaped_without_losing_analysis_worker() {
        let _serial = TEST_LOCK.lock().await;
        let game = FakeWorker::new("bad_identity");
        let analysis = FakeWorker::new("healthy");
        let payload = json!({"operation":"identity"});
        let limit = Duration::from_secs(5);
        request(&analysis.profile, payload.clone(), limit)
            .await
            .unwrap();
        let analysis_pid = analysis.pid();
        warm_play(&game.profile).await.unwrap();
        assert!(
            super::request(&game.profile, payload.clone(), limit, Slot::Play)
                .await
                .is_err()
        );
        assert!(PLAY.lock().await.is_none());
        assert_reaped(&game.pid()).await;
        request(&analysis.profile, payload, limit).await.unwrap();
        assert_eq!(analysis.pid(), analysis_pid);
        drop(GENERAL.lock().await.take());
        assert_reaped(&analysis_pid).await;
    }

    #[tokio::test]
    async fn restart_reaps_cached_worker_and_handshakes_a_fresh_process() {
        let _serial = TEST_LOCK.lock().await;
        let fake = FakeWorker::new("healthy");
        request(
            &fake.profile,
            json!({"operation":"identity"}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        let previous_pid = fake.pid();
        tokio::time::timeout(
            Duration::from_secs(5),
            crate::engine::benchmark_exclusive(restart(&fake.profile)),
        )
        .await
        .expect("Restart deadlocked inside the benchmark gate")
        .unwrap();
        let current_pid = fake.pid();
        let cached = GENERAL.lock().await.take();
        let is_cached = cached.is_some();
        drop(cached);
        assert_reaped(&previous_pid).await;
        assert_reaped(&current_pid).await;
        assert_ne!(previous_pid, current_pid, "Restart reused the old worker");
        assert!(is_cached, "Restart did not initialize a reusable worker");
    }
    #[test]
    fn cached_worker_is_replaced_after_originating_runtime_shuts_down() {
        let _serial = TEST_LOCK.blocking_lock();
        let fake = FakeWorker::new("healthy");
        let first = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        first
            .block_on(request(
                &fake.profile,
                json!({"operation":"identity"}),
                Duration::from_secs(5),
            ))
            .unwrap();
        let old_pid = fake.pid();
        drop(first);

        let second = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        second.block_on(async {
            request(
                &fake.profile,
                json!({"operation":"identity"}),
                Duration::from_secs(5),
            )
            .await
            .expect("A worker bound to a stopped runtime must be replaced");
            let new_pid = fake.pid();
            assert_ne!(old_pid, new_pid);
            assert_reaped(&old_pid).await;
            let worker = GENERAL.lock().await.take().unwrap();
            worker.shutdown().await.unwrap();
            assert_reaped(&new_pid).await;
        });
    }

    #[test]
    fn restart_reaps_worker_from_a_stopped_runtime() {
        let _serial = TEST_LOCK.blocking_lock();
        let fake = FakeWorker::new("healthy");
        let first = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        first
            .block_on(request(
                &fake.profile,
                json!({"operation":"identity"}),
                Duration::from_secs(5),
            ))
            .unwrap();
        let old_pid = fake.pid();
        drop(first);

        let second = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        second.block_on(async {
            restart(&fake.profile)
                .await
                .expect("Restart must reap a worker registered with a stopped runtime");
            let new_pid = fake.pid();
            assert_ne!(old_pid, new_pid);
            assert_reaped(&old_pid).await;
            let worker = GENERAL.lock().await.take().unwrap();
            worker.shutdown().await.unwrap();
            assert_reaped(&new_pid).await;
        });
    }

    #[tokio::test]
    async fn installation_checks_finish_while_an_analysis_block_holds_the_general_worker() {
        let _serial = TEST_LOCK.lock().await;
        let analysis = FakeWorker::new("batch_healthy");
        let check = FakeWorker::new("healthy");
        let batch = VerifiedBatch::begin(&analysis.profile).await.unwrap();
        let engine = AnalysisEngine::OpenSage(SageEngine::new(check.profile.clone()).unwrap());
        let checks = async { tokio::join!(probe(&check.profile), engine.check_installation()) };
        let (first, second) = tokio::time::timeout(Duration::from_secs(5), checks)
            .await
            .expect("An installation check waited for the analysis block");
        first.unwrap();
        second.unwrap();
        assert_reaped(&check.pid()).await;
        batch.finish().await.unwrap();
        assert!(
            GENERAL.lock().await.is_some(),
            "A check took the worker of the analysis block"
        );
        drop(GENERAL.lock().await.take());
        assert_reaped(&analysis.pid()).await;
    }

    #[tokio::test]
    async fn interactive_requests_keep_their_worker_beside_an_analysis_and_a_game() {
        let _serial = TEST_LOCK.lock().await;
        let analysis = FakeWorker::new("batch_healthy");
        let game = FakeWorker::new("healthy");
        let hint = FakeWorker::new("healthy");
        let engine =
            AnalysisEngine::OpenSage(SageEngine::new(hint.profile.clone()).unwrap()).interactive();
        let AnalysisEngine::OpenSage(engine) = engine else {
            unreachable!("The engine changed with its worker")
        };
        let batch = VerifiedBatch::begin(&analysis.profile).await.unwrap();
        warm_play(&game.profile).await.unwrap();
        let mut pids = Vec::new();
        for _ in 0..2 {
            tokio::time::timeout(Duration::from_secs(5), engine.version())
                .await
                .expect("An interactive request waited for the analysis block")
                .unwrap();
            pids.push(hint.pid());
        }
        assert_eq!(pids[0], pids[1], "An interactive request started cold");
        batch.finish().await.unwrap();
        restart(&analysis.profile).await.unwrap();
        for slot in [&GENERAL, &PLAY, &INTERACTIVE] {
            assert!(slot.lock().await.is_some(), "A worker was displaced");
        }
        engine.version().await.unwrap();
        assert_eq!(hint.pid(), pids[0], "A cold benchmark displaced the worker");
        for slot in [&GENERAL, &PLAY, &INTERACTIVE] {
            drop(slot.lock().await.take());
        }
        for fake in [&analysis, &game, &hint] {
            assert_reaped(&fake.pid()).await;
        }
    }

    #[tokio::test]
    async fn interactive_requests_wait_for_an_exclusive_benchmark() {
        let _serial = TEST_LOCK.lock().await;
        let hint = FakeWorker::new("healthy");
        let identity = json!({"operation":"identity"});
        let limit = Duration::from_millis(300);
        crate::engine::benchmark_exclusive(async {
            // A task of its own does not inherit the access of the benchmark.
            let (profile, identity) = (hint.profile.clone(), identity.clone());
            let blocked = tokio::spawn(async move {
                super::request(&profile, identity, limit, Slot::Interactive).await
            });
            assert!(blocked.await.unwrap().is_err(), "Overlapped a benchmark");
            assert!(!hint.directory.path().join("pid").exists());
        })
        .await;
        let limit = Duration::from_secs(5);
        super::request(&hint.profile, identity, limit, Slot::Interactive)
            .await
            .unwrap();
        drop(INTERACTIVE.lock().await.take());
        assert_reaped(&hint.pid()).await;
    }

    #[tokio::test]
    async fn moves_finish_while_an_analysis_block_holds_the_general_worker() {
        let _serial = TEST_LOCK.lock().await;
        let analysis = FakeWorker::new("batch_healthy");
        let game = FakeWorker::new("healthy");
        let identity = json!({"operation":"identity"});
        let batch = VerifiedBatch::begin(&analysis.profile).await.unwrap();
        let started = std::time::Instant::now();
        super::request(
            &game.profile,
            identity.clone(),
            Duration::from_secs(5),
            Slot::Play,
        )
        .await
        .unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "A move waited for the analysis block"
        );
        // Other general work still waits for the block and ends at its deadline.
        let waited = std::time::Instant::now();
        let general = request(&analysis.profile, identity, Duration::from_millis(300)).await;
        assert!(general.is_err(), "General work overlapped a game block");
        assert!(waited.elapsed() < Duration::from_secs(2));
        batch.finish().await.unwrap();
        assert!(
            GENERAL.lock().await.is_some(),
            "The finished block lost its worker"
        );
        drop(PLAY.lock().await.take());
        drop(GENERAL.lock().await.take());
        assert_reaped(&game.pid()).await;
        assert_reaped(&analysis.pid()).await;
    }

    #[tokio::test]
    async fn a_move_ends_at_its_deadline_including_the_wait_for_the_play_worker() {
        let _serial = TEST_LOCK.lock().await;
        let game = FakeWorker::new("healthy");
        // Another move owns the play worker for longer than this move's deadline.
        let held = PLAY.lock().await;
        let started = std::time::Instant::now();
        let error = super::request(
            &game.profile,
            json!({"operation":"identity"}),
            Duration::from_millis(300),
            Slot::Play,
        )
        .await
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("including the wait for its worker"),
            "{error:#}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "The deadline did not include the wait"
        );
        assert!(
            !game.directory.path().join("pid").exists(),
            "A move started a worker after its deadline"
        );
        drop(held);
        assert!(PLAY.lock().await.is_none());
    }

    #[tokio::test]
    async fn play_request_deadline_includes_exclusive_benchmark_wait() {
        let _serial = TEST_LOCK.lock().await;
        let game = FakeWorker::new("healthy");
        let identity = json!({"operation":"identity"});
        warm_play(&game.profile).await.unwrap();
        let pid = game.pid();
        let profile = game.profile.clone();
        let benchmark_identity = identity.clone();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (release, done) = tokio::sync::oneshot::channel();
        let benchmark = tokio::spawn(crate::engine::benchmark_exclusive(async move {
            entered.send(()).unwrap();
            done.await.unwrap();
            // Benchmark-owned requests still reuse the task's engine access.
            super::request(
                &game.profile,
                benchmark_identity,
                Duration::from_secs(5),
                Slot::Play,
            )
            .await
            .map(|_| game)
        }));
        ready.await.unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            super::request(&profile, identity, Duration::from_millis(60), Slot::Play),
        )
        .await;
        release.send(()).unwrap();
        let game = benchmark.await.unwrap().unwrap();
        let operations = std::fs::read_to_string(game.directory.path().join("operations")).unwrap();
        assert_eq!(
            game.pid(),
            pid,
            "Waiting for the gate displaced the play worker"
        );
        drop(PLAY.lock().await.take());
        assert_reaped(&pid).await;
        let error = result
            .expect("Play request never reached its deadline")
            .unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error:#}");
        assert_eq!(operations, "initialize\nidentity\n");
    }

    #[tokio::test]
    #[ignore = "requires isolated timing of an exclusive benchmark with a synthetic worker"]
    async fn benchmark_play_request_gate_wait() {
        let _serial = TEST_LOCK.lock().await;
        let game = FakeWorker::new("healthy");
        let identity = json!({"operation":"identity"});
        let deadline = Duration::from_millis(60);
        let hold = Duration::from_millis(250);
        let samples = 7;
        warm_play(&game.profile).await.unwrap();
        let mut completions = [0; 2];
        for (phase, blocked) in [false, true].into_iter().enumerate() {
            let mut elapsed = Vec::new();
            for _ in 0..samples {
                let benchmark = if blocked {
                    let (entered, ready) = tokio::sync::oneshot::channel();
                    let benchmark = tokio::spawn(crate::engine::benchmark_exclusive(async move {
                        entered.send(()).unwrap();
                        tokio::time::sleep(hold).await;
                    }));
                    ready.await.unwrap();
                    Some(benchmark)
                } else {
                    None
                };
                let start = std::time::Instant::now();
                let result =
                    super::request(&game.profile, identity.clone(), deadline, Slot::Play).await;
                elapsed.push(start.elapsed().as_secs_f64() * 1000.0);
                match result {
                    Ok(_) => completions[phase] += 1,
                    Err(error) => assert!(error.to_string().contains("timed out"), "{error:#}"),
                }
                if let Some(benchmark) = benchmark {
                    benchmark.await.unwrap();
                }
            }
            elapsed.sort_by(f64::total_cmp);
            eprintln!(
                "benchmark gate held: {blocked}, completed: {}/{samples}, median: {:.2} ms, range: {:.2}–{:.2} ms",
                completions[phase],
                elapsed[samples / 2],
                elapsed[0],
                elapsed[samples - 1],
            );
        }
        let pid = game.pid();
        drop(PLAY.lock().await.take());
        assert_reaped(&pid).await;
        assert_eq!(completions, [samples, 0]);
    }

    #[tokio::test]
    async fn gnubg_calculation_occupies_the_same_budget_as_a_sage_probe() {
        let _serial = TEST_LOCK.lock().await;
        let work = tempfile::tempdir().unwrap();
        let binary = work.path().join("blocking-gnubg");
        let capacity = HostCpuBudget::host().await.capacity();
        std::fs::write(&binary, format!(
            "#!/bin/sh\ntouch started\nwhile [ ! -f release ]; do sleep 0.01; done\necho '{capacity} calculation threads.'\n"
        )).unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let gnubg = crate::engine::Engine::new(binary.to_string_lossy().into())
            .with_threads(Some(i32::try_from(capacity).unwrap()));
        let check = FakeWorker::new("healthy");
        crate::engine::with_shared_access(async {
            let observe = async {
                tokio::time::timeout(Duration::from_secs(5), async {
                    while !work.path().join("started").exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                let probe = probe(&check.profile);
                tokio::pin!(probe);
                assert!(
                    tokio::time::timeout(Duration::from_millis(100), &mut probe)
                        .await
                        .is_err(),
                    "a foreground probe bypassed the active full-width GNUbg calculation"
                );
                assert!(!check.directory.path().join("pid").exists());
                std::fs::write(work.path().join("release"), "").unwrap();
                probe.await.unwrap();
            };
            let (result, ()) = tokio::join!(
                gnubg.run_with_timeout(work.path(), "", Duration::from_secs(6)),
                observe
            );
            result.unwrap();
        })
        .await;
    }

    #[tokio::test]
    async fn waiting_requests_share_budget_between_verified_evaluations() {
        let _serial = TEST_LOCK.lock().await;
        let capacity = HostCpuBudget::host().await.capacity();
        crate::engine::with_shared_access(async {
            for (slot, threads, waits) in [Slot::Play, Slot::Interactive]
                .into_iter()
                .flat_map(|slot| {
                    [
                        (slot, 2, capacity < 4),
                        (slot, capacity.max(2), true),
                        (slot, capacity.saturating_add(1).max(2), true),
                    ]
                })
            {
                let mut analysis = FakeWorker::new("batch_gated");
                analysis.profile.threads = threads;
                let play = FakeWorker::new("healthy");
                *slot.worker().lock().await = Some(Worker::initialize(&play.profile).await.unwrap());
                let pid = play.pid();
                let mut batch = VerifiedBatch::begin(&analysis.profile).await.unwrap();
                let checker = json!({"operation":"checker", "force_board":vec![0; 26]});
                let first_checker = checker.clone();
                let evaluation = tokio::spawn(async move {
                    batch
                        .request_costed(first_checker, Duration::from_secs(5))
                        .await
                        .unwrap();
                    batch
                });
                tokio::time::timeout(Duration::from_secs(3), async {
                    while !analysis.directory.path().join("checker_started").exists() {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                let identity = json!({"operation":"identity"});
                let timeout = if waits {
                    Duration::from_millis(200)
                } else {
                    Duration::from_secs(3)
                };
                let play_result =
                    super::request(&play.profile, identity.clone(), timeout, slot).await;
                let started = play.directory.path().join("started").exists();
                let queued = super::request(
                    &play.profile,
                    identity,
                    Duration::from_secs(3),
                    slot,
                );
                tokio::pin!(queued);
                if waits {
                    std::future::poll_fn(|cx| {
                        assert!(queued.as_mut().poll(cx).is_pending());
                        std::task::Poll::Ready(())
                    })
                    .await;
                }
                std::fs::write(analysis.directory.path().join("checker_release"), "").unwrap();
                let mut batch = evaluation.await.unwrap();
                let play_finished = std::cell::Cell::new(false);
                // Poll the next analysis exchange first. It must respect the
                // play request already queued behind the previous exchange.
                tokio::join!(
                    biased;
                    async {
                        batch.request_costed(checker, Duration::from_secs(5)).await.unwrap();
                        if waits {
                            assert!(play_finished.get(), "A new analysis bypassed the waiting request");
                        }
                    },
                    async {
                        queued.await.unwrap();
                        play_finished.set(true);
                    },
                );
                batch.finish().await.unwrap();
                assert_eq!(play.pid(), pid, "Waiting displaced the warm worker");
                drop(slot.worker().lock().await.take());
                drop(GENERAL.lock().await.take());
                assert_reaped(&pid).await;
                assert_reaped(&analysis.pid()).await;
                assert_eq!(
                    started, !waits,
                    "{threads} analysis threads, capacity {capacity}"
                );
                if waits {
                    assert!(play_result.unwrap_err().to_string().contains("timed out"));
                } else {
                    play_result.unwrap();
                }
            }
        })
        .await;
    }

    #[tokio::test]
    #[ignore = "requires a locked NeonGammon installation and an isolated CPU benchmark"]
    async fn benchmark_play_with_analysis_thread_budget() {
        use super::super::SageEngine;
        use sha2::{Digest, Sha256};

        let _serial = TEST_LOCK.lock().await;
        let python = std::env::var("NEONDOJO_SAGE_PYTHON").unwrap();
        let capacity = HostCpuBudget::host().await.capacity();
        assert!(capacity >= 6, "Benchmark needs at least six logical CPUs");
        let profile = SageEngine::probe_profile(python, 3, 2).await.unwrap();
        let play = SageEngine::for_play(profile.clone()).unwrap();
        let sgf = include_str!("../../tests/fixtures/WC2014.gnubg.sgf");
        let imported = crate::import::sgf::parse_sgf(sgf).unwrap();
        let game = 3;
        let positions: Vec<_> = imported
            .positions
            .iter()
            .filter(|position| position.game_no == game)
            .take(20)
            .collect();
        assert_eq!(positions.len(), 20);
        play.warm_play().await.unwrap();
        for threads in [4, capacity] {
            let mut analysis_profile = profile.clone();
            analysis_profile.threads = threads;
            let analysis = SageEngine::new(analysis_profile).unwrap();
            analysis.version().await.unwrap();
            let (background, (mut wall, mut engine_wall, signature)) = tokio::join!(
                async {
                    let start = std::time::Instant::now();
                    let mut signature = Sha256::new();
                    for _ in 0..2 {
                        let result = analysis.analyze_game(sgf, game, 3).await.unwrap();
                        signature.update(serde_json::to_vec(&result.decisions).unwrap());
                    }
                    (
                        start.elapsed().as_secs_f64(),
                        format!("{:x}", signature.finalize()),
                    )
                },
                async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let mut wall = Vec::new();
                    let mut engine_wall = Vec::new();
                    let mut signature = Sha256::new();
                    for position in &positions {
                        let start = std::time::Instant::now();
                        let (candidates, cost) = play
                            .checker_boards_costed(
                                &position.position_id,
                                &position.match_id,
                                None,
                                None,
                            )
                            .await
                            .unwrap();
                        wall.push(start.elapsed().as_secs_f64() * 1000.);
                        engine_wall.push(cost.unwrap().wall_us as f64 / 1000.);
                        signature.update(serde_json::to_vec(&candidates).unwrap());
                    }
                    (wall, engine_wall, format!("{:x}", signature.finalize()))
                }
            );
            wall.sort_by(f64::total_cmp);
            engine_wall.sort_by(f64::total_cmp);
            let median = (wall[9] + wall[10]) / 2.;
            let engine_median = (engine_wall[9] + engine_wall[10]) / 2.;
            eprintln!(
                "{threads}+2 configured threads, capacity {capacity}: play median {median:.2} ms, p95 {:.2} ms, engine median {engine_median:.2} ms, p95 {:.2} ms, analyses {:.2} s; play {signature}; analysis {}",
                wall[18], engine_wall[18], background.0, background.1
            );
        }
    }
}
