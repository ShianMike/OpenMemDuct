# Release Preparation

The source version is **0.1.0** across npm, Cargo, and Tauri. The [GitHub Releases page](https://github.com/ShianMike/OpenMemDuct/releases) records published builds, their source tags, and their validation limits.

## Current Scope

- Target: Windows 11 x64.
- Cleanup compatibility: build family 26200 only; other builds remain in monitoring mode.
- Runtime: Microsoft Edge WebView2.
- License: MIT; distributed dependencies retain their own licenses.
- Installer target: NSIS setup EXE.
- Packaging uses the per-user NSIS bundle configured in `src-tauri/tauri.conf.json`.

The manual Package workflow runs the source checks, builds the Windows x64 installer, and uploads it with SHA256SUMS as a workflow artifact. It does not publish a GitHub release automatically.

## Before Packaging

- [ ] Publish the source repository and run its Windows Checks workflow on the intended release commit.
- [ ] Pass every command in [Contributing → Checks](../CONTRIBUTING.md#checks) using the committed lockfiles.
- [ ] Confirm npm, Cargo, lockfile, and Tauri versions agree.
- [ ] Confirm the README screenshot, features, supported builds, and changelog match the candidate.
- [ ] Confirm release archives exclude local environment files, development captures, dependency folders, and build output.
- [ ] Review licenses and required notices for dependencies included in the distributable.

The local workspace check results belong to the candidate being inspected. Hosted CI remains a separate gate; a workflow file alone does not prove a passing run.

## Packaged-App Checks

Build for `x86_64-pc-windows-msvc` using the version and source revision selected for release. Record the compiler, WebView2 version, Windows build, artifact sizes, and SHA-256 hashes.

```powershell
npm run tauri -- build --target x86_64-pc-windows-msvc --bundles nsis --ci
```

- [ ] Install, launch, upgrade, and uninstall on a clean supported Windows environment.
- [ ] Check WebView2 availability and installer behavior when the runtime is missing.
- [ ] Verify all three counters, graph history, actual sleep/resume gaps, and settings persistence.
- [ ] Verify the tray percentage, multiple display scales, Explorer restart, second launch, and dashboard close/reopen behavior.
- [ ] Verify manual cleanup, UAC cancellation, partial/unsupported outcomes, and the observation period. Default-off operations need their own controlled checks before being described as runtime validated.
- [ ] Verify notifications, notification sound, and app branding in the installed application.
- [ ] Verify shortcut conflicts, tray actions, and automatic cleanup in elevated and ordinary sessions.
- [ ] Measure foreground and tray CPU/memory usage in the packaged build, including owned WebView2 processes. Keep performance claims aligned with those measurements.

The development tray-lifecycle and native cleanup observations do not replace these installed-app checks. Earlier debug measurements exceeded the original foreground budgets; optimized release performance has not been established. Comparisons with Mem Reduct require matched workloads and controlled trials.

## Publish

- [ ] Choose and document the signing approach; verify signatures if signing is used.
- [ ] Move the completed changelog scope into a dated version entry.
- [ ] Tag the exact commit whose checks and packaged artifacts were validated.
- [ ] Publish the installer, checksums, source tag, supported Windows build, and known limitations together.
- [ ] Verify the public release page and download links, then update the README's release status.
