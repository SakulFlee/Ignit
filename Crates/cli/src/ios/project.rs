use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;

pub fn init(package_name: Option<&str>, force: bool) -> Result<()> {
    let project_root = config::find_project_root()?;
    let ios_dir = project_root.join("iOS");

    let ios_config = config::load_ios_config()?;
    let lib_name = config::resolve_lib_name(package_name)?;

    let project_file = ios_dir
        .join(format!("{}.xcodeproj", ios_config.app_name()))
        .join("project.pbxproj");

    if ios_dir.exists() && project_file.exists() && !force {
        println!(
            "iOS project already exists at {}. Use --force to regenerate it.",
            ios_dir.display()
        );
        return Ok(());
    }

    println!("Generating iOS project...");
    println!("  Bundle ID: {}", ios_config.bundle_id());
    println!("  Deployment Target: {}", ios_config.deployment_target());
    println!("  App Name: {}", ios_config.app_name());
    println!("  Library: {}", lib_name);

    generate_project(&ios_dir, &ios_config, &lib_name)?;

    println!(
        "\niOS project generated successfully at {}",
        ios_dir.display()
    );
    println!("\nNext steps:");
    println!("  Run: ignit build ios");

    Ok(())
}

fn generate_project(ios_dir: &Path, config: &config::IosConfig, lib_name: &str) -> Result<()> {
    let replacements = create_replacements(config, lib_name);
    let app_dir = ios_dir.join(config.app_name());

    // Create directory structure
    fs::create_dir_all(ios_dir.join(format!("{}.xcodeproj", config.app_name())))
        .context("Failed to create xcodeproj directory")?;
    fs::create_dir_all(app_dir.join("Assets.xcassets").join("AppIcon.appiconset"))
        .context("Failed to create Assets.xcassets directory")?;

    // Write template files
    write_template_file(
        ios_dir
            .join(format!("{}.xcodeproj", config.app_name()))
            .join("project.pbxproj"),
        PROJECT_PBXPROJ,
        &replacements,
    )?;
    write_template_file(app_dir.join("Info.plist"), INFO_PLIST, &replacements)?;
    write_template_file(app_dir.join("main.swift"), MAIN_SWIFT, &replacements)?;
    write_template_file(
        app_dir.join("Ignit-Bridging-Header.h"),
        BRIDGING_HEADER,
        &replacements,
    )?;
    write_template_file(
        app_dir.join("LaunchScreen.storyboard"),
        LAUNCH_SCREEN_STORYBOARD,
        &replacements,
    )?;
    write_template_file(
        app_dir.join("Assets.xcassets").join("Contents.json"),
        ASSETS_CONTENTS_JSON,
        &replacements,
    )?;
    write_template_file(
        app_dir
            .join("Assets.xcassets")
            .join("AppIcon.appiconset")
            .join("Contents.json"),
        APPICON_CONTENTS_JSON,
        &replacements,
    )?;

    // The old template used an @main AppDelegate. Remove it when regenerating
    // so a stale delegate cannot compete with `main.swift` for the entry point.
    let legacy_delegate = app_dir.join("AppDelegate.swift");
    if legacy_delegate.exists() {
        fs::remove_file(&legacy_delegate)
            .with_context(|| format!("Failed to remove {}", legacy_delegate.display()))?;
    }

    Ok(())
}

fn create_replacements(config: &config::IosConfig, lib_name: &str) -> Vec<(String, String)> {
    vec![
        (
            "@@@BUNDLE_ID@@@".to_string(),
            config.bundle_id().to_string(),
        ),
        (
            "@@@DEPLOYMENT_TARGET@@@".to_string(),
            config.deployment_target().to_string(),
        ),
        ("@@@APP_NAME@@@".to_string(), config.app_name().to_string()),
        ("@@@LIB_NAME@@@".to_string(), lib_name.to_string()),
        (
            "@@@RUST_TARGET@@@".to_string(),
            super::simulator_target()
                .unwrap_or("aarch64-apple-ios-sim")
                .to_string(),
        ),
    ]
}

fn write_template_file(
    path: PathBuf,
    content: &str,
    replacements: &[(String, String)],
) -> Result<()> {
    let replaced_content = replace_placeholders(content, replacements);
    fs::write(&path, replaced_content)
        .with_context(|| format!("Failed to write file: {}", path.display()))?;
    Ok(())
}

fn replace_placeholders(content: &str, replacements: &[(String, String)]) -> String {
    let mut result = content.to_string();
    for (placeholder, value) in replacements {
        result = result.replace(placeholder, value);
    }
    result
}

// Template files — pbxproj loaded from file to avoid Rust string escaping issues
const PROJECT_PBXPROJ: &str = include_str!("../template/ios/project.pbxproj");

