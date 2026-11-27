use anyhow::{Result, bail};
use inquire::{Select, Text};
use std::path::Path;

use crate::config;

const FORGEJO_REPO: &str = "https://forgejo.sakul-flee.de/SakulFlee/Ignit.git";
const GITHUB_REPO: &str = "https://github.com/SakulFlee/Ignit.git";
const DEFAULT_BRANCH: &str = "main";

const CRATES_IO: &str = "crates.io (recommended)";
const FORGEJO: &str = "Forgejo (main)";
const GITHUB: &str = "GitHub mirror (main)";
const CUSTOM_GIT: &str = "Custom git repository";
const LOCAL: &str = "Local checkout";

/// Where the generated project should get the Ignit engine from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineSource {
    CratesIo,
    Git { repo: String, branch: String },
    Path(String),
}

impl EngineSource {
    /// Validate a source before it is written into `Cargo.toml`.
    fn validate(&self) -> Result<()> {
        match self {
            Self::CratesIo => Ok(()),
            Self::Git { repo, branch } => {
                let valid_prefixes = ["ssh://", "https://", "http://", "git://", "git@"];

                if !valid_prefixes.iter().any(|prefix| repo.starts_with(prefix)) {
                    bail!(
                        "engine repository must be a remote git URL \
                         (ssh://, https://, http://, git://, git@...): {repo}"
                    );
                }

                if branch.trim().is_empty() {
                    bail!("engine branch must not be empty");
                }

                Ok(())
            }
            Self::Path(path) => {
                let p = Path::new(path);

                if !p.exists() {
                    bail!("engine path does not exist: {path}");
                }

                if !p.join("Crates/ignit/Cargo.toml").exists() {
                    bail!("not an Ignit engine repo (missing Crates/ignit/Cargo.toml): {path}");
                }

                Ok(())
            }
        }
    }
}

pub struct ProjectConfig {
    pub project_name: String,
    pub package_name: String,
    pub template: String,
    pub generate_android: bool,
    pub generate_ios: bool,
    pub engine_source: EngineSource,
}

fn normalize_repo(repo: &str) -> &str {
    repo.strip_suffix(".git").unwrap_or(repo)
}

fn default_engine_index(config: &config::IgnitGeneral) -> usize {
    if config.engine_path.is_some() {
        return 4; // Local checkout
    }

    match config.engine_repo.as_deref() {
        Some(repo) if normalize_repo(repo) == normalize_repo(FORGEJO_REPO) => 1,
        Some(repo) if normalize_repo(repo) == normalize_repo(GITHUB_REPO) => 2,
        Some(_) => 3,
        None => 0,
    }
}

fn prompt_with_optional_default(message: &str, default: Option<&str>) -> Result<String> {
    let mut prompt = Text::new(message);

    if let Some(default) = default {
        prompt = prompt.with_default(default);
    }

    Ok(prompt.prompt()?)
}

