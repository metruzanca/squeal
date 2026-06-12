# Rules
- Before committing make sure build compiles.
- Never commit changes unless instructed to explicitly.
- Commit using conventional commits.

# Releases

## Versioning

`0.x.y` (Semantic Versioning):
- **Minor** (`x`) — new features or changes
- **Patch** (`y`) — bug fixes
- **Major** — never change unless explicitly instructed

## Changelog

`CHANGELOG.md` follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Between releases, only use conventional commits. Do not update the changelog until preparing a release.

## Release Process

1. **Get commits since last release** to review what has changed:
   ```bash
   git log $(git describe --tags --abbrev=0)..HEAD --oneline
   ```
   Review conventional commits to ensure all important changes are captured in the changelog before releasing.

2. **Determine version bump**:
   - If **ALL** commits are `fix:` scoped → bump patch (e.g. `v0.1.0` → `v0.1.1`)
   - Otherwise → bump minor (e.g. `v0.1.0` → `v0.2.0`)
   - Major bumps are manual / user decision only

3. **Update `README.md`** to reflect any changes (usage examples, flags, features, etc.)

4. **Update `CHANGELOG.md`**:
   - Review the `[Unreleased]` section against the commits since the last release to ensure nothing important is missing
   - Move items from `[Unreleased]` into a new `## [x.y.z] - YYYY-MM-DD` section
   - Add the release date
   - Update comparison links at the bottom of the file

5. **Bump the version** in `Cargo.toml` to match the new release version

6. **Commit the changes** using conventional commits (e.g., `chore(release): bump version to 0.2.0`)

7. **Push all commits to main** before creating the release tag

8. **Create and push a Git tag** matching the version:
   ```bash
   git tag v0.2.0 && git push origin v0.2.0
   ```

9. **Let CI handle the rest** — the `.github/workflows/release.yml` workflow will:
   - Extract the latest section from `CHANGELOG.md` and use it as the GitHub release body
   - Build release binaries for Linux, macOS, and Windows
   - Create a GitHub release with the changelog body and attach the binaries

## Cargo Binstall

The project is `cargo binstall` compatible. `binstall` looks at GitHub releases for prebuilt binaries matching the crate name. The `repository` field in `Cargo.toml` points to the correct repo so binstall can locate releases. Binaries are packaged as `.tar.gz` (Unix) and `.zip` (Windows) with target-triple suffixes.

Users can install with: `cargo binstall squeal`

## Compatibility Notes

- Linux x86_64 binaries are built using `musl` (`x86_64-unknown-linux-musl`) to create fully static binaries with no glibc dependency. This ensures the binary works on any x86_64 Linux distribution regardless of the glibc version installed.
- `cargo binstall` may require specifying the target explicitly: `cargo binstall --target x86_64-unknown-linux-musl`
