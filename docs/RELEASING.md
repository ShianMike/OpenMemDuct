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

## Unsigned Distribution

Distribute unsigned installers for now and disclose this clearly beside the download link and in every release note. Windows may show Unknown publisher or SmartScreen warnings; do not instruct users to disable protections. If signing is introduced, verify the signature and update the disclosure.

Publish `SHA256SUMS` with each installer. A matching SHA-256 hash confirms the downloaded bytes match the published checksum; it is not a trusted publisher signature or a safety guarantee. The README includes a PowerShell verification example.

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

- [ ] State the signing status in every release note (currently unsigned).
- [ ] Move the completed changelog scope into a dated version entry.
- [ ] Tag the exact commit whose checks and packaged artifacts were validated.
- [ ] Publish the installer and `SHA256SUMS` together. In every release note, link the exact source commit and the Windows Checks and Package CI runs used for that release, and include the installer's SHA-256 checksum.
- [ ] In every release note, state that cleanup supports Windows build 26200 and other builds remain in monitoring mode. Record factual validation limits: what was checked, on which Windows build, and which installed-app checks or performance comparisons remain unverified. Do not describe source or CI checks as installed-app validation.
- [ ] Verify the public release page and download links, then update the README's release status.
