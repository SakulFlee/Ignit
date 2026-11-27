use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct IgnitConfig {
    pub ignit: Option<IgnitGeneral>,
    pub android: Option<AndroidConfig>,
    pub ios: Option<IosConfig>,
}

impl IgnitConfig {
    pub fn ignit(&self) -> IgnitGeneral {
        self.ignit.clone().unwrap_or_default()
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct IgnitGeneral {
    pub engine_repo: Option<String>,
    pub engine_branch: Option<String>,
    pub engine_path: Option<String>,
}

impl IgnitGeneral {
    pub fn engine_repo(&self) -> Option<&str> {
        self.engine_repo.as_deref()
    }

    pub fn engine_branch(&self) -> &str {
        self.engine_branch.as_deref().unwrap_or("main")
    }
}

#[derive(Debug, Deserialize)]
pub struct AndroidConfig {
    pub package: Option<String>,
    pub min_sdk: Option<u32>,
    pub target_sdk: Option<u32>,
    pub targets: Option<Vec<String>>,
    pub apk_mode: Option<String>,
    pub ndk_version: Option<String>,
    pub screen_orientation: Option<String>,
    /// Path to the keystore file for signing release APKs (relative to project root or absolute)
    pub keystore_path: Option<String>,
    /// Keystore password
    pub keystore_password: Option<String>,
    /// Key alias in the keystore
    pub key_alias: Option<String>,
    /// Key password
    pub key_password: Option<String>,
}

impl Default for AndroidConfig {
    fn default() -> Self {
        Self {
            package: Some("de.sakulflee.ignit".to_string()),
            min_sdk: Some(21),
            target_sdk: Some(34),
            targets: None,
            apk_mode: None,
            ndk_version: None,
            screen_orientation: None,
            keystore_path: None,
            keystore_password: None,
            key_alias: None,
            key_password: None,
        }
    }
}

impl AndroidConfig {
    pub fn package_name(&self) -> &str {
        self.package.as_deref().unwrap_or("de.sakulflee.ignit")
    }

    pub fn min_sdk(&self) -> u32 {
        self.min_sdk.unwrap_or(21)
    }

    pub fn target_sdk(&self) -> u32 {
        self.target_sdk.unwrap_or(34)
    }

    pub fn targets(&self) -> Vec<String> {
        self.targets.clone().unwrap_or_else(|| {
            vec![
                "arm64-v8a".to_string(),
                "armeabi-v7a".to_string(),
                "x86_64".to_string(),
                "x86".to_string(),
            ]
        })
    }

    pub fn apk_mode(&self) -> &str {
        self.apk_mode.as_deref().unwrap_or("multiarch")
    }

    pub fn ndk_version(&self) -> &str {
        self.ndk_version.as_deref().unwrap_or("26.2.11394342")
    }

    /// Returns the screen orientation, defaulting to landscape that still
    /// follows the device sensor.
    pub fn screen_orientation(&self) -> &str {
        self.screen_orientation
            .as_deref()
            .unwrap_or("sensorLandscape")
    }

    /// Returns the path to the keystore file, or None if not configured.
    /// The path is resolved relative to the project root if it's not absolute.
    pub fn keystore_path(&self) -> Option<&str> {
        self.keystore_path.as_deref()
    }

    /// Returns the keystore password, or "android" as a default.
    pub fn keystore_password(&self) -> &str {
        self.keystore_password.as_deref().unwrap_or("android")
    }

    /// Returns the key alias, or "androiddebugkey" as a default.
    pub fn key_alias(&self) -> &str {
        self.key_alias.as_deref().unwrap_or("androiddebugkey")
    }

    /// Returns the key password, or "android" as a default.
    pub fn key_password(&self) -> &str {
        self.key_password.as_deref().unwrap_or("android")
    }

    /// Returns true if signing configuration is provided.
    pub fn has_signing_config(&self) -> bool {
        self.keystore_path.is_some()
    }
}

#[derive(Debug, Deserialize)]
pub struct IosConfig {
    pub bundle_id: Option<String>,
    pub deployment_target: Option<String>,
    pub app_name: Option<String>,
}

impl Default for IosConfig {
    fn default() -> Self {
        Self {
            bundle_id: Some("de.sakulflee.ignit".to_string()),
            deployment_target: Some("16.0".to_string()),
            app_name: Some("Ignit".to_string()),
        }
    }
}

impl IosConfig {
    pub fn bundle_id(&self) -> &str {
        self.bundle_id.as_deref().unwrap_or("de.sakulflee.ignit")
    }

    pub fn deployment_target(&self) -> &str {
        self.deployment_target.as_deref().unwrap_or("16.0")
    }

    pub fn app_name(&self) -> &str {
        self.app_name.as_deref().unwrap_or("Ignit")
    }
}

/// Find the project root by looking for Ignit.toml
pub fn find_project_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("Failed to get current directory")?;

    loop {
        if dir.join("Ignit.toml").exists() {
            return Ok(dir);
        }

        dir = dir
            .parent()
            .context("Not an Ignit project (no Ignit.toml found)")?
            .to_path_buf();
    }
}

/// Load the Ignit.toml config
pub fn load_config() -> Result<IgnitConfig> {
    let project_root = find_project_root()?;
    let config_path = project_root.join("Ignit.toml");

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;

    let config: IgnitConfig = toml::from_str(&content).context("Failed to parse Ignit.toml")?;

    Ok(config)
}

/// Load Android config with defaults
pub fn load_android_config() -> Result<AndroidConfig> {
    let config = load_config()?;
    let android = config.android.unwrap_or_default();
    if let Some(orientation) = &android.screen_orientation
        && !VALID_SCREEN_ORIENTATIONS.contains(&orientation.as_str())
    {
        anyhow::bail!(
            "Invalid screen_orientation \"{orientation}\" in Ignit.toml. \
             Valid values: {}",
            VALID_SCREEN_ORIENTATIONS.join(", ")
        );
    }
    Ok(android)
}

/// Load iOS config with defaults
pub fn load_ios_config() -> Result<IosConfig> {
    let config = load_config()?;
    Ok(config.ios.unwrap_or_default())
}

/// Ensure Android setup values exist, prompting for missing ones in a terminal.
///
/// Called lazily when an Android project is about to be generated. Non-
/// interactive environments get the defaults so CI never blocks.
pub fn ensure_android_config(interactive: bool) -> Result<AndroidConfig> {
    let project_root = find_project_root()?;
    let parsed = load_config()?;
    let section_present = parsed.android.is_some();

    let mut android = if section_present {
        parsed.android.unwrap()
    } else {
        AndroidConfig {
            package: None,
            min_sdk: None,
            target_sdk: None,
            ..AndroidConfig::default()
        }
    };

    let interactive = interactive && std::io::stdin().is_terminal();
    let mut changes: Vec<(&'static str, String)> = Vec::new();

    if android.package.is_none() {
        let default = default_platform_package(&project_root);
        let value = prompt_text("Android package", &default, interactive)?;
        android.package = Some(value.clone());
        changes.push(("package", toml_quote(&value)));
    }

    if android.min_sdk.is_none() {
        let value = prompt_u32("Android min SDK", 21, interactive)?;
        android.min_sdk = Some(value);
        changes.push(("min_sdk", value.to_string()));
    }

    if android.target_sdk.is_none() {
        let value = prompt_u32("Android target SDK", 34, interactive)?;
        android.target_sdk = Some(value);
        changes.push(("target_sdk", value.to_string()));
    }

    if !changes.is_empty() {
        write_config_values(&project_root, "android", section_present, &changes)?;
    }

    Ok(android)
}

/// Ensure iOS setup values exist, prompting for missing ones in a terminal.
///
/// The deployment target is the iOS equivalent of Android's SDK versions and
/// is written on the first iOS project generation.
pub fn ensure_ios_config(interactive: bool) -> Result<IosConfig> {
    let project_root = find_project_root()?;
    let parsed = load_config()?;
    let section_present = parsed.ios.is_some();

    let mut ios = if section_present {
        parsed.ios.unwrap()
    } else {
        IosConfig {
            bundle_id: None,
            deployment_target: None,
            app_name: None,
        }
    };

    let interactive = interactive && std::io::stdin().is_terminal();
    let mut changes: Vec<(&'static str, String)> = Vec::new();

    if ios.bundle_id.is_none() {
        let default = default_platform_package(&project_root);
        let value = prompt_text("iOS bundle ID", &default, interactive)?;
        ios.bundle_id = Some(value.clone());
        changes.push(("bundle_id", toml_quote(&value)));
    }

    if ios.deployment_target.is_none() {
        let value = prompt_text("iOS deployment target", "16.0", interactive)?;
        ios.deployment_target = Some(value.clone());
        changes.push(("deployment_target", toml_quote(&value)));
    }

    if ios.app_name.is_none() {
        let default = read_cargo_package_name(&project_root).unwrap_or_else(|| "Ignit".to_string());
        let value = prompt_text("iOS app name", &default, interactive)?;
        ios.app_name = Some(value.clone());
        changes.push(("app_name", toml_quote(&value)));
    }

    if !changes.is_empty() {
        write_config_values(&project_root, "ios", section_present, &changes)?;
    }

    Ok(ios)
}

/// Default platform package/bundle identifier derived from the Cargo package.
fn default_platform_package(project_root: &Path) -> String {
    let name = read_cargo_package_name(project_root).unwrap_or_else(|| "ignit_game".to_string());
    format!("com.example.{}", name.replace('-', "_"))
}

/// Reads the `[package] name` from a project's Cargo.toml.
fn read_cargo_package_name(project_root: &Path) -> Option<String> {
    let content = fs::read_to_string(project_root.join("Cargo.toml")).ok()?;
    let mut in_package = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[package]" {
            in_package = true;
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package = false;
            continue;
        }

        if in_package
            && let Some((key, value)) = trimmed.split_once('=')
            && key.trim() == "name"
        {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }

    None
}

fn prompt_text(label: &str, default: &str, interactive: bool) -> Result<String> {
    if !interactive {
        return Ok(default.to_string());
    }

    let value = inquire::Text::new(&format!("{label}:"))
        .with_default(default)
        .prompt()?;

    let value = value.trim();
    Ok(if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    })
}

fn prompt_u32(label: &str, default: u32, interactive: bool) -> Result<u32> {
    if !interactive {
        return Ok(default);
    }

    let value = inquire::Text::new(&format!("{label}:"))
        .with_default(&default.to_string())
        .prompt()?;

    Ok(value.trim().parse().unwrap_or(default))
}

fn toml_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Writes missing keys into a platform section, appending the section if needed.
fn write_config_values(
    project_root: &Path,
    section: &str,
    section_present: bool,
    values: &[(&str, String)],
) -> Result<()> {
    let path = project_root.join("Ignit.toml");
    let mut content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;

    if section_present {
        for (key, value) in values {
            if !set_toml_key(&mut content, section, key, value) {
                anyhow::bail!(
                    "Could not find the [{}] section in {}",
                    section,
                    path.display()
                );
            }
        }
    } else {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("\n[{}]\n", section));
        for (key, value) in values {
            content.push_str(&format!("{} = {}\n", key, value));
        }
    }

