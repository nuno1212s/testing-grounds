//! Shared test helpers. The `protocol_test!` matrix macro lands here in M3; for now
//! this just provides tracing setup.

use std::sync::Once;

static TRACING: Once = Once::new();

/// Initialize a tracing subscriber once per process. Controlled by `RUST_LOG`
/// (defaults to `warn`). Safe to call from every test.
pub fn init_tracing() {
    TRACING.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
            )
            .with_test_writer()
            .try_init();
    });
}
