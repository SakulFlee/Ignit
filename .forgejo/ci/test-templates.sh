#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CARGO_TARGET_ROOT="${CARGO_TARGET_DIR:-$ROOT/target}"
CLI="$CARGO_TARGET_ROOT/debug/ignit"
TEMPLATE_TARGET_DIR="$CARGO_TARGET_ROOT/template-check"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/ignit-templates.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT


required_example_assets=(
    "Examples/all-in-one/Assets/Models/DamagedHelmet.glb"
    "Examples/all-in-one/Assets/Scenes/procgeo_demo.ron"
)

for asset in "${required_example_assets[@]}"; do
    if [[ ! -f "$ROOT/$asset" ]]; then
        echo "Missing example asset: $asset" >&2
        exit 1
    fi
done

# The publishable CLI crate carries copies of the example template sources.
# Keep them byte-for-byte identical to the canonical workspace examples.
vendored_templates=(
    "Examples/minimal-3d/src/lib.rs:Crates/cli/src/template/app/minimal-3d.rs"
    "Examples/minimal-2d/src/lib.rs:Crates/cli/src/template/app/minimal-2d.rs"
    "Examples/all-in-one/src/lib.rs:Crates/cli/src/template/app/all-in-one.rs"
    "Examples/all-in-one/src/custom_shader.wgsl:Crates/cli/src/template/app/all-in-one-custom-shader.wgsl"
    "Examples/all-in-one/Assets/Scenes/procgeo_demo.ron:Crates/cli/src/template/app/procgeo_demo.ron"
    "Examples/all-in-one/Assets/Models/DamagedHelmet.glb:Crates/cli/src/template/app/DamagedHelmet.glb"
)

for pair in "${vendored_templates[@]}"; do
    source="${pair%%:*}"
    copy="${pair##*:}"

    if ! cmp -s "$ROOT/$source" "$ROOT/$copy"; then
        echo "Vendored template is out of date: $copy" >&2
        echo "Copy it from $source" >&2
        exit 1
    fi
done

echo "Building the ignit CLI..."
cargo build --manifest-path "$ROOT/Cargo.toml" -p ignit-cli

for template in minimal-3d all-in-one minimal-2d; do
    project="$TMP/$template"

    echo "Checking template '$template'..."
    "$CLI" init "template-check-$template" \
        --yes \
        --template "$template" \
        --project-path "$project" \
        --engine-path "$ROOT"

    for file in Cargo.toml Ignit.toml src/lib.rs src/main.rs Assets/.gitkeep; do
        if [[ ! -e "$project/$file" ]]; then
            echo "Missing generated file: $project/$file" >&2
            exit 1
        fi
    done

    if [[ "$template" == "all-in-one" ]]; then
        for file in src/custom_shader.wgsl Assets/Scenes/procgeo_demo.ron Assets/Models/DamagedHelmet.glb; do
            if [[ ! -s "$project/$file" ]]; then
                echo "Missing generated all-in-one asset: $project/$file" >&2
                exit 1
            fi
        done
    fi

    if grep -Rq "{{PROJECT_NAME}}" "$project/src"; then
        echo "Unreplaced {{PROJECT_NAME}} placeholder in $project/src" >&2
        exit 1
    fi

    # Reuse the workspace lockfile so dependency resolution does not drift
    # between the engine and the generated project.
    cp "$ROOT/Cargo.lock" "$project/Cargo.lock"

    (
        cd "$project"
        CARGO_TARGET_DIR="$TEMPLATE_TARGET_DIR" cargo check
    )
done

echo "All templates generated and checked successfully."