    fs::write(&path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Replaces or inserts a key inside an existing TOML section.
fn set_toml_key(content: &mut String, section: &str, key: &str, value: &str) -> bool {
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();

    let Some(start) = lines
        .iter()
        .position(|line| is_section_header(line, section))
    else {
        return false;
    };

    let end = lines[start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map_or(lines.len(), |offset| start + 1 + offset);

    let existing = (start + 1..end).find(|&index| {
        lines[index]
            .split_once('=')
            .map(|(name, _)| name.trim() == key)
            .unwrap_or(false)
    });

    let replacement = format!("{} = {}", key, value);
    match existing {
        Some(index) => lines[index] = replacement,
        None => lines.insert(end, replacement),
    }

    let mut joined = lines.join("\n");
    joined.push('\n');
    *content = joined;
    true
}

fn is_section_header(line: &str, section: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('[')
        && trimmed.ends_with(']')
        && trimmed[1..trimmed.len() - 1].trim() == section
}

/// Valid values for the `android:screenOrientation` manifest attribute.
const VALID_SCREEN_ORIENTATIONS: &[&str] = &[
    "unspecified",
    "behind",
    "landscape",
    "portrait",
    "reverseLandscape",
    "reversePortrait",
    "sensorLandscape",
    "sensorPortrait",
    "userLandscape",
    "userPortrait",
    "sensor",
    "fullSensor",
    "nosensor",
    "user",
    "fullUser",
    "locked",
];

/// Get the package name from the current directory's Cargo.toml
pub fn get_package_name() -> Result<String> {
    let cargo_toml_path = Path::new("Cargo.toml");
    if !cargo_toml_path.exists() {
        anyhow::bail!("No Cargo.toml found in current directory");
    }

    let content = std::fs::read_to_string(cargo_toml_path).context("Failed to read Cargo.toml")?;

    // Parse [package] name
    let mut in_package_section = false;
    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[package]" {
            in_package_section = true;
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package_section = false;
            continue;
        }

        if in_package_section
            && trimmed.starts_with("name")
            && let Some(value) = trimmed.split_once('=')
        {
            let value = value.1.trim().trim_matches('"');
            return Ok(value.to_string());
        }
    }

    anyhow::bail!("No [package] name found in Cargo.toml")
}

/// Get the lib name from the current directory's Cargo.toml
pub fn get_lib_name() -> Result<String> {
    let cargo_toml_path = Path::new("Cargo.toml");
    if !cargo_toml_path.exists() {
        anyhow::bail!("No Cargo.toml found in current directory");
    }

    let content = std::fs::read_to_string(cargo_toml_path).context("Failed to read Cargo.toml")?;

    // Parse [lib] name
    let mut in_lib_section = false;
    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[lib]" {
            in_lib_section = true;
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_lib_section = false;
            continue;
        }

