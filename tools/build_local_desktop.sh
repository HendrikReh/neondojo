#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$root"

target="$root/target/local-desktop"
lock="$target/.build-lock"
mkdir -p "$target"
if ! mkdir "$lock" 2>/dev/null; then
    printf 'Could not lock the local desktop build at %s\n' "$lock" >&2
    exit 1
fi
cleanup() {
    rm -rf "$lock"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

# Ensure Metal shader compilation (gpui_macos/build.rs -> xcrun metal) finds a
# full Xcode Developer directory when xcode-select still points at CLT.
detect_metal_developer_dir() {
    if xcrun -f metal >/dev/null 2>&1; then
        return 0
    fi

    local candidate
    for candidate in \
        "/Volumes/OWC Envoy Ultra/Applications/Xcode.app/Contents/Developer" \
        "/Applications/Xcode.app/Contents/Developer"; do
        if [ -d "$candidate" ] && DEVELOPER_DIR="$candidate" xcrun -f metal >/dev/null 2>&1; then
            export DEVELOPER_DIR="$candidate"
            printf 'Using DEVELOPER_DIR=%s for Metal shader compilation\n' "$DEVELOPER_DIR" >&2
            return 0
        fi
    done

    printf 'Error: could not locate the Metal shader compiler via `xcrun -f metal`.\n' >&2
    printf 'Install Xcode (and run `xcodebuild -downloadComponent MetalToolchain` if needed)\n' >&2
    printf 'or export DEVELOPER_DIR pointing to Xcode.app/Contents/Developer.\n' >&2
    return 1
}

if [ "$(uname -s)" = "Darwin" ]; then
    detect_metal_developer_dir
fi

# Workspace Cargo.toml sets strip = "none" on dev/release and build-override
# profiles to avoid Rust #157750 LINKEDIT failures on macOS 27. Refuse
# environment overrides that would silently re-enable stripping.
if [ -n "${CARGO_PROFILE_RELEASE_STRIP:-}" ] && [ "${CARGO_PROFILE_RELEASE_STRIP}" != "none" ] && [ "${CARGO_PROFILE_RELEASE_STRIP}" != "false" ]; then
    printf 'Error: CARGO_PROFILE_RELEASE_STRIP=%s conflicts with workspace strip = "none"\n' "$CARGO_PROFILE_RELEASE_STRIP" >&2
    exit 1
fi

# Rebuild neondojo so sqlx::migrate! always embeds this checkout's migrations.
cargo clean --release --package neondojo --target-dir "$target"
cargo build --release --package neondojo-desktop --bin neondojo-desktop --target-dir "$target" "$@"

binary="$target/release/neondojo-desktop"
app_out="$target/NeonDojo.app"

"$root/tools/package_macos_app.sh" --binary "$binary" --output "$app_out"

printf 'Local desktop binary: %s\n' "$binary"
printf 'Local macOS app bundle: %s\n' "$app_out"
