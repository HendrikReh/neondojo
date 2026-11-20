use neondojo::learning::anki_flashcards::import_rendered_package;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::from_path(".env").ok();
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://neondojo:neondojo_local@127.0.0.1:54329/neondojo".into());
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    for directory in std::env::args_os().skip(1) {
        for (name, count) in
            import_rendered_package(&pool, std::path::Path::new(&directory)).await?
        {
            println!("{count}\t{name}");
        }
    }
    Ok(())
}