        if in_lib_section
            && trimmed.starts_with("name")
            && let Some(value) = trimmed.split_once('=')
        {
            let value = value.1.trim().trim_matches('"');
            return Ok(value.to_string());
        }
    }

    // Fallback: use package name
    get_package_name()
}

/// Resolve the Rust library name for a package, normalizing hyphens the way
/// Cargo does for static-library filenames.
pub fn resolve_lib_name(package_name: Option<&str>) -> Result<String> {
    let name = match package_name {
        Some(pkg) => find_package_lib_name(pkg)?,
        None => get_lib_name()?,
    };

    Ok(name.replace('-', "_"))
}

/// Find a package's path in the workspace.
///
/// Uses `cargo metadata` first so glob workspace members such as
/// `Examples/*` resolve correctly, then falls back to the small manifest
/// parser for environments where Cargo metadata is unavailable.
pub fn find_package_path(package_name: &str) -> Result<PathBuf> {
    let project_root = find_project_root()?;

    if let Some(path) = package_path_from_cargo_metadata(&project_root, package_name) {
        return Ok(path);
    }

    let cargo_toml_path = project_root.join("Cargo.toml");

    let content = std::fs::read_to_string(&cargo_toml_path)
        .with_context(|| format!("Failed to read {}", cargo_toml_path.display()))?;

    // Check if this is a workspace
    if !content.contains("[workspace]") {
        anyhow::bail!("Not a workspace. Use --package only in workspace projects.");
    }

    // Parse workspace members
    let mut in_workspace_section = false;
    let mut in_members_list = false;
    let mut members = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[workspace]" {
            in_workspace_section = true;
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_workspace_section = false;
            in_members_list = false;
            continue;
        }

        if in_workspace_section && trimmed.starts_with("members") {
            in_members_list = true;
            // Check for inline list
            if let Some(list_start) = trimmed.find('[')
                && let Some(list_end) = trimmed.find(']')
            {
                let list = &trimmed[list_start + 1..list_end];
                for item in list.split(',') {
                    let item = item.trim().trim_matches('"');
                    members.push(item.to_string());
                }
                in_members_list = false;
            }
            continue;
        }

        if in_members_list {
            if trimmed.starts_with('"') || trimmed.starts_with('\'') {
                let member = trimmed.trim_matches(|c| c == '"' || c == '\'');
                members.push(member.to_string());
            } else if trimmed == "]" {
                in_members_list = false;
            }
        }
    }

    // Find the package in members
    for member in &members {
        let member_path = project_root.join(member);
        if !member_path.exists() {
            continue;
        }

        let member_cargo = member_path.join("Cargo.toml");
        if !member_cargo.exists() {
            continue;
        }

        let member_content = std::fs::read_to_string(&member_cargo)
            .with_context(|| format!("Failed to read {}", member_cargo.display()))?;

        // Check if this member has the package name
        let mut in_package_section = false;
        for line in member_content.lines() {
            let trimmed = line.trim();

            if trimmed == "[package]" {
                in_package_section = true;
                continue;
            }

            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_package_section = false;
                continue;
            }

            if in_package_section
                && trimmed.starts_with("name")
                && let Some(value) = trimmed.split_once('=')
            {
                let value = value.1.trim().trim_matches('"');
                if value == package_name {
                    return Ok(member_path);
                }
            }
        }
    }

    anyhow::bail!("Package '{}' not found in workspace", package_name)
}

