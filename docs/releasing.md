# Releasing LocalBar

Releases are built and published by GitHub Actions (`.github/workflows/release.yml`) when a `v*` tag is pushed.

## Version numbers

Versions use three parts, `MAJOR.MINOR.PATCH` (for example `0.3.3`). Don't use a fourth part such as `0.3.2.1`: Cargo, Tauri and the app's own update check (ADR D18) all expect three parts.

- **PATCH** for fixes and small additions.
- **MINOR** for larger features or behaviour changes.
- **MAJOR** stays at `0` until the project declares a stable 1.0.

## Steps

1. Make sure `main` is green and holds everything that should ship.
2. On a new branch, set the same version in both places:
   - `localbar-tauri/Cargo.toml`, the `version =` line under `[package]`
   - `localbar-tauri/tauri.conf.json`, the `"version"` field
3. Run `cargo check` so `Cargo.lock` picks up the new version.
4. Open a pull request titled `Release vX.Y.Z`. Merge it once CI passes.
5. On the updated `main`, tag and push:
   ```sh
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
6. The release workflow creates a draft release, builds the packages on macOS and Linux, and then publishes it. It also copies the version from `Cargo.toml` into `tauri.conf.json` before building, so a mismatch can't reach a release. Keeping both files equal still matters for local builds.
7. Check the release page. It should have 8 files:
   - macOS: `LocalBar_X.Y.Z_universal.dmg` and `LocalBar_universal.app.tar.gz`
   - Linux: `.deb`, `.rpm` and `.AppImage`, each for x86-64 and ARM64

   You can also check from a terminal with `gh release view vX.Y.Z --json assets`.

## After releasing

- Installed copies with "Check GitHub for new versions" turned on will show the new version within a day (ADR D18).
- The macOS build is unsigned (#93). The release notes include the steps for opening it, and the README explains them in more detail.
- Add anything that needs a manual check to a dated checklist in `docs/manual-tests/`.
