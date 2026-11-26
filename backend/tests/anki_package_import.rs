use neondojo::learning::anki_flashcards::import_rendered_package;
use sqlx::PgPool;
use std::io::Write;

#[sqlx::test(migrations = "./migrations")]
async fn oversized_rendered_card_rolls_back_the_package(pool: PgPool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path();
    std::fs::create_dir(path.join("media")).unwrap();
    let write_manifest = |package: &str, deck: &str, count: usize| {
        let manifest = serde_json::json!({
            "package": package, "decks": [{"name": deck, "cards": count}]
        });
        std::fs::write(path.join("manifest.json"), manifest.to_string()).unwrap();
    };
    let card = |deck: &str, guid: &str, front: &str| {
        serde_json::json!({
            "deck": deck, "guid": guid, "template_number": 1, "prompt": "prompt",
            "tags": [], "css": "", "front": front, "back": "answer"
        })
        .to_string()
    };
    write_manifest(&"a".repeat(64), "Original", 1);
    std::fs::write(path.join("media/old.gif"), b"GIF89a-old").unwrap();
    std::fs::write(
        path.join("cards.jsonl"),
        card("Original", "shared", "original"),
    )
    .unwrap();
    import_rendered_package(&pool, path).await.unwrap();
    sqlx::query("UPDATE learning_cards SET review_count=7, due_at='2030-01-01' WHERE signature='anki:shared:1'")
        .execute(&pool).await.unwrap();

    write_manifest(&"b".repeat(64), "Replacement", 2);
    std::fs::write(path.join("media/new.gif"), b"GIF89a-new").unwrap();
    let mut file = std::fs::File::create(path.join("cards.jsonl")).unwrap();
    writeln!(file, "{}", card("Replacement", "shared", "changed")).unwrap();
    write!(
        file,
        "{}{}",
        " ".repeat(32 * 1024 * 1024),
        card("Replacement", "new", "new")
    )
    .unwrap();
    drop(file);
    let error = import_rendered_package(&pool, path).await.unwrap_err();
    assert_eq!(error.to_string(), "Anki card record is too large");

    let rows: Vec<(String, String, i32, bool)> = sqlx::query_as(
        "SELECT d.name, f.front, c.review_count, c.due_at='2030-01-01'
         FROM decks d JOIN deck_cards dc ON dc.deck_id=d.id
         JOIN learning_cards c ON c.id=dc.card_id JOIN anki_card_faces f ON f.card_id=c.id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows, vec![("Original".into(), "original".into(), 7, true)]);
    let decks: Vec<String> = sqlx::query_scalar("SELECT source_name FROM anki_source_decks")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(decks, ["Original"]);
    let images: Vec<(String, Vec<u8>)> = sqlx::query_as("SELECT name,content FROM anki_media")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(images, vec![("old.gif".into(), b"GIF89a-old".to_vec())]);
}
