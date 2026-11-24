use anyhow::{Context, Result};
use std::process::Command;

use crate::CargoFeatures;

/// Builds the desktop binary of the current project.
pub fn build(release: bool, features: &CargoFeatures) -> Result<()> {
    let project_root = crate::config::find_project_root()?;

    println!("Building for Desktop...");
    println!("  Mode: {}", if release { "release" } else { "debug" });
    if !features.is_empty() {
        println!("  Cargo: {}", features.summary());
    }

    let mut args = vec!["build".to_string()];
    if release {
        args.push("--release".to_string());
    }
    features.apply(&mut args);

    let status = Command::new("cargo")
        .args(&args)
        .current_dir(&project_root)
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        anyhow::bail!("cargo build failed");
    }

    println!("\nBuild successful!");
    Ok(())
}

/// Runs the desktop binary of the current project.
pub fn run(release: bool, features: &CargoFeatures) -> Result<()> {
    let project_root = crate::config::find_project_root()?;

    println!("Running Desktop app...");
    println!("  Mode: {}", if release { "release" } else { "debug" });
    if !features.is_empty() {
        println!("  Cargo: {}", features.summary());
    }

    let mut args = vec!["run".to_string()];
    if release {
        args.push("--release".to_string());
    }
    features.apply(&mut args);

    let status = Command::new("cargo")
        .args(&args)
        .current_dir(&project_root)
        .status()
        .context("Failed to run cargo run")?;

    if !status.success() {
        anyhow::bail!("cargo run failed");
    }

    Ok(())
}