/// Resolve a workspace package directory through `cargo metadata`.
fn package_path_from_cargo_metadata(project_root: &Path, package_name: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(project_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let packages = metadata.get("packages")?.as_array()?;

    for package in packages {
        if package.get("name").and_then(|name| name.as_str()) != Some(package_name) {
            continue;
        }

        let manifest = package.get("manifest_path")?.as_str()?;
        return Path::new(manifest).parent().map(Path::to_path_buf);
    }

    None
}

/// Find a package's lib name in the workspace
pub fn find_package_lib_name(package_name: &str) -> Result<String> {
    let package_path = find_package_path(package_name)?;
    let cargo_toml_path = package_path.join("Cargo.toml");

    let content = std::fs::read_to_string(&cargo_toml_path)
        .with_context(|| format!("Failed to read {}", cargo_toml_path.display()))?;

    // Parse [lib] name
    let mut in_lib_section = false;
    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[lib]" {
            in_lib_section = true;
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_lib_section = false;
            continue;
        }

        if in_lib_section
            && trimmed.starts_with("name")
            && let Some(value) = trimmed.split_once('=')
        {
            let value = value.1.trim().trim_matches('"');
            return Ok(value.to_string());
        }
    }

    // Fallback: use package name
    Ok(package_name.to_string())
}

