use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;
use std::process::Command;

use super::prompt::ProjectConfig;

pub fn generate_project(project_dir: &Path, config: &ProjectConfig) -> Result<()> {
    fs::create_dir_all(project_dir.join("src"))?;

    generate_lib_rs(project_dir, config)?;
    generate_cargo_toml(project_dir, config)?;
    generate_orbital_toml(project_dir, config)?;
    generate_main_rs(project_dir, config)?;
    generate_gitignore(project_dir)?;

    // Create the Assets/ directory for project-owned assets. Desktop builds
    // resolve runtime paths against <cwd>/Assets and Android bundles this
    // directory into the APK.
    fs::create_dir_all(project_dir.join("Assets"))?;
    fs::write(project_dir.join("Assets").join(".gitkeep"), "")
        .context("Failed to write Assets/.gitkeep")?;

    // The all-in-one template ships its own scene and helmet model. Embedding
    // them keeps generated projects runnable when the CLI was installed from a
    // git checkout, where no Examples/ directory exists on disk.
    if config.template == "all-in-one" {
        write_all_in_one_assets(project_dir)?;
    }

    Ok(())
}

/// Writes a project `.gitignore` for build outputs and logs.
fn generate_gitignore(project_dir: &Path) -> Result<()> {
    const GITIGNORE: &str = r#"# Rust
/target/

# Generated Android project
/Android/

# Cargo state
/.cargo/

# Logs
*.log
Logs/

"#;

    fs::write(project_dir.join(".gitignore"), GITIGNORE).context("Failed to write .gitignore")?;
    Ok(())
}

/// Initializes a Git repository unless the project is already inside one.
pub fn init_git(project_dir: &Path) -> Result<()> {
    if git_says_inside_worktree(project_dir) {
        println!("Project is already inside a Git repository.");
        return Ok(());
    }

    match Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(project_dir)
        .status()
    {
        Ok(status) if status.success() => println!("Initialized Git repository."),
        Ok(status) => bail!("git init failed with status {status}"),
        Err(error) => println!("Git not available; skipping repository initialization: {error}"),
    }

    Ok(())
}

fn git_says_inside_worktree(project_dir: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(project_dir)
        .output()
        .map(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
        })
        .unwrap_or(false)
}

fn write_all_in_one_assets(project_dir: &Path) -> Result<()> {
    fs::write(
        project_dir.join("src").join("custom_shader.wgsl"),
        ALL_IN_ONE_CUSTOM_SHADER,
    )
    .context("Failed to write src/custom_shader.wgsl")?;

    let assets = project_dir.join("Assets");

    fs::create_dir_all(assets.join("Scenes")).context("Failed to create Assets/Scenes")?;
    fs::write(
        assets.join("Scenes").join("procgeo_demo.ron"),
        ALL_IN_ONE_SCENE,
    )
    .context("Failed to write Assets/Scenes/procgeo_demo.ron")?;

    fs::create_dir_all(assets.join("Models")).context("Failed to create Assets/Models")?;
    fs::write(
        assets.join("Models").join("DamagedHelmet.glb"),
        ALL_IN_ONE_HELMET,
    )
    .context("Failed to write Assets/Models/DamagedHelmet.glb")?;

    Ok(())
}

fn generate_cargo_toml(project_dir: &Path, config: &ProjectConfig) -> Result<()> {
    let lib_name = config.project_name.replace('-', "_").to_lowercase();

    let orbital_dep = if let Some(path) = &config.engine_path {
        format!(r#"orbital = {{ path = "{path}/Crates/orbital" }}"#)
    } else {
        format!(
            r#"orbital = {{ git = "{repo}", branch = "{branch}" }}"#,
            repo = config.engine_repo,
            branch = config.engine_branch,
        )
    };

    let content = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "{name}_desktop"
path = "src/main.rs"

[lib]
name = "{lib_name}"
crate-type = ["cdylib", "lib"]

[dependencies]
{orbital_dep}

# The engine and cryoglyph both use the same patched cosmic-text fork.
# This patch pins any crates.io consumer of cosmic-text to that revision,
# so cargo cannot resolve two incompatible cosmic-text versions (which
# makes iced_wgpu fail to compile).
[patch.crates-io]
cosmic-text = {{ git = "https://github.com/hecrj/cosmic-text.git", rev = "19cca2bd90e73e20614964f06d44240e71e503b3" }}
"#,
        name = config.project_name,
        lib_name = lib_name,
        orbital_dep = orbital_dep,
    );

    fs::write(project_dir.join("Cargo.toml"), content).context("Failed to write Cargo.toml")?;

    Ok(())
}

fn generate_orbital_toml(project_dir: &Path, config: &ProjectConfig) -> Result<()> {
    let content = format!(
        r#"[orbital]

[android]
package = "{package}"
min_sdk = {min_sdk}
target_sdk = {target_sdk}
targets = ["arm64-v8a", "armeabi-v7a", "x86_64", "x86"]
apk_mode = "multiarch"
ndk_version = "26.2.11394342"
screen_orientation = "unspecified"

# Signing configuration for release APKs (optional)
# If not specified, a debug keystore will be automatically generated
# keystore_path = "keystore/release.keystore"
# keystore_password = "your_keystore_password"
# key_alias = "your_key_alias"
# key_password = "your_key_password"
"#,
        package = config.package_name,
        min_sdk = config.min_sdk,
        target_sdk = config.target_sdk,
    );

    fs::write(project_dir.join("Orbital.toml"), content).context("Failed to write Orbital.toml")?;

    Ok(())
}

fn generate_lib_rs(project_dir: &Path, config: &ProjectConfig) -> Result<()> {
    let content = template_lib_source(&config.template)?;

    fs::write(project_dir.join("src").join("lib.rs"), content)
        .context("Failed to write src/lib.rs")?;

    Ok(())
}

/// Returns the embedded application source for a template.
fn template_lib_source(template: &str) -> Result<&'static str> {
    match template {
        "minimal-3d" => Ok(MINIMAL_3D_TEMPLATE),
        "all-in-one" => Ok(ALL_IN_ONE_TEMPLATE),
        "minimal-2d" => Ok(MINIMAL_2D_TEMPLATE),
        other => bail!(
            "Unknown template '{other}'. Available templates: minimal-3d, all-in-one, minimal-2d"
        ),
    }
}

fn generate_main_rs(project_dir: &Path, config: &ProjectConfig) -> Result<()> {
    let lib_name = config.project_name.replace('-', "_").to_lowercase();

    let content = format!(
        r#"fn main() {{
    // Desktop entry point - `run()` is generated by `#[orbital::main]`.
    {lib_name}::run();
}}
"#,
        lib_name = lib_name,
    );

    fs::write(project_dir.join("src").join("main.rs"), content)
        .context("Failed to write src/main.rs")?;

    Ok(())
}

// The canonical template sources are workspace examples. Embedding them keeps
// `orbital init` self-contained when the CLI is installed from git or a path.
const MINIMAL_3D_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../Examples/minimal-3d/src/lib.rs"
));

const ALL_IN_ONE_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../Examples/all-in-one/src/lib.rs"
));

const ALL_IN_ONE_SCENE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../Examples/all-in-one/Assets/Scenes/procgeo_demo.ron"
));

const ALL_IN_ONE_HELMET: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../Examples/all-in-one/Assets/Models/DamagedHelmet.glb"
));

const ALL_IN_ONE_CUSTOM_SHADER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../Examples/all-in-one/src/custom_shader.wgsl"
));

const MINIMAL_2D_TEMPLATE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../Examples/minimal-2d/src/lib.rs"
));
