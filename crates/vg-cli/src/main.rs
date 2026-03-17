use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .without_time()
        .init();

    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let code = vg_core::run_cli(&args)?;
    std::process::exit(code);
}