#[cfg(test)]
mod tests {
    use super::{AndroidConfig, set_toml_key};

    #[test]
    fn android_defaults_to_sensor_landscape() {
        assert_eq!(
            AndroidConfig::default().screen_orientation(),
            "sensorLandscape"
        );
    }

    #[test]
    fn configured_orientation_wins_over_the_default() {
        let config = AndroidConfig {
            screen_orientation: Some("portrait".to_string()),
            ..AndroidConfig::default()
        };

        assert_eq!(config.screen_orientation(), "portrait");
    }

    #[test]
    fn toml_key_updates_stay_inside_their_section() {
        let mut content =
            "[ignit]\n\n[android]\npackage = \"com.example.demo\"\n\n[ios]\napp_name = \"demo\"\n"
                .to_string();

        assert!(set_toml_key(&mut content, "android", "min_sdk", "21"));
        assert!(set_toml_key(&mut content, "android", "min_sdk", "22"));
        assert!(set_toml_key(
            &mut content,
            "ios",
            "deployment_target",
            "\"16.0\""
        ));

        assert!(content.contains("[android]\npackage = \"com.example.demo\""));
        assert!(content.contains("min_sdk = 22"));
        assert!(!content.contains("min_sdk = 21"));
        assert!(content.contains("[ios]\napp_name = \"demo\""));
        assert!(content.contains("deployment_target = \"16.0\""));
    }
}