fn prompt_engine_source(config: &config::IgnitGeneral) -> Result<EngineSource> {
    let options = vec![CRATES_IO, FORGEJO, GITHUB, CUSTOM_GIT, LOCAL];

    let choice = Select::new("Engine source:", options)
        .with_starting_cursor(default_engine_index(config))
        .prompt()?;

    match choice {
        CRATES_IO => Ok(EngineSource::CratesIo),
        FORGEJO => Ok(EngineSource::Git {
            repo: FORGEJO_REPO.to_string(),
            branch: DEFAULT_BRANCH.to_string(),
        }),
        GITHUB => Ok(EngineSource::Git {
            repo: GITHUB_REPO.to_string(),
            branch: DEFAULT_BRANCH.to_string(),
        }),
        CUSTOM_GIT => {
            let repo = prompt_with_optional_default(
                "Engine git repository:",
                config.engine_repo.as_deref(),
            )?;
            let branch = Text::new("Engine branch:")
                .with_default(config.engine_branch())
                .prompt()?;

            Ok(EngineSource::Git { repo, branch })
        }
        LOCAL => {
            let path = prompt_with_optional_default(
                "Engine local checkout:",
                config.engine_path.as_deref(),
            )?;

            Ok(EngineSource::Path(path))
        }
        _ => unreachable!("unexpected engine source option"),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn interactive(
    name: Option<String>,
    package: Option<String>,
    template: Option<String>,
    android: bool,
    ios: bool,
    engine_repo: Option<String>,
    engine_branch: Option<String>,
    engine_path: Option<String>,
) -> Result<ProjectConfig> {
    let ignit_config = config::load_config().map(|c| c.ignit()).unwrap_or_default();

    // 1. Project name
    let project_name = match name {
        Some(n) => n,
        None => Text::new("Project name:").prompt()?,
    };

    // 2. Package name. This is also the Android package and iOS bundle ID.
    let default_package = format!("com.example.{}", project_name.replace('-', "_"));
    let package_name = match package {
        Some(p) => p,
        None => Text::new("Package name:")
            .with_default(&default_package)
            .prompt()?,
    };

    // 3. Template selection (minimal-3d, all-in-one, or minimal-2d)
    let template_name = match template {
        Some(t) => t,
        None => {
            let templates = vec!["minimal-3d", "all-in-one", "minimal-2d"];
            Select::new("Select a template:", templates)
                .prompt()?
                .to_string()
        }
    };

    // 4. Engine source
    let engine_source = if let Some(path) = engine_path {
        if engine_repo.is_some() {
            bail!("--engine-path and --engine-repo are mutually exclusive");
        }
        if engine_branch.is_some() {
            bail!("--engine-branch cannot be used with --engine-path");
        }

        EngineSource::Path(path)
    } else if let Some(repo) = engine_repo {
        EngineSource::Git {
            repo,
            branch: engine_branch.unwrap_or_else(|| ignit_config.engine_branch().to_string()),
        }
    } else {
        if engine_branch.is_some() {
            bail!("--engine-branch requires --engine-repo");
        }

        prompt_engine_source(&ignit_config)?
    };

    engine_source.validate()?;

    Ok(ProjectConfig {
        project_name,
        package_name,
        template: template_name,
        generate_android: android,
        generate_ios: ios,
        engine_source,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn non_interactive(
    name: Option<String>,
    package: Option<String>,
    template: Option<String>,
    android: bool,
    ios: bool,
    engine_repo: Option<String>,
    engine_branch: Option<String>,
    engine_path: Option<String>,
) -> Result<ProjectConfig> {
    let ignit_config = config::load_config().map(|c| c.ignit()).unwrap_or_default();

    let project_name = name.ok_or_else(|| {
        anyhow::anyhow!(
            "Project name is required in non-interactive mode. Usage: ignit init <name>"
        )
    })?;

    let package_name =
        package.unwrap_or_else(|| format!("com.example.{}", project_name.replace('-', "_")));

    let template_name = template.unwrap_or_else(|| "minimal-3d".to_string());

    let engine_source = if let Some(path) = engine_path {
        if engine_repo.is_some() {
            bail!("--engine-path and --engine-repo are mutually exclusive");
        }
        if engine_branch.is_some() {
            bail!("--engine-branch cannot be used with --engine-path");
        }

        EngineSource::Path(path)
    } else if let Some(repo) = engine_repo {
        EngineSource::Git {
            repo,
            branch: engine_branch.unwrap_or_else(|| ignit_config.engine_branch().to_string()),
        }
    } else if let Some(path) = ignit_config.engine_path.clone() {
        if engine_branch.is_some() {
            bail!("--engine-branch cannot be used with a configured engine path");
        }

        EngineSource::Path(path)
    } else if let Some(repo) = ignit_config.engine_repo().map(str::to_string) {
        EngineSource::Git {
            repo,
            branch: engine_branch.unwrap_or_else(|| ignit_config.engine_branch().to_string()),
        }
    } else {
        if engine_branch.is_some() {
            bail!("--engine-branch requires --engine-repo");
        }

        EngineSource::CratesIo
    };

    engine_source.validate()?;

    Ok(ProjectConfig {
        project_name,
        package_name,
        template: template_name,
        generate_android: android,
        generate_ios: ios,
        engine_source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn crates_io_source_is_valid() {
        assert!(EngineSource::CratesIo.validate().is_ok());
    }

    #[test]
    fn git_source_requires_a_git_url_and_branch() {
        let valid = EngineSource::Git {
            repo: "https://example.com/Ignit.git".to_string(),
            branch: "main".to_string(),
        };
        assert!(valid.validate().is_ok());

        let invalid_repo = EngineSource::Git {
            repo: "not-a-url".to_string(),
            branch: "main".to_string(),
        };
        assert!(invalid_repo.validate().is_err());

        let invalid_branch = EngineSource::Git {
            repo: "https://example.com/Ignit.git".to_string(),
            branch: String::new(),
        };
        assert!(invalid_branch.validate().is_err());
    }

    #[test]
    fn local_source_requires_an_engine_checkout() {
        let base = std::env::temp_dir().join(format!(
            "ignit-engine-source-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        fs::create_dir_all(base.join("Crates/ignit")).unwrap();
        fs::write(base.join("Crates/ignit/Cargo.toml"), "").unwrap();

        let source = EngineSource::Path(base.to_string_lossy().into_owned());
        assert!(source.validate().is_ok());

        fs::remove_dir_all(base).unwrap();
    }
}
