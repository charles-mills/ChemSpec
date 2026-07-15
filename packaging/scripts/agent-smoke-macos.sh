#!/usr/bin/env bash

set -euo pipefail

readonly expected_packager="cargo-packager 0.11.8"
readonly app_name="ChemSpec Agent Smoke"
readonly bundle_id="dev.charlesmills.chemspec.agent-smoke"
readonly executable_name="chemspec-agent-smoke"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
out_dir="$repo_root/target/agent-smoke"
staging_dir="$repo_root/target/agent-smoke-bin"
bundle="$out_dir/$app_name.app"
source_executable="$repo_root/target/debug/chemspec-app"
bundled_executable="$bundle/Contents/MacOS/$executable_name"
plist="$bundle/Contents/Info.plist"
config="$repo_root/packaging/Packager.agent-smoke.toml"

stop_smoke() {
    local pids
    pids="$(pgrep -x "$executable_name" || true)"
    if [[ -z "$pids" ]]; then
        echo "$app_name is not running."
        return
    fi

    while IFS= read -r pid; do
        kill "$pid"
    done <<< "$pids"

    for _ in {1..40}; do
        if ! pgrep -x "$executable_name" >/dev/null; then
            echo "Stopped $app_name."
            return
        fi
        sleep 0.05
    done

    echo "Timed out waiting for $app_name to stop." >&2
    exit 1
}

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "The agent smoke bundle is currently supported only on macOS." >&2
    exit 1
fi

mode="${1:-2d}"
if [[ "$mode" == "stop" ]]; then
    stop_smoke
    exit 0
fi

case "$mode" in
    2d)
        smoke_argument="--structural-2d-smoke"
        smoke_title="Structural 2D"
        ;;
    3d)
        smoke_argument="--structural-3d-smoke"
        smoke_title="Structural 3D"
        ;;
    *)
        echo "Usage: just agent-smoke [2d|3d|stop]" >&2
        exit 2
        ;;
esac

if ! packager_version="$(cargo packager --version 2>/dev/null)"; then
    echo "cargo-packager 0.11.8 is required." >&2
    echo "Install it with: cargo install cargo-packager --version 0.11.8 --locked" >&2
    exit 1
fi
if [[ "$packager_version" != "$expected_packager" ]]; then
    echo "Expected $expected_packager, found $packager_version." >&2
    exit 1
fi

stop_smoke

case "$out_dir:$staging_dir" in
    "$repo_root/target/agent-smoke:$repo_root/target/agent-smoke-bin") ;;
    *)
        echo "Refusing to clean unexpected smoke paths." >&2
        exit 1
        ;;
esac

rm -rf "$out_dir" "$staging_dir"
mkdir -p "$out_dir" "$staging_dir"

cargo build -p chemspec-app --locked
install -m 755 "$source_executable" "$staging_dir/$executable_name"
cargo packager --config "$config" --formats app

if [[ ! -x "$bundled_executable" || ! -f "$plist" ]]; then
    echo "cargo-packager did not create the expected app bundle." >&2
    exit 1
fi

actual_name="$(plutil -extract CFBundleDisplayName raw -o - "$plist")"
actual_id="$(plutil -extract CFBundleIdentifier raw -o - "$plist")"
actual_executable="$(plutil -extract CFBundleExecutable raw -o - "$plist")"

if [[ "$actual_name" != "$app_name" ]]; then
    echo "Unexpected bundle name: $actual_name" >&2
    exit 1
fi
if [[ "$actual_id" != "$bundle_id" ]]; then
    echo "Unexpected bundle identifier: $actual_id" >&2
    exit 1
fi
if [[ "$actual_executable" != "$executable_name" ]]; then
    echo "Unexpected bundle executable: $actual_executable" >&2
    exit 1
fi
if ! cmp -s "$source_executable" "$bundled_executable"; then
    echo "Bundled executable does not match the freshly built binary." >&2
    exit 1
fi

binary_sha="$(shasum -a 256 "$bundled_executable" | awk '{print $1}')"
open -n "$bundle" --args "$smoke_argument"

cat <<EOF
Computer Use app: $app_name
Window title: $app_name — $smoke_title
Bundle: $bundle
Bundle identifier: $bundle_id
Binary SHA-256: $binary_sha
EOF
