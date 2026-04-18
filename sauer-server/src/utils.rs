/// Sets up a beautiful default terminal logger.
/// Call this at the very beginning of `main()` if you don't want to set up your own.
pub fn init_default_tracing() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    // 1. Try to get the filter from the `RUST_LOG` environment variable.
    // 2. If the variable is missing or empty, fall back to our default.
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("sauer_server=debug"));

    // Only try to init if one hasn't been set yet
    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_ansi(true))
        .with(filter)
        .try_init();
}
