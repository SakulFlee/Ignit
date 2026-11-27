use anyhow::{Context, Result};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::CargoFeatures;
use crate::config;

pub fn build(package_name: Option<&str>, release: bool, features: &CargoFeatures) -> Result<()> {
    // Check that we're on macOS (xcodebuild is macOS-only)
    if cfg!(not(target_os = "macos")) {
        anyhow::bail!(
            "iOS builds require macOS with Xcode installed.\n\
             This command can only be run on a Mac."
        );
    }

    let project_root = config::find_project_root()?;
    let ios_dir = project_root.join("iOS");

    let interactive = std::io::stdin().is_terminal();
    config::ensure_ios_config(interactive)?;
    let ios_config = config::load_ios_config()?;

    let package_root = match package_name {
        Some(pkg) => config::find_package_path(pkg)?,
        None => project_root.clone(),
    };
    let lib_name = config::resolve_lib_name(package_name)?;

    let project_file = ios_dir
        .join(format!("{}.xcodeproj", ios_config.app_name()))
        .join("project.pbxproj");
    if !project_file.exists() {
        super::project::init(package_name, false)?;
    }

    println!("Building for iOS simulator...");
    println!("  Package: {}", package_name.unwrap_or(&lib_name));
    println!("  Library: {}", lib_name);
    println!("  Mode: {}", if release { "release" } else { "debug" });
    if !features.is_empty() {
        println!("  Cargo: {}", features.summary());
    }

    // Pick the simulator target that matches the host CPU. The simulator runs
    // natively, so an Intel Mac needs x86_64-apple-ios and an Apple Silicon Mac
    // needs aarch64-apple-ios-sim. Using the device target here produces a
    // static library Xcode cannot link into a simulator app.
    let rust_target = super::simulator_target()?;
    ensure_ios_target(rust_target)?;

    println!("\nBuilding Rust static library...");
    let mut cargo_args = vec![
        "build".to_string(),
        "--lib".to_string(),
        "--target".to_string(),
        rust_target.to_string(),
    ];
    if let Some(pkg) = package_name {
        cargo_args.push("--package".to_string());
        cargo_args.push(pkg.to_string());
    }
    if release {
        cargo_args.push("--release".to_string());
    }
    features.apply(&mut cargo_args);

    let status = Command::new("cargo")
        .args(&cargo_args)
        .current_dir(&project_root)
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        anyhow::bail!("cargo build failed");
    }

    let profile = if release { "release" } else { "debug" };
    let lib_dir = cargo_target_dir(&project_root)
        .join(rust_target)
        .join(profile);

    let lib_path = lib_dir.join(format!("lib{}.a", lib_name));
    if !lib_path.exists() {
        anyhow::bail!(
            "Static library not found at {}.\n\
             Add \"staticlib\" to the [lib] crate-type in Cargo.toml so the iOS \
             app can link the engine.",
            lib_path.display()
        );
    }

    // Assets are copied into the .app bundle root, matching how Android places
    // them at the APK asset root and desktop resolves <cwd>/Assets.
    let asset_dir = package_root.join("Assets");
    let asset_dir = asset_dir.exists().then_some(asset_dir);

    // Build with xcodebuild
    println!("\nBuilding Xcode project...");

    let configuration = if release { "Release" } else { "Debug" };
    let build_root = project_root.join("Build");

    let xcode_args = vec![
        "-project".to_string(),
        ios_dir
            .join(format!("{}.xcodeproj", ios_config.app_name()))
            .to_string_lossy()
            .into_owned(),
        "-target".to_string(),
        ios_config.app_name().to_string(),
        "-configuration".to_string(),
        configuration.to_string(),
        "-sdk".to_string(),
        "iphonesimulator".to_string(),
        "-destination".to_string(),
        "generic/platform=iOS Simulator".to_string(),
        format!("SYMROOT={}", build_root.join("Products").display()),
        format!("OBJROOT={}", build_root.join("Intermediates").display()),
        format!("ARCHS={}", super::simulator_arch()?),
        "ONLY_ACTIVE_ARCH=YES".to_string(),
        "CODE_SIGNING_ALLOWED=NO".to_string(),
        "CODE_SIGNING_REQUIRED=NO".to_string(),
        "IGNIT_SKIP_RUST_BUILD=1".to_string(),
        format!("IGNIT_LIB_DIR={}", lib_dir.display()),
        format!("IGNIT_LIB_NAME={}", lib_name),
        format!(
            "IGNIT_ASSET_DIR={}",
            asset_dir
                .as_deref()
                .map(Path::to_string_lossy)
                .unwrap_or_default()
        ),
        "build".to_string(),
    ];

    let status = Command::new("xcodebuild")
        .args(&xcode_args)
        .current_dir(&project_root)
        .status()
        .context("Failed to run xcodebuild")?;

    if !status.success() {
        anyhow::bail!("xcodebuild failed");
    }

    let app_path = project_root
        .join("Build")
        .join("Products")
        .join(format!("{}-iphonesimulator", configuration))
        .join(format!("{}.app", ios_config.app_name()));

    println!("\nBuild successful!");
    println!("  App: {}", app_path.display());
    println!("  Run: ignit run ios");

    Ok(())
}

/// Resolve Cargo's target directory, honoring `CARGO_TARGET_DIR`.
fn cargo_target_dir(project_root: &Path) -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                project_root.join(dir)
            }
        }
        None => project_root.join("target"),
    }
}

/// Ensure a Rust target is installed, installing it through rustup if needed.
fn ensure_ios_target(target: &str) -> Result<()> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .context("Failed to run 'rustup target list'")?;

    let installed = String::from_utf8_lossy(&output.stdout);
    if installed.lines().any(|line| line.trim() == target) {
        return Ok(());
    }

    println!("Installing iOS target {}...", target);
    let status = Command::new("rustup")
        .args(["target", "install", target])
        .status()
        .context("Failed to install iOS target")?;

    if !status.success() {
        anyhow::bail!("Failed to install target: {}", target);
    }

    Ok(())
}

/// Locate the app bundle produced by the last `ignit build ios`.
pub fn app_bundle_path(project_root: &Path, app_name: &str, release: bool) -> PathBuf {
    let configuration = if release { "Release" } else { "Debug" };

    project_root
        .join("Build")
        .join("Products")
        .join(format!("{}-iphonesimulator", configuration))
        .join(format!("{}.app", app_name))
}
