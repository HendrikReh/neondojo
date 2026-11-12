//! Retain GNUbg only for one atomic Sage action chain; never pool rule state.
use super::{NativeRequest, NativeSnapshot};
use crate::{
    engine::{Engine, resolve_binary},
    engine_process::terminate_and_reap,
};
use anyhow::{Context, Result, ensure};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    task::JoinHandle,
};

const FRAME_LIMIT: usize = 65536;
const TIMEOUT: Duration = Duration::from_secs(180);

pub(super) struct RulesProcess {
    directory: tempfile::TempDir,
    engine: Engine,
    version: String,
    child: Option<Child>,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    diagnostics: Option<JoinHandle<Result<Vec<u8>>>>,
    sequence: u32,
}

async fn read_frame(reader: &mut BufReader<ChildStdout>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take((FRAME_LIMIT + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .await?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= FRAME_LIMIT && bytes.last() == Some(&b'\n'),
        "GNUbg rule frame missing or exceeds limit"
    );
    Ok(bytes)
}

impl RulesProcess {
    pub(super) async fn start(engine: &Engine, sgf: Option<&str>) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        if let Some(sgf) = sgf {
            tokio::fs::write(directory.path().join("input.sgf"), sgf).await?;
        }
        for (name, content) in [
            ("adapter.py", include_str!("native.py")),
            ("rules_worker.py", include_str!("rules_worker.py")),
            ("commands.txt", "load python rules_worker.py\n"),
        ] {
            tokio::fs::write(directory.path().join(name), content).await?;
        }
        let version = engine.version().await?;
        let mut child = Command::new(resolve_binary(engine.binary())?)
            .args(["-t", "-q", "-r", "-c", "commands.txt"])
            .current_dir(directory.path())
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let input = child.stdin.take().context("Missing GNUbg stdin")?;
        let output = BufReader::new(child.stdout.take().context("Missing GNUbg stdout")?);
        let stderr = child.stderr.take().context("Missing GNUbg stderr")?;
        // Retain bounded diagnostics; native errors must never become success.
        let diagnostics = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stderr
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .await?;
            ensure!(
                bytes.len() <= 8 * 1024 * 1024,
                "GNUbg diagnostics exceed limit"
            );
            Ok(bytes)
        });
        let mut process = Self {
            directory,
            engine: engine.clone(),
            version,
            child: Some(child),
            input: Some(input),
            output,
            diagnostics: Some(diagnostics),
            sequence: 0,
        };
        tokio::time::timeout(TIMEOUT, async {
            let mut preamble = 0;
            loop {
                let line = read_frame(&mut process.output).await?;
                preamble += line.len();
                ensure!(preamble <= FRAME_LIMIT, "GNUbg rule startup exceeds limit");
                if serde_json::from_slice::<serde_json::Value>(&line).ok()
                    == Some(serde_json::json!({"ready": true}))
                {
                    break;
                }
            }
            anyhow::Ok(())
        })
        .await
        .context("GNUbg rule startup timed out")??;
        Ok(process)
    }

    // Consuming ownership ensures cancellation kills the process immediately,
    // even if a caller abandons just this request future.
    pub(super) async fn execute(
        mut self,
        request: &NativeRequest<'_>,
    ) -> Result<(Self, NativeSnapshot, String)> {
        let (snapshot, sgf) = tokio::time::timeout(TIMEOUT, async {
            ensure!(self.sequence < 64, "GNUbg rule action chain exceeds limit");
            self.sequence += 1;
            let mut frame =
                serde_json::to_vec(&serde_json::json!({"id": self.sequence, "request": request}))?;
            frame.push(b'\n');
            ensure!(
                frame.len() <= FRAME_LIMIT,
                "GNUbg rule request exceeds limit"
            );
            let input = self.input.as_mut().context("GNUbg rules input closed")?;
            input.write_all(&frame).await?;
            input.flush().await?;
            let reply: serde_json::Value =
                serde_json::from_slice(&read_frame(&mut self.output).await?)?;
            ensure!(
                reply.get("id").and_then(|v| v.as_u64()) == Some(u64::from(self.sequence)),
                "GNUbg rule response sequence mismatch"
            );
            ensure!(
                reply.get("ok").and_then(|v| v.as_bool()) == Some(true)
                    && reply.get("error").is_none(),
                "GNUbg rule action failed: {}",
                reply.get("error").unwrap_or(&serde_json::Value::Null)
            );
            let output = String::from_utf8(
                super::read_native_artifact(self.directory.path(), "native-output.txt").await?,
            )?;
            super::read_native_result(self.directory.path(), request, &output).await
        })
        .await
        .context("GNUbg rule action timed out")??;
        Ok((self, snapshot, sgf))
    }

    pub(super) async fn finish(mut self) -> Result<()> {
        ensure!(
            self.engine.version().await? == self.version,
            "GNUbg rules version changed before commit"
        );
        let child = self.child.as_mut().context("GNUbg rule child missing")?;
        ensure!(
            child.try_wait()?.is_none(),
            "GNUbg rules exited before completing the action chain"
        );
        // EOF ends the worker and GNUbg normally. Require a successful exit,
        // not just an acknowledgment followed by an unnoticed native crash.
        drop(self.input.take());
        let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .context("GNUbg rule shutdown timed out")??;
        ensure!(
            status.success(),
            "GNUbg rule process failed during shutdown"
        );
        self.child.take();
        if let Some(diagnostics) = self.diagnostics.take() {
            let bytes = tokio::time::timeout(Duration::from_secs(5), diagnostics)
                .await
                .context("GNUbg diagnostics timed out")???;
            ensure!(
                String::from_utf8_lossy(&bytes).trim().is_empty(),
                "GNUbg rule diagnostics: {}",
                String::from_utf8_lossy(&bytes)
            );
        }
        Ok(())
    }
}

