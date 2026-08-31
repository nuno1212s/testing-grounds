//! Executor variant selection.
//!
//! The benchmark compares four executors. Which one is compiled in is a build-time choice,
//! because the executor is fixed by a type alias rather than a runtime value.
//!
//! Everything the variants share -- the application, the workload generator, the protocol
//! stack, the configuration -- lives outside this module and is compiled identically for all
//! of them. That is deliberate: this harness exists to compare executors, so anything that
//! could drift between variants would quietly invalidate the comparison. Each variant
//! contributes only the three items re-exported below.
//!
//! This file is the only place in the crate that mentions the variant feature flags.
//!
//! | Feature | Module | Executor |
//! |---|---|---|
//! | `baseline` | [`baseline`] | post-commit execution (the comparison baseline) |
//! | `dual_state` | [`dual_state`] | speculative, two full state copies |
//! | `crud_single` | [`crud_single`] | speculative, single-threaded cache/delta |
//! | `crud_scalable` | [`crud_scalable`] | speculative, parallel cache/delta |

#[cfg(not(any(
    feature = "baseline",
    feature = "dual_state",
    feature = "crud_single",
    feature = "crud_scalable"
)))]
compile_error!(
    "no executor variant selected: enable exactly one of \
     `baseline`, `dual_state`, `crud_single`, or `crud_scalable`"
);

#[cfg(any(
    all(
        feature = "baseline",
        any(
            feature = "dual_state",
            feature = "crud_single",
            feature = "crud_scalable"
        )
    ),
    all(
        feature = "dual_state",
        any(feature = "crud_single", feature = "crud_scalable")
    ),
    all(feature = "crud_single", feature = "crud_scalable"),
))]
compile_error!(
    "more than one executor variant selected; they are mutually exclusive. Cargo features are \
     additive and `crud_single` is the default, so a non-default variant needs \
     --no-default-features, e.g. \
     `cargo build --release --no-default-features --features crud_scalable`"
);

#[cfg(feature = "baseline")]
mod baseline;
#[cfg(feature = "baseline")]
pub use baseline::{Executor, NAME, metrics};

#[cfg(feature = "dual_state")]
mod dual_state;
#[cfg(feature = "dual_state")]
pub use dual_state::{Executor, NAME, metrics};

#[cfg(feature = "crud_single")]
mod crud_single;
#[cfg(feature = "crud_single")]
pub use crud_single::{Executor, NAME, metrics};

#[cfg(feature = "crud_scalable")]
mod crud_scalable;
#[cfg(feature = "crud_scalable")]
pub use crud_scalable::{Executor, NAME, metrics};
