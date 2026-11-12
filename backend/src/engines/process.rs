use super::SageSnapshot;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
};

const MAX_REQUEST: usize = 262_144;
const MAX_RESPONSE: u64 = 2_097_152;
static WORKER: Mutex<Option<Worker>> = Mutex::const_new(None);

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
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            // The task owns the child until wait() reaps it, also on request cancellation.
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    let _ = reap_child(&mut child).await;
                });
            }
        }
    }
}

/// Unlike Child::wait, try_wait does not depend on the runtime that registered
/// the child process. This also works after that runtime's reactor has closed.
async fn reap_child(child: &mut Child) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(5), async {
        while child
            .try_wait()
            .context("Cannot reap Open Sage worker")?
            .is_none()
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    })
    .await
    .context("Timed out reaping Open Sage worker")?
}

impl Worker {
    async fn start(profile: &SageSnapshot) -> Result<Self> {
        let probe = include_str!("../../../tools/probe_open_sage.py");
        let script = format!(
            "integrity = {{'__name__': 'bgtutor_integrity'}}\nexec({}, integrity)\n{}\nmain(integrity)\n",
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
            .context("Cannot start Open Sage Python; install the locked native environment")?;
        let stdin = child.stdin.take().context("Missing Open Sage stdin")?;
        let stdout = BufReader::new(child.stdout.take().context("Missing Open Sage stdout")?);
        let mut stderr = child.stderr.take().context("Missing Open Sage stderr")?;
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
        let identity = worker
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
                .context("Cannot terminate Open Sage worker")?;
            reap_child(child).await?;
        }
        self.child.take();
        Ok(())
    }

    async fn exchange(&mut self, mut request: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id = id.checked_add(1).context("Sidecar request ID exhausted")?;
        request["id"] = json!(id);
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= MAX_REQUEST,
            "Open Sage request exceeds limit"
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
            "Open Sage crashed or exceeded its output limit"
        );
        let response: Value =
            serde_json::from_slice(&line).context("Invalid Open Sage protocol response")?;
        ensure!(
            response.get("id").and_then(Value::as_u64) == Some(id),
            "Open Sage response ID mismatch"
        );
        if let Some(error) = response.get("error").and_then(Value::as_str) {
            anyhow::bail!("Open Sage: {error}");
        }
        response
            .get("result")
            .cloned()
            .context("Open Sage response omitted result")
    }
}

pub(super) async fn request(
    profile: &SageSnapshot,
    payload: Value,
    timeout: Duration,
) -> Result<Value> {
    crate::engine::with_shared_access(async {
        // Worker is removed from the slot while awaiting I/O, so dropping this
        // future always drops/kills its process instead of caching a dirty stream.
        let mut slot = WORKER.lock().await;
        tokio::time::timeout(timeout, async {
            let mut worker = match slot.take() {
                Some(worker)
                    if worker.profile == *profile
                        && worker.runtime_id == tokio::runtime::Handle::current().id() =>
                {
                    worker
                }
                previous => {
                    if let Some(previous) = previous {
                        previous.shutdown().await?;
                    }
                    Worker::initialize(profile).await?
                }
            };
            let result = worker.exchange(payload.clone()).await?;
            // A complete JSON frame is not yet a valid engine response. Keep
            // ownership here until its typed contents have passed validation;
            // every failure then drops, kills and reaps this worker.
            validate_payload(profile, &payload, &result)?;
            *slot = Some(worker);
            Ok(result)
        })
        .await
        .context("Open Sage evaluation timed out; worker terminated")?
    })
    .await
}

pub(super) async fn probe(profile: &SageSnapshot) -> Result<Value> {
    crate::engine::with_shared_access(async {
        let mut slot = WORKER.lock().await;
        tokio::time::timeout(Duration::from_secs(120), async {
            if let Some(previous) = slot.take() {
                previous.shutdown().await?;
            }
            let mut worker = Worker::start(profile).await?;
            worker
                .exchange(json!({"operation":"initialize", "profile":profile}))
                .await
        })
        .await
        .context("Open Sage installation check timed out; worker terminated")?
    })
    .await
}