impl Drop for RulesProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            terminate_and_reap(child);
        }
        if let Some(diagnostics) = self.diagnostics.take() {
            diagnostics.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake(mode: &str) -> (tempfile::TempDir, Engine) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rules");
        let script = format!(
            r#"#!/usr/bin/env python3
import os,sys,json,time
if '--version' in sys.argv:
 print('GNU Backgammon test');sys.exit(0)
open({pid:?},'w').write(str(os.getpid()))
print('{{"ready":true}}',flush=True)
sys.stdin.readline()
mode={mode:?}
if mode=='wrong': print('{{"id":2,"ok":true}}',flush=True)
elif mode=='oversize': print('x'*65537,flush=True)
elif mode=='malformed': print('invalid-json',flush=True)
elif mode=='exit': sys.exit(2)
elif mode in ('premature', 'nonzero', 'diagnostic', 'clean'):
 with open('snapshot.json','w') as f: f.write({snapshot:?})
 with open('output.sgf','w') as f: f.write('checkpoint')
 with open('native-output.txt','w') as f: f.write('')
 print('{{"id":1,"ok":true}}',flush=True)
 if mode=='premature': sys.exit(7)
 if mode=='diagnostic': print('native failure after acknowledgment',file=sys.stderr,flush=True)
 sys.stdin.read()
 sys.exit(7 if mode=='nonzero' else 0)
time.sleep(60)
"#,
            pid = directory.path().join("pid").to_str().unwrap(),
            snapshot = serde_json::json!({
                "native_id": "checkpoint", "board": vec![0; 26], "player": 0,
                "dice": [], "phase": "roll", "can_double": false,
                "legal_moves": [], "history": [], "scores": [0, 0],
                "game_no": 1, "winner": null,
            })
            .to_string()
        );
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let engine = Engine::new(path.to_str().unwrap().into());
        (directory, engine)
    }

    fn request() -> NativeRequest<'static> {
        NativeRequest {
            new: false,
            external_bot: true,
            accept_resignation: None,
            human_player: 0,
            match_length: 11,
            plies: 0,
            native_id: "checkpoint",
            action: "inspect",
            notation: None,
            resign_level: None,
        }
    }

    async fn assert_reaped(directory: &tempfile::TempDir) {
        let pid: i32 = std::fs::read_to_string(directory.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let status = Command::new("/bin/kill")
                    .args(["-0", &pid.to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .await
                    .unwrap();
                if !status.success() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Rule child must be killed and reaped");
    }

    #[tokio::test]
    async fn invalid_frames_discard_and_reap_rule_process() {
        for mode in ["wrong", "oversize", "malformed", "exit"] {
            let (directory, engine) = fake(mode);
            crate::engine::with_shared_access(async {
                let process = RulesProcess::start(&engine, None).await.unwrap();
                assert!(process.execute(&request()).await.is_err(), "{mode}");
                assert_reaped(&directory).await;
            })
            .await;
        }
    }

    #[tokio::test]
    async fn cancelled_request_discards_and_reaps_rule_process() {
        let (directory, engine) = fake("wait");
        crate::engine::with_shared_access(async {
            let process = RulesProcess::start(&engine, None).await.unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(100), process.execute(&request()))
                    .await
                    .is_err()
            );
            assert_reaped(&directory).await;
        })
        .await;
    }

    #[tokio::test]
    async fn final_acknowledgment_requires_clean_live_shutdown() {
        for (mode, expected_error) in [
            ("premature", Some("exited before completing")),
            ("nonzero", Some("failed during shutdown")),
            ("diagnostic", Some("native failure after acknowledgment")),
            ("clean", None),
        ] {
            let (directory, engine) = fake(mode);
            crate::engine::with_shared_access(async {
                let process = RulesProcess::start(&engine, None).await.unwrap();
                let (mut process, _, _) = process.execute(&request()).await.unwrap();
                if mode == "premature" {
                    // Establish that the fake engine has already exited before
                    // calling finish; this must not depend on process scheduling.
                    tokio::time::timeout(Duration::from_secs(3), async {
                        while process
                            .child
                            .as_mut()
                            .unwrap()
                            .try_wait()
                            .unwrap()
                            .is_none()
                        {
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .unwrap();
                }
                match (process.finish().await, expected_error) {
                    (Err(error), Some(expected)) => {
                        assert!(error.to_string().contains(expected), "{mode}: {error:#}")
                    }
                    (Ok(()), None) => {}
                    (result, _) => panic!("{mode}: unexpected shutdown result {result:?}"),
                }
                assert_reaped(&directory).await;
            })
            .await;
        }
    }
}
