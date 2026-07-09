#!/usr/bin/env bash
#
# Build everything: the Rust workspace and the browser frontend.
#
#   contrib/build.sh            release build
#   contrib/build.sh --debug    debug build, skips the web bundle
#
# The result is a single binary at target/release/asmteacher that serves the
# API and the built frontend from web/dist.

set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="release"
CARGO_FLAGS=(--release)
BUILD_WEB=1

for arg in "$@"; do
    case "$arg" in
        --debug)   PROFILE="debug"; CARGO_FLAGS=(); BUILD_WEB=0 ;;
        --no-web)  BUILD_WEB=0 ;;
        -h|--help) sed -n '2,10p' "$0" | sed 's/^# \?//'; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

say() { printf '\n\033[1;34m==>\033[0m %s\n' "$*"; }

say "Building Rust workspace (${PROFILE})"
cargo build --workspace --all-targets "${CARGO_FLAGS[@]}"

if [[ "$BUILD_WEB" == 1 ]]; then
    if [[ -d web && -f web/package.json ]]; then
        say "Installing frontend dependencies"
        # `npm ci` needs a lockfile; fall back for a fresh checkout.
        (cd web && { npm ci --no-audit --no-fund || npm install --no-audit --no-fund; })

        say "Building frontend bundle"
        (cd web && npm run build)
    else
        echo "web/ not present, skipping frontend" >&2
    fi
fi

say "Done"
if [[ "$PROFILE" == "release" ]]; then
    echo "  server: target/release/asmteacher"
    echo "  run:    target/release/asmteacher --listen 127.0.0.1:8080 --web web/dist --lessons lessons"
fi
