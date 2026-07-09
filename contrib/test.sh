#!/usr/bin/env bash
#
# Run the complete test suite.
#
#   contrib/test.sh              everything
#   contrib/test.sh --fast       skip clippy and the frontend
#   contrib/test.sh --rust       Rust only
#   contrib/test.sh --web        frontend only
#
# Exit status is non-zero if anything fails. Every step is announced, and a
# summary at the end says what ran and what was skipped — a suite that silently
# skips half of itself is worse than no suite at all.

set -uo pipefail

cd "$(dirname "$0")/.."

FAST=0
DO_RUST=1
DO_WEB=1

for arg in "$@"; do
    case "$arg" in
        --fast) FAST=1 ;;
        --rust) DO_WEB=0 ;;
        --web)  DO_RUST=0 ;;
        -h|--help) sed -n '2,12p' "$0" | sed 's/^# \?//'; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

FAILED=()
SKIPPED=()
PASSED=()

say()  { printf '\n\033[1;34m==>\033[0m %s\n' "$*"; }
skip() { printf '\033[1;33m--- skipped:\033[0m %s\n' "$*"; SKIPPED+=("$1"); }

# Run a step, recording whether it passed. Never aborts the suite early: we want
# the full picture in one run, not the first failure.
step() {
    local name="$1"; shift
    say "$name"
    if "$@"; then
        PASSED+=("$name")
    else
        printf '\033[1;31m!!! FAILED:\033[0m %s\n' "$name"
        FAILED+=("$name")
    fi
}

have() { command -v "$1" >/dev/null 2>&1; }

# ---------------------------------------------------------------------------
# The differential tests compare our decoder against objdump and our assembler
# against nasm. They skip themselves when the tools are missing, which means a
# green run on a bare machine proves much less. Say so loudly.
# ---------------------------------------------------------------------------
say "Environment"
for tool in cargo nasm objdump gcc readelf node; do
    if have "$tool"; then
        printf '  %-10s %s\n' "$tool" "$(command -v "$tool")"
    else
        printf '  \033[1;33m%-10s MISSING\033[0m — tests that depend on it will skip\n' "$tool"
    fi
done
if ! have nasm || ! have objdump; then
    echo
    echo "  WARNING: without nasm and objdump the differential tests do not run."
    echo "           Use contrib/Dockerfile for a complete environment."
fi

if [[ "$DO_RUST" == 1 ]]; then
    step "cargo fmt --check" cargo fmt --all -- --check

    if [[ "$FAST" == 0 ]]; then
        step "cargo clippy" cargo clippy --workspace --all-targets -- -D warnings
    else
        skip "cargo clippy (--fast)"
    fi

    step "cargo test (workspace)" cargo test --workspace --all-targets
    step "cargo test (doc tests)" cargo test --workspace --doc
else
    skip "rust"
fi

if [[ "$DO_WEB" == 1 && "$FAST" == 0 ]]; then
    if [[ -f web/package.json ]] && have node; then
        if [[ ! -d web/node_modules ]]; then
            say "Installing frontend dependencies"
            (cd web && { npm ci --no-audit --no-fund || npm install --no-audit --no-fund; }) \
                || FAILED+=("npm install")
        fi
        step "tsc --noEmit"  bash -c 'cd web && npx tsc --noEmit'
        step "web unit tests" bash -c 'cd web && npm test --silent'
        step "vite build"    bash -c 'cd web && npm run build'
    else
        skip "frontend (no web/package.json or no node)"
    fi
else
    skip "frontend"
fi

# ---------------------------------------------------------------------------
say "Summary"
for p in "${PASSED[@]:-}";  do [[ -n "$p" ]] && printf '  \033[1;32mpass\033[0m  %s\n' "$p"; done
for s in "${SKIPPED[@]:-}"; do [[ -n "$s" ]] && printf '  \033[1;33mskip\033[0m  %s\n' "$s"; done
for f in "${FAILED[@]:-}";  do [[ -n "$f" ]] && printf '  \033[1;31mFAIL\033[0m  %s\n' "$f"; done

if [[ ${#FAILED[@]} -gt 0 ]]; then
    echo
    echo "${#FAILED[@]} step(s) failed."
    exit 1
fi
echo
echo "All ${#PASSED[@]} step(s) passed."
