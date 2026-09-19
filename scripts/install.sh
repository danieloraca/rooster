#!/bin/bash
# Build (or use an explicit local bundle), install for this user, and launch.
set -euo pipefail

usage() {
    cat <<'HELP'
Usage: ./scripts/install.sh [--app /path/to/Rooster.app] [--install-dir /path] [--no-open]

By default, build the native macOS release from this checkout, install it into
~/Applications/Rooster.app, and open it. No sudo is needed.

  --app PATH         Install an existing local bundle without building.
  --install-dir DIR  Choose an Applications directory (default: ~/Applications).
  --no-open          Install without launching, useful for CI.
  --help             Show this help.

Quit Rooster before updating. A previous installation is kept in a uniquely
named backup folder beside the installed app. Settings/drafts remain untouched.
HELP
}
fail() { printf 'rooster install: %s\n' "$*" >&2; exit 1; }
require_command() { command -v "$1" >/dev/null 2>&1 || fail "Missing $1. See docs/release.md for build requirements."; }
rooster_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
rooster_bundle=''
rooster_install_dir="${HOME:?Cannot resolve your home directory}/Applications"
rooster_open=true
while [[ $# -gt 0 ]]; do
    case "$1" in
        --app|--install-dir)
            [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || fail "$1 requires a path"
            if [[ "$1" == --app ]]; then rooster_bundle="$2"; else rooster_install_dir="$2"; fi
            shift 2 ;;
        --no-open) rooster_open=false; shift ;;
        --help|-h) usage; exit 0 ;;
        *) fail "Unknown option: $1 (use --help)" ;;
    esac
