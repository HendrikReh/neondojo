//! Optional native MAT regression coverage without a database or private fixtures.
use neondojo::{
    engine::Engine,
    import::{mat::normalize_mat, parse_sgf},
};
use std::path::{Path, PathBuf};

#[tokio::test]
#[ignore = "requires GNUbg and the optional local games/mat corpus"]
async fn local_mat_corpus_normalizes_to_strictly_replayable_sgf() {
    let root = std::env::var_os("NEONDOJO_GAMES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("games")
        });
    let directory = root.join("mat");
    if !directory.is_dir() {
        eprintln!("SKIP optional MAT corpus: {}", directory.display());
        return;
    }
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let mut files = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("mat"))
        })
        .collect::<Vec<_>>();
    files.sort();
    assert!(!files.is_empty());
    let mut failures = Vec::new();
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let imported = match normalize_mat(&engine, &bytes)
            .await
            .and_then(|sgf| parse_sgf(&sgf))
        {
            Ok(imported) => imported,
            Err(error) => {
                failures.push(format!("{}: {error:#}", path.display()));
                continue;
            }
        };
        println!(
            "PASS {}: {} checker decisions",
            path.file_name().unwrap().to_string_lossy(),
            imported.positions.len()
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
