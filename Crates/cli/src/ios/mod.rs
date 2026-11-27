pub mod build;
pub mod device;
pub mod project;
pub mod run;

use anyhow::Result;

/// The Rust target triple for the host's iOS simulator.
pub(crate) fn simulator_target() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x86_64-apple-ios"),
        "aarch64" => Ok("aarch64-apple-ios-sim"),
        other => anyhow::bail!(
            "Unsupported macOS host architecture '{}'. iOS simulator builds \
             need an x86_64 or aarch64 Mac.",
            other
        ),
    }
}

/// The architecture name Xcode expects for the simulator slice.
pub(crate) fn simulator_arch() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x86_64"),
        "aarch64" => Ok("arm64"),
        other => anyhow::bail!(
            "Unsupported macOS host architecture '{}'. iOS simulator builds \
             need an x86_64 or aarch64 Mac.",
            other
        ),
    }
}
