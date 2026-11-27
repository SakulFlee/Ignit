mod prompt;
mod template;

use std::path::{Path, PathBuf};

use anyhow::Result;

#[allow(clippy::too_many_arguments)]
pub fn run(
    name: Option<String>,
    parent_path: Option<PathBuf>,
    project_path: Option<PathBuf>,
    package: Option<String>,
    template: Option<String>,
    android: bool,
    ios: bool,
    engine_repo: Option<String>,
    engine_branch: Option<String>,
    engine_path: Option<PathBuf>,
    yes: bool,
    no_git: bool,
) -> Result<()> {
    if parent_path.is_some() && project_path.is_some() {
        anyhow::bail!("--parent-path and --project-path are mutually exclusive");
    }

    let engine_path_str = engine_path.map(|p| {
        p.into_os_string()
            .into_string()
            .expect("engine path contains invalid UTF-8")
    });

    let config = if yes {
        // Non-interactive mode: use defaults or provided values
        prompt::non_interactive(
            name,
            package,
            template,
            android,
            ios,
            engine_repo,
            engine_branch,
            engine_path_str,
        )?
    } else {
        // Interactive mode: prompt for the general project settings
        prompt::interactive(
            name,
            package,
            template,
            android,
            ios,
            engine_repo,
            engine_branch,
            engine_path_str,
        )?
    };

    println!("\nGenerating project...");

    // Create project directory
    let project_dir = if let Some(parent) = parent_path {
        parent.join(&config.project_name)
    } else if let Some(project) = project_path {
        project
    } else {
        std::env::current_dir()?.join(&config.project_name)
    };
    if project_dir.exists() {
        anyhow::bail!(
            "Directory '{}' already exists. Please choose a different name.",
            config.project_name
        );
    }

    std::fs::create_dir_all(&project_dir)?;

    // Generate project files
    template::generate_project(&project_dir, &config)?;

    if no_git {
        println!("Skipping Git repository initialization (--no-git).");
    } else {
        template::init_git(&project_dir)?;
    }

    println!("\nProject '{}' created successfully!", config.project_name);

    generate_platform_projects(&project_dir, &config, !yes)?;

    println!("\nNext steps:");
    println!("  cd {}", project_dir.display());

    if crate::tooling::is_nixos() {
        println!("  # NixOS: build in a shell that provides the engine's system libraries, e.g.");
        println!("  nix-shell -p cargo pkg-config systemd wayland libxkbcommon vulkan-loader mesa");
    }

    println!("  ignit build");

    if config.generate_android {
        println!("  ignit build android");
    }

    #[cfg(target_os = "macos")]
    if config.generate_ios {
        println!("  ignit build ios");
    }

    Ok(())
}

/// Generates any platform projects requested with `--android` or `--ios`.
///
/// `interactive` mirrors the `init` prompt mode: interactive runs ask for
/// missing platform values, `--yes` runs fill them with defaults.
fn generate_platform_projects(
    project_dir: &Path,
    config: &prompt::ProjectConfig,
    interactive: bool,
) -> Result<()> {
    if !config.generate_android && !config.generate_ios {
        return Ok(());
    }

    let original_dir = std::env::current_dir()?;
    std::env::set_current_dir(project_dir)?;

    let result = (|| -> Result<()> {
        if config.generate_android {
            println!("\nInitializing Android project...");
            crate::config::ensure_android_config(interactive)?;
            crate::android::project::init()?;
        }

        if config.generate_ios {
            #[cfg(target_os = "macos")]
            {
                println!("\nInitializing iOS project...");
                crate::config::ensure_ios_config(interactive)?;
                crate::ios::project::init(None, false)?;
            }

            #[cfg(not(target_os = "macos"))]
            println!("\nSkipping iOS project generation: it requires macOS with Xcode.");
        }

        Ok(())
    })();

    std::env::set_current_dir(original_dir)?;
    result
}
