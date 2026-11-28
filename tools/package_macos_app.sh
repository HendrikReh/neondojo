#!/usr/bin/env bash
set -euo pipefail

root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)

binary="$root/target/local-desktop/release/neondojo-desktop"
app_out="$root/target/local-desktop/NeonDojo.app"
cargo_toml="$root/crates/neondojo-desktop/Cargo.toml"

usage() {
    printf 'Usage: %s [--binary PATH] [--output PATH]\n' "$(basename -- "$0")" >&2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --binary)
            [ "$#" -ge 2 ] || { usage; exit 1; }
            binary="$2"
            shift 2
            ;;
        --output)
            [ "$#" -ge 2 ] || { usage; exit 1; }
            app_out="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'Unknown option: %s\n' "$1" >&2
            usage
            exit 1
            ;;
    esac
done

if [ ! -f "$binary" ]; then
    printf 'Error: desktop binary not found at %s\n' "$binary" >&2
    exit 1
fi

if [ ! -f "$cargo_toml" ]; then
    printf 'Error: desktop Cargo.toml not found at %s\n' "$cargo_toml" >&2
    exit 1
fi

# Extract [package] version from crates/neondojo-desktop/Cargo.toml.
version=$(awk '
    /^\[package\][[:space:]]*$/ { in_pkg = 1; next }
    /^\[/ { in_pkg = 0 }
    in_pkg && /^[[:space:]]*version[[:space:]]*=/ {
        sub(/^[^"]*"/, "")
        sub(/".*$/, "")
        print
        exit
    }
' "$cargo_toml")

if [ -z "$version" ]; then
    printf 'Error: could not read [package] version from %s\n' "$cargo_toml" >&2
    exit 1
fi

logo_svg=""
for candidate in \
    "$root/crates/neondojo-desktop/assets/neondojo-logo.svg" \
    "$root/assets/neondojo-logo.svg" \
    "$root/web/public/neondojo-logo.svg"; do
    if [ -f "$candidate" ]; then
        logo_svg="$candidate"
        break
    fi
done

if [ -z "$logo_svg" ]; then
    printf 'Error: neondojo-logo.svg not found in desktop, shared, or web assets\n' >&2
    exit 1
fi

out_parent=$(dirname -- "$app_out")
mkdir -p "$out_parent"

work_dir=$(mktemp -d "$out_parent/package-macos-app.XXXXXX")
cleanup() {
    rm -rf "$work_dir"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

staged_app="$work_dir/NeonDojo.app"
contents_dir="$staged_app/Contents"
macos_dir="$contents_dir/MacOS"
resources_dir="$contents_dir/Resources"

mkdir -p "$macos_dir" "$resources_dir"
cp "$binary" "$macos_dir/neondojo-desktop"
chmod 755 "$macos_dir/neondojo-desktop"
cp "$logo_svg" "$resources_dir/neondojo-logo.svg"

# Bundle local BGBlitz theme ZIP archives when present in themes/ so the
# packaged NeonDojo.app finds them even when launched outside the repo checkout.
if [ -d "$root/themes" ]; then
    shopt -s nullglob
    theme_zips=("$root/themes/"*.zip "$root/themes/"*.ZIP)
    shopt -u nullglob
    if [ "${#theme_zips[@]}" -gt 0 ]; then
        mkdir -p "$resources_dir/themes"
        cp "${theme_zips[@]}" "$resources_dir/themes/"
    fi
fi

# Optional .icns generation when sips and iconutil (plus an SVG rasterizer or
# explicit NEONDOJO_ICON_PNG) are available.
generate_icns() {
    local svg_path="$1"
    local icns_out="$2"

    command -v sips >/dev/null 2>&1 || return 1
    command -v iconutil >/dev/null 2>&1 || return 1

    local base_png=""
    if [ -n "${NEONDOJO_ICON_PNG:-}" ] && [ -f "${NEONDOJO_ICON_PNG}" ]; then
        base_png="${NEONDOJO_ICON_PNG}"
    elif command -v rsvg-convert >/dev/null 2>&1; then
        base_png="$work_dir/icon-1024.png"
        rsvg-convert -w 1024 -h 1024 "$svg_path" -o "$base_png" >/dev/null 2>&1 || return 1
    elif command -v qlmanage >/dev/null 2>&1; then
        local ql_dir="$work_dir/ql"
        mkdir -p "$ql_dir"
        if qlmanage -t -s 1024 -o "$ql_dir" "$svg_path" >/dev/null 2>&1; then
            local ql_png="$ql_dir/$(basename -- "$svg_path").png"
            if [ -s "$ql_png" ]; then
                base_png="$ql_png"
            fi
        fi
    fi

    [ -n "$base_png" ] && [ -s "$base_png" ] || return 1

    local iconset_dir="$work_dir/AppIcon.iconset"
    mkdir -p "$iconset_dir"
    local entry size name
    for entry in \
        "16:icon_16x16.png" \
        "32:icon_16x16@2x.png" \
        "32:icon_32x32.png" \
        "64:icon_32x32@2x.png" \
        "128:icon_128x128.png" \
        "256:icon_128x128@2x.png" \
        "256:icon_256x256.png" \
        "512:icon_256x256@2x.png" \
        "512:icon_512x512.png" \
        "1024:icon_512x512@2x.png"; do
        size="${entry%%:*}"
        name="${entry#*:}"
        sips -z "$size" "$size" -s format png "$base_png" --out "$iconset_dir/$name" >/dev/null 2>&1 || return 1
    done

    iconutil -c icns "$iconset_dir" -o "$icns_out" >/dev/null 2>&1 || return 1
    [ -s "$icns_out" ]
}

icon_plist_entry=""
if generate_icns "$logo_svg" "$resources_dir/AppIcon.icns"; then
    icon_plist_entry=$'    <key>CFBundleIconFile</key>\n    <string>AppIcon</string>'
else
    printf 'Note: skipped AppIcon.icns generation (neondojo-logo.svg bundled in Resources)\n' >&2
fi

cat > "$contents_dir/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>NeonDojo</string>
    <key>CFBundleExecutable</key>
    <string>neondojo-desktop</string>
    <key>CFBundleIdentifier</key>
    <string>dev.neondojo.desktop</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>NeonDojo</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>${version}</string>
    <key>CFBundleVersion</key>
    <string>${version}</string>
${icon_plist_entry:+${icon_plist_entry}
}    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
EOF

if command -v plutil >/dev/null 2>&1; then
    plutil -lint "$contents_dir/Info.plist" >/dev/null
fi

if command -v codesign >/dev/null 2>&1; then
    codesign -s - --force "$macos_dir/neondojo-desktop"
    codesign -s - --force "$staged_app"
    codesign --verify --strict "$staged_app"
elif [ "$(uname -s)" = "Darwin" ]; then
    printf 'Error: codesign not found on macOS\n' >&2
    exit 1
fi

# Replace the previous app bundle only after staging and signing succeeded.
backup_app=""
if [ -e "$app_out" ]; then
    backup_app="$work_dir/NeonDojo.app.prev"
    mv "$app_out" "$backup_app"
fi
if ! mv "$staged_app" "$app_out"; then
    if [ -n "$backup_app" ] && [ -e "$backup_app" ]; then
        mv "$backup_app" "$app_out" || true
    fi
    exit 1
fi

printf 'Packaged %s (version %s)\n' "$app_out" "$version"
