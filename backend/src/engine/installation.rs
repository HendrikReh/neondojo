//! Bind one GNUbg process to the executable that was checked for it. Analysis
//! runs (`Engine::run_observed`) and play rules processes share this check.
use super::{Engine, resolve_binary};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, time::Duration};
use tokio_util::sync::CancellationToken;

const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The resolved executable and its SHA-256, read before a process starts and
/// again after it ends: a replacement is detected even when the new
/// executable reports the same version. A direct native binary reports its
/// version in the startup banner of the process itself. A wrapper may launch a
/// different executable whose version changes while the wrapper stays
/// identical, so wrappers keep the separate `--version` checks before and after.
pub(crate) struct Installation {
    identity: Identity,
    wrapper_version: Option<String>,
}

#[derive(PartialEq, Eq)]
struct Identity {
    executable: String,
    digest: [u8; 32],
    direct_native: bool,
}

impl Identity {
    async fn read(engine: &Engine) -> Result<Self> {
        let binary = engine.binary().to_owned();
        let cancel = CancellationToken::new();
        let _cancel_on_drop = cancel.clone().drop_guard();
        let read = tokio::task::spawn_blocking(move || {
            let executable =
                resolve_binary(&binary).context("Could not start GNUbg; check GNUBG_BIN")?;
            let mut file = File::open(&executable)?;
            ensure!(
                file.metadata()?.is_file(),
                "GNUbg executable is not a regular file"
            );
            let mut magic = [0; 4];
            file.read_exact(&mut magic)?;
            let direct_native = cfg!(target_os = "macos")
                && matches!(
                    magic,
                    [0xcf, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe] | [0xca, 0xfe, 0xba, 0xbf]
                );
            let mut hash = Sha256::new();
            hash.update(magic);
            let mut chunk = vec![0; 64 * 1024];
            loop {
                // A running blocking task can outlive its caller or the timeout.
                ensure!(
                    !cancel.is_cancelled(),
                    "GNUbg executable verification cancelled"
                );
                let len = file.read(&mut chunk)?;
                if len == 0 {
                    break;
                }
                hash.update(&chunk[..len]);
            }
            Ok(Self {
                executable,
                digest: hash.finalize().into(),
                direct_native,
            })
        });
        tokio::time::timeout(READ_TIMEOUT, read)
            .await
            .context("GNUbg executable verification timed out")??
    }
}

impl Installation {
    /// Read the configured executable and, for a wrapper, its version, which
    /// must equal the saved version. Callers already hold engine access.
    pub(crate) async fn bind(engine: &Engine) -> Result<Self> {
        let identity = Identity::read(engine).await?;
        let wrapper_version = if identity.direct_native {
            None
        } else {
            Some(engine.version_unlocked(&identity.executable).await?)
        };
        Ok(Self {
            identity,
            wrapper_version,
        })
    }

    /// The checked executable, which the caller starts.
    pub(crate) fn executable(&self) -> &str {
        &self.identity.executable
    }

    /// Whether the startup banner is the only version check of this process.
    pub(crate) fn reports_version_at_startup(&self) -> bool {
        self.identity.direct_native
    }

    /// Check the first line the started process printed: the banner of the
    /// saved version and, for a wrapper, of the version it reported.
    pub(crate) fn check_startup_banner(&self, engine: &Engine, line: &[u8]) -> Result<()> {
        let banner = String::from_utf8_lossy(line);
        let banner = banner.trim_end();
        ensure!(
            banner.starts_with("GNU Backgammon "),
            "GNUbg startup version missing"
        );
        engine.validate_saved_version(banner)?;
        if let Some(expected) = &self.wrapper_version {
            ensure!(expected == banner, "GNUbg version changed at startup");
        }
        Ok(())
    }

    /// After the process ended: a wrapper still reports its version and the
    /// configured path still resolves to the same executable bytes.
    pub(crate) async fn recheck(&self, engine: &Engine) -> Result<()> {
        if let Some(expected) = &self.wrapper_version {
            ensure!(
                engine.version_unlocked(&self.identity.executable).await? == *expected,
                "GNUbg version changed after start"
            );
        }
        let current = Identity::read(engine)
            .await
            .context("Cannot verify the GNUbg installation")?;
        ensure!(
            current == self.identity,
            "GNUbg installation changed after start"
        );
        Ok(())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// A Mach-O header the check must classify without ever executing it.
    fn direct_binary() -> (tempfile::TempDir, Engine) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("gnubg");
        std::fs::write(&path, [0xcf, 0xfa, 0xed, 0xfe, 1, 2, 3, 4]).unwrap();
        let engine = Engine::new(path.to_str().unwrap().into())
            .with_expected_version(Some("GNU Backgammon 1.08.003".into()));
        (directory, engine)
    }

