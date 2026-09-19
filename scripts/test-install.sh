#!/bin/bash
# Exercise real bundle validation/install/update without opening the GUI.
set -euo pipefail
[[ $# -eq 1 ]] || { printf 'Usage: %s /path/to/built/Rooster.app\n' "$0" >&2; exit 1; }
rooster_test_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
rooster_test_app="$(cd "$1" && pwd -P)"
rooster_test_dir="$(mktemp -d "${TMPDIR:-/tmp}/rooster-install-test.XXXXXX")"
trap 'rm -rf "$rooster_test_dir"' EXIT
rooster_test_dest="$rooster_test_dir/Applications ü with spaces"
rooster_test_install="$rooster_test_root/scripts/install.sh"
"$rooster_test_install" --app "$rooster_test_app" --install-dir "$rooster_test_dest" --no-open
cmp "$rooster_test_app/Contents/MacOS/rooster-desktop" "$rooster_test_dest/Rooster.app/Contents/MacOS/rooster-desktop"
"$rooster_test_install" --app "$rooster_test_app" --install-dir "$rooster_test_dest" --no-open
rooster_test_backups=("$rooster_test_dest"/Rooster-backup.*/Rooster.app)
[[ ${#rooster_test_backups[@]} -eq 1 && -d "${rooster_test_backups[0]}" ]]
codesign --verify --deep --strict "${rooster_test_backups[0]}"
cmp "$rooster_test_app/Contents/MacOS/rooster-desktop" "${rooster_test_backups[0]}/Contents/MacOS/rooster-desktop"
# A corrupted candidate must leave the valid installation unchanged.
ditto "$rooster_test_app" "$rooster_test_dir/Broken.app"
printf '\ncorruption\n' >> "$rooster_test_dir/Broken.app/Contents/MacOS/rooster-desktop"
if "$rooster_test_install" --app "$rooster_test_dir/Broken.app" --install-dir "$rooster_test_dest" --no-open; then
    printf 'Corrupted bundle was incorrectly accepted\n' >&2; exit 1
fi
codesign --verify --deep --strict "$rooster_test_dest/Rooster.app"
cmp "$rooster_test_app/Contents/MacOS/rooster-desktop" "$rooster_test_dest/Rooster.app/Contents/MacOS/rooster-desktop"
# Never follow a destination app symlink or merge an installation into it.
mkdir "$rooster_test_dir/Linked"
ln -s "$rooster_test_app" "$rooster_test_dir/Linked/Rooster.app"
if "$rooster_test_install" --app "$rooster_test_app" --install-dir "$rooster_test_dir/Linked" --no-open; then
    printf 'Symbolic-link destination was incorrectly accepted\n' >&2; exit 1
fi
[[ -L "$rooster_test_dir/Linked/Rooster.app" ]]
[[ ! -e "$rooster_test_dest/.rooster-install.lock" ]]
printf 'Installer checks passed: fresh install, retained backup, invalid-bundle rejection, symbolic-link rejection.\n'