/// Start a genuinely cold worker while respecting the same application gate.
/// The old child is reaped before the new interpreter is started.
pub(super) async fn restart(profile: &SageSnapshot) -> Result<()> {
    crate::engine::with_shared_access(async {
        let mut slot = WORKER.lock().await;
        tokio::time::timeout(Duration::from_secs(120), async {
            if let Some(previous) = slot.take() {
                previous.shutdown().await?;
            }
            let worker = Worker::initialize(profile).await?;
            *slot = Some(worker);
            Ok(())
        })
        .await
        .context("Open Sage restart timed out; worker terminated")?
    })
    .await
}

fn validate_payload(profile: &SageSnapshot, request: &Value, result: &Value) -> Result<()> {
    match request.get("operation").and_then(Value::as_str) {
        Some("identity") => validate_identity(profile, result),
        Some("checker") => {
            let candidates: Vec<super::SageCheckerCandidate> =
                serde_json::from_value(result.clone())
                    .context("Invalid Open Sage checker response")?;
            ensure!(
                !candidates.is_empty(),
                "Open Sage returned no checker candidates"
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
                            "Open Sage omitted checker notation"
                        );
                    }
                }
            }
            if let Some(forced) = request.get("force_board").filter(|value| !value.is_null()) {
                let forced: [i32; 26] = serde_json::from_value(forced.clone())?;
                ensure!(
                    candidates.iter().any(|candidate| candidate.board == forced),
                    "Open Sage omitted the played candidate"
                );
            }
            Ok(())
        }
        Some("cube") => {
            let cube: super::SageCubeEvaluation = serde_json::from_value(result.clone())
                .context("Invalid Open Sage cube response")?;
            for equity in [cube.no_double, cube.take, cube.pass] {
                super::validate_evaluation(equity, &cube.probabilities)?;
            }
            Ok(())
        }
        Some("resignation") => {
            ensure!(
                result.get("accept").and_then(Value::as_bool).is_some(),
                "Open Sage omitted resignation decision"
            );
            for field in ["continuation", "offered"] {
                ensure!(
                    result
                        .get(field)
                        .and_then(Value::as_f64)
                        .is_some_and(f64::is_finite),
                    "Open Sage returned invalid resignation equity"
                );
            }
            Ok(())
        }
        _ => anyhow::bail!("Unsupported Open Sage response operation"),
    }
}

fn validate_identity(profile: &SageSnapshot, identity: &Value) -> Result<()> {
    ensure!(
        identity["package_version"].as_str() == Some(&profile.package_version)
            && identity["artifact_fingerprint"].as_str() == Some(&profile.artifact_fingerprint),
        "Open Sage installation differs from the immutable job snapshot"
    );
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

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
    if request['operation'] == 'initialize':
        profile = request['profile']
        result = {key: profile[key] for key in ('package_version', 'artifact_fingerprint')}
        print(json.dumps({'id': request['id'], 'result': result}), flush=True)
        continue
    (root / 'started').write_text('ready')
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
                    package_version: "2.0.20260907".into(),
                    artifact_fingerprint: "a".repeat(64),
                    threads: 2,
                    plies: 2,
                    adapter_version: 1,
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
            let cached = WORKER.lock().await.take();
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
            assert!(WORKER.lock().await.is_none());
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
        assert!(WORKER.lock().await.is_none());
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
        assert!(WORKER.lock().await.is_none());
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
        drop(WORKER.lock().await.take());
        assert_reaped(&pid).await;
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
        let cached = WORKER.lock().await.take();
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
            let worker = WORKER.lock().await.take().unwrap();
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
            let worker = WORKER.lock().await.take().unwrap();
            worker.shutdown().await.unwrap();
            assert_reaped(&new_pid).await;
        });
    }
}
