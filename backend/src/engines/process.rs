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
// One worker for playing and one for everything else, each behind its own
// lock. A move therefore never waits for an analysis block; the configured
// play and analysis thread counts share the CPU while both evaluate. Game
// blocks, other requests, probes and restarts stay serial on the general
// slot, and benchmarks still exclude all engine work through the engine gate.
static GENERAL: Mutex<Option<Worker>> = Mutex::const_new(None);
static PLAY: Mutex<Option<Worker>> = Mutex::const_new(None);

#[derive(Clone, Copy)]
pub(super) enum Slot {
    General,
    Play,
}

impl Slot {
    fn worker(self) -> &'static Mutex<Option<Worker>> {
        match self {
            Self::General => &GENERAL,
            Self::Play => &PLAY,
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
            "integrity = {{'__name__': 'neondojo_integrity'}}\nexec({}, integrity)\n{}\nmain(integrity)\n",
            serde_json::to_string(probe)?,
            include_str!("worker.py")
        );
        let mut child = Command::new(&profile.python)
            .args(["-I", "-u", "-c", &script])
            .env("OMP_NUM_THREADS", "1")
            .env("OPENBLAS_NUM_THREADS", "1")
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
            reap_other_runtime(&mut cache).await?;
            let mut worker = Worker::checkout(&mut cache, profile).await?;
            worker
                .validated_exchange(json!({"operation":"begin_batch"}))
                .await?;
            Ok::<_, anyhow::Error>(worker)
        })
        .await
        .context("NeonGammon batch initialization timed out; worker terminated")??;
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
        let mut worker = self.worker.take().context("NeonGammon batch was aborted")?;
        ensure!(
            matches!(payload["operation"].as_str(), Some("checker" | "cube")),
            "Unsupported NeonGammon game operation"
        );
        // Removing ownership also poisons the block if this individual future
        // is cancelled while its caller still retains the batch object.
        let result = tokio::time::timeout(timeout, worker.validated_exchange(payload))
            .await
            .context("NeonGammon evaluation timed out; worker terminated")??;
        self.worker = Some(worker);
        Ok(result)
    }

    pub(super) async fn finish(mut self) -> Result<()> {
        let mut worker = self.worker.take().context("NeonGammon batch was aborted")?;
        tokio::time::timeout(
            Duration::from_secs(120),
            worker.validated_exchange(json!({"operation":"end_batch"})),
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
    crate::engine::with_shared_access(async {
        // The deadline includes waiting for the slot, so a request ends on time
        // even while another request of the same slot holds its worker. The
        // worker is removed from the slot while awaiting I/O, so dropping this
        // future always drops/kills its process instead of caching a dirty stream.
        tokio::time::timeout(timeout, async {
            let mut cache = slot.worker().lock().await;
            reap_other_runtime(&mut cache).await?;
            let mut worker = Worker::checkout(&mut cache, profile).await?;
            let result = worker.validated_exchange(payload).await?;
            *cache = Some(worker);
            Ok(result)
        })
        .await
        .context("NeonGammon request timed out, including the wait for its worker")?
    })
    .await
}

pub(super) async fn probe(profile: &SageSnapshot) -> Result<Value> {
    crate::engine::with_shared_access(async {
        let mut cache = GENERAL.lock().await;
        tokio::time::timeout(Duration::from_secs(120), async {
            reap_other_runtime(&mut cache).await?;
            let mut worker = Worker::start(profile).await?;
            let (identity, _) = worker
                .exchange(json!({"operation":"initialize", "profile":profile}))
                .await?;
            worker.shutdown().await?;
            Ok(identity)
        })
        .await
        .context("NeonGammon installation check timed out; worker terminated")?
    })
    .await
}

/// Start a genuinely cold worker while respecting the same application gate.
/// The old child is reaped before the new interpreter is started.
pub(super) async fn restart(profile: &SageSnapshot) -> Result<()> {
    crate::engine::with_shared_access(async {
        let mut cache = GENERAL.lock().await;
        tokio::time::timeout(Duration::from_secs(120), async {
            reap_other_runtime(&mut cache).await?;
            if let Some(previous) = cache.take() {
                previous.shutdown().await?;
            }
            let worker = Worker::initialize(profile).await?;
            *cache = Some(worker);
            Ok(())
        })
        .await
        .context("NeonGammon restart timed out; worker terminated")?
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
            let worker = Worker::checkout(&mut cache, profile).await?;
            *cache = Some(worker);
            Ok(())
        })
        .await
        .context("NeonGammon play warm-up timed out; worker terminated")?
    })
    .await
}

fn validate_payload(profile: &SageSnapshot, request: &Value, result: &Value) -> Result<()> {
    match request.get("operation").and_then(Value::as_str) {
        Some("identity" | "begin_batch" | "end_batch") => validate_identity(profile, result),
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
                        crate::import::sgf::apply_checker_turn(&absolute, 0, dice, &[])?;
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

fn validate_identity(profile: &SageSnapshot, identity: &Value) -> Result<()> {
    ensure!(
        identity["package_version"].as_str() == Some(&profile.package_version)
            && identity["artifact_fingerprint"].as_str() == Some(&profile.artifact_fingerprint),
        "NeonGammon installation differs from the immutable job snapshot"
    );
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    async fn request(profile: &SageSnapshot, payload: Value, timeout: Duration) -> Result<Value> {
        super::request(profile, payload, timeout, Slot::General).await
    }

    static TEST_LOCK: Mutex<()> = Mutex::const_new(());

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
        if operation == 'checker':
            (root / 'checker_started').write_text('ready')
            if mode == 'batch_block':
                time.sleep(60)
            result = [{'board': request['force_board'], 'notation': '8/5 6/5', 'equity': .2,
                       'probabilities': [.6, .1, .01, .4, .1, .01]}]
            if mode == 'batch_bad_checker':
                del result[0]['probabilities']
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
            "initialize\nbegin_batch\nchecker\nend_batch\nbegin_batch\nchecker\nend_batch\nidentity\n"
        );
    }

    #[tokio::test]
    async fn invalid_batch_frames_and_typed_results_discard_worker() {
        let _serial = TEST_LOCK.lock().await;
        for mode in ["batch_bad_checker", "batch_bad_json"] {
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
}
