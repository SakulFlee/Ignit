#!/usr/bin/env bash
set -euo pipefail

# Generate the crate READMEs for the two main entrypoints from the repository
# README plus a crate-specific header. Relative repository links are rewritten
# to absolute URLs because crates.io renders READMEs outside the repository.
#
# Usage:
#   ./generate-crate-readmes.sh          # write the README files
#   ./generate-crate-readmes.sh --check  # fail if they are out of date

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FORGEJO="https://forgejo.sakul-flee.de/SakulFlee/Ignit"

render() {
    local header="$1"
    local target="$2"

    {
        cat "$header"
        printf '\n'
        sed -E \
            -e "s|src=\"\\.github/|src=\"$FORGEJO/raw/branch/main/.github/|g" \
            -e "s|\\]\\(\\.github/|]($FORGEJO/raw/branch/main/.github/|g" \
            -e "s|\\]\\(Images/|]($FORGEJO/raw/branch/main/Images/|g" \
            -e "s|\\]\\(PERFORMANCE_REPORTING\\.md\\)|]($FORGEJO/src/branch/main/PERFORMANCE_REPORTING.md)|g" \
            -e "s|\\]\\(AGENTS\\.md\\)|]($FORGEJO/src/branch/main/AGENTS.md)|g" \
            "$ROOT/README.md"
    } > "$target"
}

pairs=(
    "$ROOT/.forgejo/ci/readme-headers/ignit.md:$ROOT/Crates/ignit/README.md"
    "$ROOT/.forgejo/ci/readme-headers/cli.md:$ROOT/Crates/cli/README.md"
)

if [ "${1:-}" = "--check" ]; then
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT

    failed=0
    for pair in "${pairs[@]}"; do
        header="${pair%%:*}"
        target="${pair##*:}"
        output="$tmp/$(basename "$(dirname "$target")")-README.md"

        render "$header" "$output"

        if ! diff -u "$target" "$output"; then
            echo "Out of date: $target" >&2
            failed=1
        fi
    done

    exit "$failed"
fi

for pair in "${pairs[@]}"; do
    header="${pair%%:*}"
    target="${pair##*:}"

    render "$header" "$target"
    echo "wrote $target"
done