    #[tokio::test]
    async fn direct_binary_is_hashed_and_proves_its_version_by_its_banner() {
        let (_directory, engine) = direct_binary();
        // The file is not executable: binding must not run `--version`.
        let installation = Installation::bind(&engine).await.unwrap();
        let digest = sha2::digest::Output::<Sha256>::from(installation.identity.digest);
        assert_eq!(
            format!("{digest:x}"),
            "e3d2802102794847801e55bcb42dac280bc0f04e1225ff5329a4497951b58648"
        );
        assert!(installation.reports_version_at_startup());
        installation
            .check_startup_banner(&engine, b"GNU Backgammon 1.08.003\n")
            .unwrap();
        for (banner, expected) in [
            (
                &b"GNU Backgammon 1.09.000"[..],
                "found 'GNU Backgammon 1.09.000'",
            ),
            (
                b"Copyright (C) 1999-2004 Gary Wong.",
                "startup version missing",
            ),
            (b"", "startup version missing"),
        ] {
            let error = installation
                .check_startup_banner(&engine, banner)
                .unwrap_err();
            assert!(format!("{error:#}").contains(expected), "{error:#}");
        }
        installation.recheck(&engine).await.unwrap();
    }

    /// Run the three measurements in rotating order on the same native executable.
    #[tokio::test]
    #[ignore = "timing benchmark requires native GNUbg and its weights"]
    async fn benchmark_native_installation_checks() {
        use std::time::Instant;

        let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
        let version = engine.version().await.unwrap();
        let pinned = engine.clone().with_expected_version(Some(version.clone()));
        let mut timings = [Vec::new(), Vec::new(), Vec::new()];
        for round in 0..33 {
            for offset in 0..3 {
                let case = (round + offset) % 3;
                let start = Instant::now();
                match case {
                    0 => {
                        let installation = Installation::bind(&pinned).await.unwrap();
                        assert!(installation.reports_version_at_startup());
                        installation.recheck(&pinned).await.unwrap();
                    }
                    1 => assert_eq!(pinned.check_installation().await.unwrap(), version),
                    _ => assert_eq!(engine.check_installation().await.unwrap(), version),
                }
                if round >= 3 {
                    timings[case].push(start.elapsed());
                }
            }
        }
        for (label, mut samples) in ["bind and recheck", "pinned probe", "unpinned probe"]
            .into_iter()
            .zip(timings)
        {
            samples.sort_unstable();
            eprintln!(
                "{label}, 30 samples: median {:?}, p90 {:?}",
                (samples[14] + samples[15]) / 2,
                samples[26]
            );
        }
    }

    #[tokio::test]
    async fn recheck_resolves_the_configured_symlink_again() {
        use std::os::unix::fs::symlink;

        let (directory, target) = direct_binary();
        let configured = directory.path().join("current");
        let replacement = directory.path().join("same-bytes");
        std::fs::copy(target.binary(), &replacement).unwrap();
        symlink(target.binary(), &configured).unwrap();
        let engine = Engine::new(configured.to_str().unwrap().to_owned());
        let installation = Installation::bind(&engine).await.unwrap();
        std::fs::remove_file(&configured).unwrap();
        symlink(replacement, configured).unwrap();
        let error = installation.recheck(&engine).await.unwrap_err();
        assert!(
            error.to_string().contains("installation changed"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn same_size_rewrite_and_atomic_replacement_are_detected() {
        let (directory, engine) = direct_binary();
        let path = directory.path().join("gnubg");
        let installation = Installation::bind(&engine).await.unwrap();
        std::fs::write(&path, [0xcf, 0xfa, 0xed, 0xfe, 1, 2, 3, 5]).unwrap();
        let error = installation.recheck(&engine).await.unwrap_err();
        assert!(
            error.to_string().contains("installation changed"),
            "{error:#}"
        );

        let installation = Installation::bind(&engine).await.unwrap();
        let replacement = directory.path().join("replacement");
        std::fs::write(&replacement, [0xcf, 0xfa, 0xed, 0xfe, 9]).unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        let error = installation.recheck(&engine).await.unwrap_err();
        assert!(
            error.to_string().contains("installation changed"),
            "{error:#}"
        );

        let installation = Installation::bind(&engine).await.unwrap();
        std::fs::remove_file(&path).unwrap();
        let error = installation.recheck(&engine).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Cannot verify the GNUbg installation"),
            "{error:#}"
        );
    }
}
