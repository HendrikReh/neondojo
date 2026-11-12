#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

target="$root/target/local-restart"
lock="$target/.build-lock"
mkdir -p "$target"
if ! mkdir "$lock"; then
    printf 'Could not lock the local restart build at %s\n' "$lock" >&2
    exit 1
fi
trap 'rm -f "$lock/bgtutor"; rmdir "$lock"' EXIT
trap 'exit 1' HUP INT TERM

# Rebuild this package so sqlx::migrate! embeds this checkout's migrations.
cargo clean --release --package bgtutor --target-dir "$target"
cargo build --release --package bgtutor --bin bgtutor --target-dir "$target"
npm run build --prefix web

# Keep the previous launch binary usable until both builds have succeeded.
cp "$target/release/bgtutor" "$lock/bgtutor"
mv -f "$lock/bgtutor" "$target/bgtutor"
printf 'Local restart binary: %s\n' "$target/bgtutor"