const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>$(DEVELOPMENT_LANGUAGE)</string>
	<key>CFBundleDisplayName</key>
	<string>@@@APP_NAME@@@</string>
	<key>CFBundleExecutable</key>
	<string>$(EXECUTABLE_NAME)</string>
	<key>CFBundleIdentifier</key>
	<string>@@@BUNDLE_ID@@@</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>@@@APP_NAME@@@</string>
	<key>CFBundlePackageType</key>
	<string>$(PRODUCT_BUNDLE_PACKAGE_TYPE)</string>
	<key>CFBundleShortVersionString</key>
	<string>$(MARKETING_VERSION)</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>LSRequiresIPhoneOS</key>
	<true/>
	<key>UIFileSharingEnabled</key>
	<true/>
	<key>LSSupportsOpeningDocumentsInPlace</key>
	<true/>
	<key>UILaunchStoryboardName</key>
	<string>LaunchScreen</string>
	<key>UIRequiresFullScreen</key>
	<true/>
	<key>UISupportedInterfaceOrientations</key>
	<array>
		<string>UIInterfaceOrientationLandscapeLeft</string>
	</array>
	<key>UIStatusBarHidden</key>
	<false/>
	<key>CADisableMinimumFrameDurationOnPhone</key>
	<true/>
</dict>
</plist>
"#;

const MAIN_SWIFT: &str = r#"import Foundation

// winit owns `UIApplicationMain` on iOS, so the engine has to be started from
// top-level code rather than from an `@main` app delegate. The C declaration
// comes from Ignit-Bridging-Header.h.
autoreleasepool {
    ios_main()
}
"#;

const BRIDGING_HEADER: &str = r#"#ifndef IGNIT_BRIDGING_HEADER_H
#define IGNIT_BRIDGING_HEADER_H

// Entry point generated by `#[ignit::main]` for the iOS static library.
void ios_main(void);

#endif
"#;

const LAUNCH_SCREEN_STORYBOARD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<document type="com.apple.InterfaceBuilder3.CocoaTouch.Storyboard.XIB" version="3.0" toolsVersion="21701" targetRuntime="iOS.CocoaTouch" propertyAccessControl="none" useAutolayout="YES" launchScreen="YES" useTraitCollections="YES" useSafeAreas="YES" colorMatched="YES" initialViewController="01J-lp-oVM">
    <scenes>
        <scene sceneID="EHf-IW-A2E">
            <objects>
                <viewController id="01J-lp-oVM" sceneMemberID="viewController">
                    <view key="view" contentMode="scaleToFill" id="Ze5-6b-2t3">
                        <rect key="frame" x="0.0" y="0.0" width="393" height="852"/>
                        <autoresizingMask key="autoresizingMask" widthSizable="YES" heightSizable="YES"/>
                        <subviews>
                            <label opaque="NO" userInteractionEnabled="NO" contentMode="left" horizontalHuggingPriority="251" verticalHuggingPriority="251" text="@@@APP_NAME@@@" textAlignment="center" lineBreakMode="tailTruncation" baselineAdjustment="alignBaselines" adjustsFontSizeToFit="NO" translatesAutoresizingMaskIntoConstraints="NO" id="lbl-01">
                                <fontDescription key="fontDescription" type="system" pointSize="32"/>
                                <color key="textColor" white="1" alpha="1" colorSpace="custom" customColorSpace="genericGamma22GrayColorSpace"/>
                            </label>
                        </subviews>
                        <viewLayoutGuide key="safeArea" id="6Tk-OE-BBY"/>
                        <color key="backgroundColor" red="0.1" green="0.1" blue="0.12" alpha="1" colorSpace="custom" customColorSpace="sRGB"/>
                        <constraints>
                            <constraint firstItem="lbl-01" firstAttribute="centerX" secondItem="Ze5-6b-2t3" secondAttribute="centerX" id="c1"/>
                            <constraint firstItem="lbl-01" firstAttribute="centerY" secondItem="Ze5-6b-2t3" secondAttribute="centerY" id="c2"/>
                        </constraints>
                    </view>
                </viewController>
                <placeholder placeholderIdentifier="IBFirstResponder" id="iYj-Kq-Ea1" userLabel="First Responder" sceneMemberID="firstResponder"/>
            </objects>
            <point key="canvasLocation" x="53" y="375"/>
        </scene>
    </scenes>
</document>
"#;

const ASSETS_CONTENTS_JSON: &str = r#"{
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
"#;

const APPICON_CONTENTS_JSON: &str = r#"{
  "images" : [
    {
      "idiom" : "universal",
      "platform" : "ios",
      "size" : "1024x1024"
    }
  ],
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
"#;
