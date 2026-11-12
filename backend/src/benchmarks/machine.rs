use super::Machine;
use sqlx::PgPool;
use uuid::Uuid;

fn command(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && !value.is_empty()).then_some(value)
}
fn collect(id: Uuid) -> Machine {
    let name = command("hostname", &[]).unwrap_or_else(|| "Unknown backend".into());
    let os_version = if cfg!(target_os = "macos") {
        command("sw_vers", &["-productVersion"])
    } else {
        command("uname", &["-r"])
    };
    let cpu_model = if cfg!(target_os = "macos") {
        command("sysctl", &["-n", "machdep.cpu.brand_string"])
    } else {
        std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("model name")
                    .and_then(|l| l.split_once(':'))
                    .map(|(_, v)| v.trim().to_owned())
            })
        })
    };
    let memory_bytes = if cfg!(target_os = "macos") {
        command("sysctl", &["-n", "hw.memsize"]).and_then(|s| s.parse().ok())
    } else {
        std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("MemTotal:")
                    .and_then(|v| v.split_whitespace().next())
                    .and_then(|v| v.parse::<i64>().ok())
                    .and_then(|v| v.checked_mul(1024))
            })
        })
    };
    Machine {
        id,
        name,
        os: format!(
            "{} {}",
            std::env::consts::OS,
            os_version.unwrap_or_default()
        )
        .trim()
        .into(),
        architecture: std::env::consts::ARCH.into(),
        cpu_model,
        logical_cpus: crate::configuration::available_cpu_threads(),
        memory_bytes,
    }
}
pub(super) async fn snapshot(pool: &PgPool) -> anyhow::Result<Machine> {
    sqlx::query(
        "INSERT INTO benchmark_machine(singleton,id) VALUES(true,$1) ON CONFLICT DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .execute(pool)
    .await?;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM benchmark_machine WHERE singleton=true")
        .fetch_one(pool)
        .await?;
    Ok(tokio::task::spawn_blocking(move || collect(id)).await?)
}

/// Read an existing installation identity without creating metadata on a GET.
pub(super) async fn current(pool: &PgPool) -> anyhow::Result<Option<Machine>> {
    let id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM benchmark_machine WHERE singleton=true")
            .fetch_optional(pool)
            .await?;
    match id {
        Some(id) => Ok(Some(
            tokio::task::spawn_blocking(move || collect(id)).await?,
        )),
        None => Ok(None),
    }
}
