use anyhow::{Context, Result};
use std::io::IsTerminal;
use std::process::Command;

use crate::config;

pub fn run(
    package_name: Option<&str>,
    device_id: Option<&str>,
    skip_build: bool,
    release: bool,
    features: &crate::CargoFeatures,
) -> Result<()> {
    // Check that we're on macOS
    if cfg!(not(target_os = "macos")) {
        anyhow::bail!(
            "iOS deployment requires macOS with Xcode installed.\n\
             This command can only be run on a Mac."
        );
    }

    // First build (unless skipped)
    if !skip_build {
        super::build::build(package_name, release, features)?;
    }

    // `build` already did this; repeat it so `--skip-build` still resolves the
    // app name and bundle ID for projects whose Ignit.toml is missing them.
    config::ensure_ios_config(std::io::stdin().is_terminal())?;

    let ios_config = config::load_ios_config()?;
    let bundle_id = ios_config.bundle_id();
    let app_name = ios_config.app_name();

    // Select and boot simulator
    let sim = super::device::select_simulator(device_id)?;
    super::device::boot_simulator(&sim.udid)?;

    // Bring up the Simulator UI when a desktop session is available. Headless
    // CI or a VM without a window server can still install and launch.
    let _ = Command::new("open").args(["-a", "Simulator"]).status();

    let project_root = config::find_project_root()?;
    let app_path = super::build::app_bundle_path(&project_root, app_name, release);

    if !app_path.exists() {
        let build_command = if release {
            "ignit build ios --release"
        } else {
            "ignit build ios"
        };
        anyhow::bail!(
            "App bundle not found at: {}\n\
             Run '{}' first.",
            app_path.display(),
            build_command
        );
    }

    // Install on simulator
    println!("Installing on simulator '{}'...", sim.name);
    let status = Command::new("xcrun")
        .args(["simctl", "install", &sim.udid])
        .arg(&app_path)
        .status()
        .context("Failed to install app on simulator")?;

    if !status.success() {
        anyhow::bail!("Failed to install app on simulator");
    }

    // Launch the app attached to its stdout/stderr so Rust logs show up in the
    // terminal. `--console-pty` blocks until the app exits.
    println!("Launching app... (Ctrl+C to stop)\n");
    let status = Command::new("xcrun")
        .args([
            "simctl",
            "launch",
            "--console-pty",
            "--terminate-running-process",
            &sim.udid,
            bundle_id,
        ])
        .status()
        .context("Failed to launch app on simulator")?;

    if !status.success() {
        anyhow::bail!("App exited with status {}", status);
    }

    Ok(())
}
