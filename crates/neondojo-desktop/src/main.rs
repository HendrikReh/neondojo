use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "neondojo=info".into()),
        )
        .init();

    neondojo_desktop::run();
}
