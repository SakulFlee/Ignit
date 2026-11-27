mod android;
mod config;
mod desktop;
mod init;
mod ios;
mod java;
mod tooling;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ignit",
    bin_name = "ignit",
    version,
    about = "Build tool for the Ignit game engine"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Cargo feature selection forwarded to the underlying build.
#[derive(clap::Args, Clone, Debug, Default)]
pub(crate) struct CargoFeatures {
    /// Space or comma separated list of features to activate
    #[arg(long, value_delimiter = ',')]
    features: Vec<String>,
    /// Do not activate the default feature
    #[arg(long)]
    no_default_features: bool,
    /// Activate all available features
    #[arg(long)]
    all_features: bool,
}

impl CargoFeatures {
    /// Appends the selected feature flags to a cargo argument list.
    pub(crate) fn apply(&self, args: &mut Vec<String>) {
        if !self.features.is_empty() {
            args.push("--features".to_string());
            args.push(self.features.join(","));
        }
        if self.no_default_features {
            args.push("--no-default-features".to_string());
        }
        if self.all_features {
            args.push("--all-features".to_string());
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.features.is_empty() && !self.no_default_features && !self.all_features
    }

    pub(crate) fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.features.is_empty() {
            parts.push(format!("features={}", self.features.join(",")));
        }
        if self.no_default_features {
            parts.push("no-default-features".to_string());
        }
        if self.all_features {
            parts.push("all-features".to_string());
        }
        parts.join(", ")
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Ignit project
    Init {
        /// Project name (optional, will prompt if not provided)
        name: Option<String>,
        /// Parent directory for the project (creates <path>/<name>/)
        #[arg(long)]
        parent_path: Option<PathBuf>,
        /// Exact project directory (files placed directly here)
        #[arg(long)]
        project_path: Option<PathBuf>,
        /// Package name (e.g., com.mycompany.mygame)
        #[arg(short, long)]
        package: Option<String>,
        /// Template to use (minimal-3d, all-in-one, minimal-2d)
        #[arg(short, long)]
        template: Option<String>,
        /// Generate the Android project during init
        #[arg(long)]
        android: bool,
        /// Generate the iOS project during init (macOS only)
        #[arg(long)]
        ios: bool,
        /// Custom engine git repository (overrides the source menu)
        #[arg(long, conflicts_with = "engine_path")]
        engine_repo: Option<String>,
        /// Engine git branch (with --engine-repo)
        #[arg(long)]
        engine_branch: Option<String>,
        /// Local engine checkout (overrides the source menu)
        #[arg(long, conflicts_with = "engine_repo")]
        engine_path: Option<PathBuf>,
        /// Skip interactive prompts (defaults to the crates.io engine)
        #[arg(long)]
        yes: bool,
        /// Do not initialize a Git repository for the generated project
        #[arg(long)]
        no_git: bool,
    },
    /// Initialize Android project for existing Ignit project
    InitAndroid,
    /// Initialize an Xcode project for an existing Ignit project
    InitIos {
        /// Package to generate the project for (workspace projects only)
        #[arg(short, long)]
        package: Option<String>,
        /// Regenerate the Xcode project, overwriting the current one
        #[arg(long)]
        force: bool,
    },
    /// Build for a platform (defaults to desktop)
    Build {
        /// Platform to build for (defaults to desktop)
        #[arg(value_enum)]
        platform: Option<Platform>,
        /// Package to build (optional in standalone projects)
        #[arg(short, long)]
        package: Option<String>,
        /// Build in release mode
        #[arg(long)]
        release: bool,
        #[command(flatten)]
        features: CargoFeatures,
    },
    /// Build, install, and run on a connected device or emulator (defaults to desktop)
    Run {
        /// Platform to run on (defaults to desktop)
        #[arg(value_enum)]
        platform: Option<Platform>,
        /// Package to run (optional in standalone projects)
        #[arg(short, long)]
        package: Option<String>,
        /// Device serial or AVD name to run on
        #[arg(short, long)]
        device: Option<String>,
        /// Run in release mode
        #[arg(long)]
        release: bool,
        /// Skip rebuilding and install the existing APK
        #[arg(long)]
        skip_build: bool,
        /// Skip streaming logcat after launching (attached by default)
        #[arg(long)]
        no_logcat: bool,
        #[command(flatten)]
        features: CargoFeatures,
    },
}

#[derive(clap::ValueEnum, Clone)]
enum Platform {
    Desktop,
    Android,
    Ios,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            name,
            parent_path,
            project_path,
            package,
            template,
            android,
            ios,
            engine_repo,
            engine_branch,
            engine_path,
            yes,
            no_git,
        } => init::run(
            name,
            parent_path,
            project_path,
            package,
            template,
            android,
            ios,
            engine_repo,
            engine_branch,
            engine_path,
            yes,
            no_git,
        ),
        Commands::InitAndroid => android::project::init(),
        Commands::InitIos { package, force } => ios::project::init(package.as_deref(), force),
        Commands::Build {
            platform,
            package,
            release,
            features,
        } => match platform.unwrap_or(Platform::Desktop) {
            Platform::Desktop => desktop::build(release, &features),
            Platform::Android => android::build::build(package.as_deref(), release, &features),
            Platform::Ios => ios::build::build(package.as_deref(), release, &features),
        },
        Commands::Run {
            platform,
            package,
            device,
            release,
            skip_build,
            no_logcat,
            features,
        } => match platform.unwrap_or(Platform::Desktop) {
            Platform::Desktop => desktop::run(release, &features),
            Platform::Android => android::run::run(
                package.as_deref(),
                device.as_deref(),
                skip_build,
                no_logcat,
                release,
                &features,
            ),
            Platform::Ios => ios::run::run(
                package.as_deref(),
                device.as_deref(),
                skip_build,
                release,
                &features,
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{CargoFeatures, Cli, Commands};

    #[test]
    fn cargo_feature_flags_are_appended_in_order() {
        let features = CargoFeatures {
            features: vec!["a".to_string(), "b".to_string()],
            no_default_features: true,
            all_features: true,
        };
        let mut args = vec!["build".to_string()];
        features.apply(&mut args);
        assert_eq!(
            args,
            [
                "build",
                "--features",
                "a,b",
                "--no-default-features",
                "--all-features"
            ]
        );
    }

    #[test]
    fn empty_cargo_features_add_nothing() {
        let features = CargoFeatures::default();
        let mut args = vec!["run".to_string()];
        features.apply(&mut args);
        assert_eq!(args, ["run"]);
        assert!(features.is_empty());
    }

    #[test]
    fn init_accepts_platform_generation_flags() {
        let cli = Cli::try_parse_from(["ignit", "init", "demo", "--android", "--ios"])
            .expect("init flags parse");
        let Commands::Init { android, ios, .. } = cli.command else {
            panic!("expected init");
        };
        assert!(android);
        assert!(ios);

        let cli = Cli::try_parse_from(["ignit", "init", "demo"]).expect("init parse");
        let Commands::Init { android, ios, .. } = cli.command else {
            panic!("expected init");
        };
        assert!(!android);
        assert!(!ios);
    }

    #[test]
    fn build_and_run_accept_cargo_feature_flags() {
        let cli = Cli::try_parse_from([
            "ignit",
            "build",
            "--features",
            "a,b",
            "--no-default-features",
        ])
        .expect("features parse");
        let Commands::Build { features, .. } = cli.command else {
            panic!("expected build");
        };
        assert_eq!(features.features, ["a", "b"]);
        assert!(features.no_default_features);
        assert!(!features.all_features);

        let cli = Cli::try_parse_from(["ignit", "run", "--all-features"]).expect("features parse");
        let Commands::Run { features, .. } = cli.command else {
            panic!("expected run");
        };
        assert!(features.all_features);
    }
}
