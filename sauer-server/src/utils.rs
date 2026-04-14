/// Sets up a beautiful default terminal logger.
/// Call this at the very beginning of `main()` if you don't want to set up your own.
pub fn init_default_tracing() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    // Only try to init if one hasn't been set yet
    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_ansi(true))
        .with(EnvFilter::from_default_env().add_directive("sauer_server=debug".parse().unwrap()))
        .try_init();
}