done
# Resolve caller-supplied relative paths before the build changes directories.
case "$rooster_install_dir" in /*) ;; *) rooster_install_dir="$PWD/$rooster_install_dir" ;; esac
if [[ -n "$rooster_bundle" && "$rooster_bundle" != /* ]]; then rooster_bundle="$PWD/$rooster_bundle"; fi
case "$rooster_install_dir/" in *.app/*) fail 'Choose an Applications directory, not a path inside an app bundle.' ;; esac
[[ "$(uname -s)" == Darwin ]] || fail 'This installer supports macOS only.'
require_command git
# Never stop an app that might contain unsaved edits.
ensure_stopped() {
    rooster_process_status=0
    /usr/bin/pgrep -x rooster-desktop >/dev/null 2>&1 || rooster_process_status=$?
    case "$rooster_process_status" in
        0) fail 'Quit Rooster before installing or updating, then run this command again.' ;;
        1) ;; # No matching process.
        *) fail 'Cannot check whether Rooster is running; installation has not started.' ;;
    esac
}
ensure_stopped

if [[ -z "$rooster_bundle" ]]; then
    for rooster_tool in node npm cargo rustc; do require_command "$rooster_tool"; done
    node -e 'const [a,b]=process.versions.node.split(".").map(Number);process.exit(a>22||(a===22&&b>=12)?0:1)' \
        || fail 'Building requires Node.js 22.12 or newer.'
    /usr/bin/xcrun --find clang >/dev/null 2>&1 || fail 'Install Xcode Command Line Tools with: xcode-select --install'
    cd "$rooster_root"
    rooster_target_dir="$(cargo metadata --locked --no-deps --format-version 1 | node -e 'let s="";process.stdin.on("data",c=>s+=c);process.stdin.on("end",()=>process.stdout.write(JSON.parse(s).target_directory))')"
    rooster_target="$(rustc -vV | sed -n 's/^host: //p')"
    [[ "$rooster_target" == *-apple-darwin && -n "$rooster_target_dir" ]] || fail 'Cannot resolve the native macOS Rust target.'
    # Resolve once, so relative Cargo target overrides keep the same meaning.
    export CARGO_TARGET_DIR="$rooster_target_dir"
    printf 'Building Rooster for %s…\n' "$rooster_target"
    (
        cd "$rooster_root/apps/desktop"
        # This command builds a local pilot, irrespective of release credentials
        # that might be present in the caller's shell.
        unset APPLE_SIGNING_IDENTITY APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD
        unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH
        npm ci
        npm run desktop:bundle -- --target "$rooster_target" -- --locked
    )
    rooster_bundle="$rooster_target_dir/$rooster_target/release/bundle/macos/Rooster.app"
fi

validate_bundle() {
    [[ -d "$1" && ! -L "$1" && -f "$1/Contents/Info.plist" && -x "$1/Contents/MacOS/rooster-desktop" ]] \
        || fail "Not a complete Rooster application: $1"
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$1/Contents/Info.plist")" == dev.rooster.desktop ]] \
        || fail "Unexpected application identifier in $1"
    /usr/bin/codesign --verify --deep --strict "$1" || fail "Application signature verification failed: $1"
}
validate_bundle "$rooster_bundle"
rooster_bundle="$(cd "$rooster_bundle" && pwd -P)"
mkdir -p "$rooster_install_dir"
rooster_install_dir="$(cd "$rooster_install_dir" && pwd -P)"
rooster_destination="$rooster_install_dir/Rooster.app"
[[ "$rooster_bundle" != "$rooster_destination" ]] || fail 'The source is already the installed app; open it directly.'
case "$rooster_install_dir/" in "$rooster_bundle/"*) fail 'The install directory cannot be inside the source app.' ;; esac
[[ ! -L "$rooster_destination" ]] || fail "Refusing to replace a symbolic link: $rooster_destination"
if [[ -e "$rooster_destination" ]]; then validate_bundle "$rooster_destination"; fi
rooster_lock="$rooster_install_dir/.rooster-install.lock"
mkdir "$rooster_lock" 2>/dev/null || fail "Another installer may be active. If none is running, remove the stale directory: $rooster_lock"
rooster_stage=''
rooster_backup=''
cleanup() {
    rooster_status=$?
    trap - EXIT
    # If replacement failed after moving the old app, put that app back.
    if [[ $rooster_status -ne 0 && -n "$rooster_backup" && -d "$rooster_backup/Rooster.app" && ! -e "$rooster_destination" && ! -L "$rooster_destination" ]]; then
        /bin/mv "$rooster_backup/Rooster.app" "$rooster_destination" \
            || printf 'Previous app remains at %s/Rooster.app\n' "$rooster_backup" >&2
    fi
    if [[ -n "$rooster_stage" ]]; then /bin/rm -rf "$rooster_stage"; fi
    /bin/rmdir "$rooster_lock" || true
    exit "$rooster_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
rooster_stage="$(mktemp -d "$rooster_install_dir/.rooster-install.XXXXXX")"
/usr/bin/ditto "$rooster_bundle" "$rooster_stage/Rooster.app"
validate_bundle "$rooster_stage/Rooster.app"
ensure_stopped
# Recheck after staging and acquiring the cooperative installer lock.
[[ ! -L "$rooster_destination" ]] || fail "Destination became a symbolic link: $rooster_destination"
if [[ -e "$rooster_destination" ]]; then
    validate_bundle "$rooster_destination"
    rooster_backup="$(mktemp -d "$rooster_install_dir/Rooster-backup.XXXXXX")"
    /bin/mv "$rooster_destination" "$rooster_backup/Rooster.app"
fi
/bin/mv "$rooster_stage/Rooster.app" "$rooster_destination"
validate_bundle "$rooster_destination"
printf 'Installed: %s\n' "$rooster_destination"
if [[ -n "$rooster_backup" ]]; then printf 'Previous app: %s/Rooster.app\n' "$rooster_backup"; fi
if [[ "$rooster_open" == true ]]; then
    rooster_launch=(-a "$rooster_destination")
    # LaunchServices otherwise drops shell-only overrides used for isolated trials.
    for rooster_variable in ROOSTER_CONFIG ROOSTER_DATA_DIR; do
        if [[ -n "${!rooster_variable:-}" ]]; then
            rooster_launch+=(--env "$rooster_variable=${!rooster_variable}")
        fi
    done
    /usr/bin/open "${rooster_launch[@]}" || fail "Installed, but launch failed. Open $rooster_destination manually."
    printf 'Opened Rooster.\n'
fi
