# macOS pilot builds and release preparation

The M7 working distribution target is a **private, locally built team pilot**. The pilot configuration uses ad-hoc signing (`-`) and never invokes a public upload or notarization. A local build can run without an Apple Developer identity. It is not a Developer ID signed/notarized download for unknown users. Audience and licence selection remain owner decisions before public distribution.

## Install and open with one command

From the repository root:

~~~sh
./scripts/install.sh
~~~

The installer builds a native release with locked dependencies, verifies its signature, copies it through a staging directory into `~/Applications/Rooster.app`, and launches it using macOS LaunchServices. It requires no sudo. It can be invoked by its full path from another directory. The build explicitly selects the Rust host target; its bundle is under `<Cargo target directory>/<host target>/release/bundle/macos/Rooster.app`. Cargo target-directory configuration is respected.

For an already built or extracted local app:

~~~sh
./scripts/install.sh --app /absolute/path/to/Rooster.app
~~~

This path needs macOS and Git, but no Rust, Node, npm, or rebuild. `--install-dir /absolute/path/to/Applications` selects another destination; `--no-open` installs without launching. `--help` lists the options. Relative input paths resolve from the caller's directory. Shell `ROOSTER_CONFIG` and `ROOSTER_DATA_DIR` overrides are forwarded to the launched app for isolated trials.

Quit any running Rooster instance before updating. The installer does not terminate it. An existing verified app is moved to a unique `Rooster-backup.*` folder beside the new app, with its path printed. A failed replacement restores that backup when the destination is absent. If launch fails, the verified installed app remains available to open manually. Installation never changes settings, drafts, recovery data, or shared guidance; ordinary app launch reads the selected configuration.

Open an installed copy later with `open "$HOME/Applications/Rooster.app"`. To return to a retained version, quit Rooster, move the current app aside, and move the backup's `Rooster.app` back into Applications. Remove old backups only when you no longer need them. A cooperative `.rooster-install.lock` prevents two installers updating the same folder; if a killed installer leaves it behind, confirm no installer is running before removing that empty lock directory. Source/destination signatures are checked, and symbolic-link destinations are rejected.

## Requirements and build

Build on macOS with Git, Xcode Command Line Tools (`xcode-select --install` if absent), Rust 1.90+, Node 22.12+, and npm. The CLI requires Git on PATH at runtime; the desktop also invokes the local Git executable. Node and Rust are build requirements, not installed-app runtime dependencies. No AI account or provider CLI is needed to use local files.

From the repository root:

~~~sh
cargo build -p rooster-cli --release --locked
cd apps/desktop
npm ci
npm run desktop:bundle
~~~

With the default Cargo target directory, outputs are `target/release/rooster` and `target/release/bundle/macos/Rooster.app`. `CARGO_TARGET_DIR` relocates both. `npm run desktop:bundle:debug` makes a development app at `target/debug/bundle/macos/Rooster.app`; `npm run desktop:build` retains the executable-only build. The pilot overlay explicitly enables `.app` packaging and sets a macOS 13 deployment floor. That metadata is not verification on every macOS version or on Intel hardware.

The embedded frontend runs without Vite or network access. Build dependencies must already be cached for `CARGO_NET_OFFLINE=true` and `npm ci --offline`. Normal clean builds need registry access to fetch locked dependencies.

## Validate and install locally

~~~sh
codesign --verify --deep --strict --verbose=2 target/release/bundle/macos/Rooster.app
codesign -dv --verbose=4 target/release/bundle/macos/Rooster.app
plutil -lint target/release/bundle/macos/Rooster.app/Contents/Info.plist
~~~

An ad-hoc signature has no Developer ID authority or notarization ticket. Keep this distinction when handing a bundle to someone else: a downloaded/quarantined copy may be blocked by Gatekeeper. Build locally for this pilot; do not disable system security or present these artifacts as publicly trusted installers. Tauri documents [ad-hoc signing and Developer ID/notarization](https://v2.tauri.app/distribute/sign/macos/).

After verification, use `./scripts/install.sh --app /absolute/path/to/Rooster.app` to install and launch it with a retained previous version. For disposable QA, launch the bundle's `Contents/MacOS/rooster-desktop` executable from a terminal with explicit `ROOSTER_CONFIG` and `ROOSTER_DATA_DIR`; Finder launches do not inherit those shell overrides; the installer forwards them explicitly.

Removing the app does not remove local settings, drafts, or recovery. Export any source you need and resolve/review recovery entries before manually deleting application data. Shared repository files remain ordinary files. See [team pilot boundaries](team-pilot.md#local-state-stays-local).

## Before any public distribution

Select the audience, project licence, final bundle identifier, supported macOS/CPU matrix, and ownership of the release. For a downloadable app, configure a Developer ID Application identity, inspect hardened runtime and required entitlements, notarize the exact archived artifact, staple the ticket, and verify it on a clean Mac with Gatekeeper. Signing credentials belong in an appropriate secret store, never the repository. Do not reuse the ad-hoc pilot overlay as a public release configuration. No public signing credentials, uploads, licence selection, or public release are part of M7's local pilot.

The [Tauri macOS bundle guide](https://v2.tauri.app/distribute/macos-application-bundle/) and [signing guide](https://v2.tauri.app/distribute/sign/macos/) explain these separate distribution steps. Other operating systems and untested CPU/OS combinations remain unclaimed until builds and filesystem journeys pass.

## CI and verified artifacts

[CI](../.github/workflows/ci.yml) runs macOS Rust stable/MSRV tests, formatting, Clippy, frontend tests/production build, the two-layout CLI pilot, ad-hoc app packaging/signature verification, and installer smoke checks (fresh install, retained update backup, damaged-bundle rejection, and symbolic-link rejection). It uses read-only repository permissions, needs no signing secrets or AI account, and publishes no release. Its local command equivalents are in the README. A hosted run is only evidence once the project is pushed and the workflow actually executes.

Build and test commands must run sequentially when sharing a Cargo target directory and changing Tauri's `custom-protocol` feature. Use a separate target for concurrent packaging to avoid feature-varied build artifacts interfering with documentation tests.

The optimized 0.1.0 app and CLI were built offline from locked dependencies on macOS 26.6.2/Apple Silicon with Rust 1.95.0; frontend dependencies were already installed from the lockfile. Both documented release and development bundle commands passed; the development bundle signature was also verified. The actual release bundle passed `codesign --verify --deep --strict`, plist validation, and native two-layout onboarding/edit/recovery journeys. Inspection confirmed an arm64 executable, ad-hoc signature, hardened-runtime flag, no TeamIdentifier or Developer ID authority, and system-library dependencies. Notarization was skipped; no public upload occurred. The app archive was extracted and its signature and executable hash checked again. CI YAML parses and local equivalents pass; no hosted CI run is claimed.

Release app executable SHA-256: `97725fb8db220cd7119ba7a3486b2c32572ec1583b159cabf0853b80c875d0fb` (12,751,824 bytes). Local-pilot ZIP SHA-256: `eeb47d8c2134267d4187d07e4854de29380a05a5f55b7bae1dde0fdf2d57de54` (4,646,146 bytes). CLI SHA-256: `60152a498bcd21e6019d3a71dfa15982f4688784358e8c029fe3202298307ac2` (4,335,136 bytes). These identify this local build, not a reproducible-build or public-trust claim.

The selected pilot audience is local developers building from source. This is a working M7 scope decision; a public audience, open-source licence, signing identity, final identifier, and clean-machine distribution validation remain unresolved owner decisions. The known CodeMirror lazy-chunk size warning remains; it did not prevent production build or native journeys.
