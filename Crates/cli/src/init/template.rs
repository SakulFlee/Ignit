use anyhow::{Context, Result, bail};
use std::fs;
use std::path::Path;
use std::process::Command;

use super::prompt::{EngineSource, ProjectConfig};

pub fn generate_project(project_dir: &Path, config: &ProjectConfig) -> Result<()> {
    fs::create_dir_all(project_dir.join("src"))?;

    generate_lib_rs(project_dir, config)?;
    generate_cargo_toml(project_dir, config)?;
    generate_ignit_toml(project_dir, config)?;
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

fn cargo_toml(config: &ProjectConfig) -> String {
    let lib_name = config.project_name.replace('-', "_").to_lowercase();

    let ignit_dep = match &config.engine_source {
        EngineSource::CratesIo => format!(r#"ignit = "{}""#, env!("CARGO_PKG_VERSION")),
        EngineSource::Git { repo, branch } => {
            format!(r#"ignit = {{ git = "{repo}", branch = "{branch}" }}"#)
        }
        EngineSource::Path(path) => {
            format!(r#"ignit = {{ path = "{path}/Crates/ignit" }}"#)
        }
    };

    format!(
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
{ignit_dep}
"#,
        name = config.project_name,
        lib_name = lib_name,
        ignit_dep = ignit_dep,
    )
}

fn generate_cargo_toml(project_dir: &Path, config: &ProjectConfig) -> Result<()> {
    fs::write(project_dir.join("Cargo.toml"), cargo_toml(config))
        .context("Failed to write Cargo.toml")?;

    Ok(())
}

fn generate_ignit_toml(project_dir: &Path, config: &ProjectConfig) -> Result<()> {
    let content = format!(
        r#"[ignit]

[android]
package = "{package}"
min_sdk = {min_sdk}
target_sdk = {target_sdk}
targets = ["arm64-v8a", "armeabi-v7a", "x86_64", "x86"]
apk_mode = "multiarch"
ndk_version = "26.2.11394342"
screen_orientation = "sensorLandscape"

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

    fs::write(project_dir.join("Ignit.toml"), content).context("Failed to write Ignit.toml")?;

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
    // Desktop entry point - `run()` is generated by `#[ignit::main]`.
    {lib_name}::run();
}}
"#,
        lib_name = lib_name,
    );

    fs::write(project_dir.join("src").join("main.rs"), content)
        .context("Failed to write src/main.rs")?;

    Ok(())
}

// The canonical template sources are workspace examples. Copies live under
// `src/template/app` so the published CLI crate is self-contained;
// `.forgejo/ci/test-templates.sh` checks that the copies stay in sync.
const MINIMAL_3D_TEMPLATE: &str = include_str!("../template/app/minimal-3d.rs");

const ALL_IN_ONE_TEMPLATE: &str = include_str!("../template/app/all-in-one.rs");

const ALL_IN_ONE_SCENE: &[u8] = include_bytes!("../template/app/procgeo_demo.ron");

const ALL_IN_ONE_HELMET: &[u8] = include_bytes!("../template/app/DamagedHelmet.glb");

const ALL_IN_ONE_CUSTOM_SHADER: &str =
    include_str!("../template/app/all-in-one-custom-shader.wgsl");

const MINIMAL_2D_TEMPLATE: &str = include_str!("../template/app/minimal-2d.rs");

#[cfg(test)]
mod tests {
    use super::*;

    fn project_config(engine_source: EngineSource) -> ProjectConfig {
        ProjectConfig {
            project_name: "demo".to_string(),
            package_name: "com.example.demo".to_string(),
            min_sdk: 21,
            target_sdk: 34,
            template: "minimal-3d".to_string(),
            generate_android: false,
            engine_source,
        }
    }

    #[test]
    fn crates_io_uses_the_cli_version() {
        let content = cargo_toml(&project_config(EngineSource::CratesIo));

        assert!(content.contains(&format!("ignit = \"{}\"", env!("CARGO_PKG_VERSION"))));
        assert!(!content.contains("[patch.crates-io]"));
    }

    #[test]
    fn git_and_local_sources_are_rendered() {
        let git = cargo_toml(&project_config(EngineSource::Git {
            repo: "https://example.com/Ignit.git".to_string(),
            branch: "dev".to_string(),
        }));
        assert!(git.contains("git = \"https://example.com/Ignit.git\""));
        assert!(git.contains("branch = \"dev\""));

        let local = cargo_toml(&project_config(EngineSource::Path(
            "/tmp/Ignit".to_string(),
        )));
        assert!(local.contains("path = \"/tmp/Ignit/Crates/ignit\""));
    }
}
