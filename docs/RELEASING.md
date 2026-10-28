# Release Preparation

This guide targets release **0.1.2**, dated **2026-10-28** (Asia/Manila). Confirm npm, Cargo, their lockfiles, and Tauri all use 0.1.2 before packaging. The [GitHub Releases page](https://github.com/ShianMike/OpenMemDuct/releases) records published builds, their exact source commits and tags, and their validation limits.

## Current Scope

- Target: Windows 11 x64.
- Cleanup compatibility: build family 26200 only; other builds remain in monitoring mode.
- Runtime: Microsoft Edge WebView2.
- License: MIT; distributed dependencies retain their own licenses.
- Installer target: NSIS setup EXE.
- Packaging uses the per-user NSIS bundle configured in `src-tauri/tauri.conf.json`.
- Version 0.1.2 adds only Clean memory in the tray icon's right-click menu using saved cleanup regions and the running App version under Preferences → General → Updates.

The manual, main-only Package workflow runs the source checks, builds the Windows x64 installer and its updater signature, and uploads exactly four files as one workflow artifact:

- `OpenMemDuct_0.1.2_x64-setup.exe`
- `OpenMemDuct_0.1.2_x64-setup.exe.sig`
- `latest.json`
- `SHA256SUMS` (hashes for the installer, signature, and manifest)

It does not publish a GitHub release, create a tag, or change the version.

## Unsigned Distribution

Distribute installers without Windows Authenticode signing for now and disclose this clearly beside the download link and in every release note. Windows may show Unknown publisher or SmartScreen warnings; do not instruct users to disable protections. If Authenticode signing is introduced, verify the signature and update the disclosure.

[Tauri updater signatures](https://v2.tauri.app/plugin/updater/#signing-updates) are required to authenticate updates against the public key embedded in the app. They are separate from paid [Windows Authenticode code signing](https://v2.tauri.app/distribute/sign/windows/): the `.exe.sig` does not establish a Windows publisher identity, and the installer and app binaries remain unsigned for that purpose.

Publish `SHA256SUMS` with each installer. A matching SHA-256 hash confirms the downloaded bytes match the published checksum; it is not a trusted publisher signature or a safety guarantee. The README includes a PowerShell verification example.

## Updater Artifacts

Configure the repository Actions secret `TAURI_SIGNING_PRIVATE_KEY` before packaging and, for an encrypted key, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The key must match the public key embedded in `src-tauri/tauri.conf.json`; the optional password secret can be absent for an unencrypted key. Package checks only the key's presence before building and fails clearly when it is missing. Only the installer build receives the key and password values. Keep them out of source control, logs, and release artifacts, and retain a secure backup of the key for future updates.

The native configuration must enable `bundle.createUpdaterArtifacts: true` and `plugins.updater.requireSignedVersion: true`. Tauri generates the `.exe.sig` during the existing x64 NSIS build and binds the app version in its trusted comment. `scripts/prepare-updater-artifacts.ps1` checks that comment's version against the manifest version, creates the [static updater manifest](https://v2.tauri.app/plugin/updater/#static-json-file), and hashes all three files. For manual signing, [pass `--app-version` to `tauri signer sign`](https://v2.tauri.app/reference/cli/#signer-sign); an unversioned or mismatched signature is rejected. The helper's focused check runs without secrets in both Checks (including pull requests) and Package and can be run locally with PowerShell 7:

```powershell
pwsh -File scripts/prepare-updater-artifacts.ps1 -SelfCheck
```

The self-check uses synthetic files without signing secrets and retains its fixtures under the ignored `validation/` directory. It checks manifest fields and checksums and rejects missing, empty, malformed, unversioned, or version-mismatched signature inputs, invalid versions, empty installers, and extra installers. Reading the trusted comment here is a packaging consistency check; the native updater verifies the cryptographic signature before trusting the comment. The self-check does not verify a real updater signature or run an installer.

The app's updater endpoint is [the latest release's `latest.json`](https://github.com/ShianMike/OpenMemDuct/releases/latest/download/latest.json). For this release, the generated manifest must contain version `0.1.2`, a UTC RFC 3339 `pub_date`, and one `windows-x86_64` platform. Its `signature` is the `.exe.sig` file's contents, and its installer URL is `https://github.com/ShianMike/OpenMemDuct/releases/download/v0.1.2/OpenMemDuct_0.1.2_x64-setup.exe`. Notes are optional and omitted. Keep the per-user (`currentUser`) NSIS installation mode.

The original public version 0.1.0 installer has no updater, and its immutable release has no `.exe.sig` or `latest.json`. Leave its installer, checksum asset, tag, and version unchanged. Version 0.1.1 introduced the opt-in updater (`autoUpdateEnabled`, off by default); users of the original 0.1.0 installer must manually install 0.1.1 or a later release once to gain it. Publish the 0.1.2 installer, matching signature, manifest, and checksums from the same Package run together so updater-capable installations can receive it through the endpoint. Offline signing or artifact checks do not prove successful automatic update installation.

Record native behavior and packaged-app evidence against the exact release commit. Report source/CI results and actual automatic update installation coverage separately; do not infer installed-app behavior from this guide or from the artifact self-check.

## Before Packaging

- [ ] Publish the source repository and run its Windows Checks workflow on the intended release commit.
- [ ] Pass every command in [Contributing → Checks](../CONTRIBUTING.md#checks) using the committed lockfiles.
- [ ] Confirm npm, Cargo, lockfile, and Tauri versions agree.
- [ ] Finalize release notes with the exact source commit, Windows Checks and Package CI links and results, installer SHA-256, and factual runtime coverage. Resolve every placeholder before publication; keep local validation paths out of the public release notes.
- [ ] Confirm the updater signing secrets are configured, the embedded public key matches, and updater artifacts are enabled. Never print or commit the private key or password.
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
- [ ] Verify tray-menu Clean memory uses the saved cleanup regions with the dashboard closed and after reopening it.
- [ ] Verify Preferences → General → Updates shows App version `0.1.2` after installation and upgrade.
- [ ] Verify manual cleanup, UAC cancellation, partial/unsupported outcomes, and the observation period. Default-off operations need their own controlled checks before being described as runtime validated.
- [ ] Verify notifications, notification sound, and app branding in the installed application.
- [ ] Verify Start with Windows and Start in Tray, cleanup approval reuse after closing/reopening the dashboard, and the frosted cleanup detail popovers, including reduced-transparency mode.
- [ ] Verify updates remain off by default. On an updater-capable installed version, opt in and test a newer signed release through `latest.json`, including rejection of a mismatched installer signature. Record this separately from the artifact self-check.
- [ ] Verify shortcut conflicts, tray actions, and automatic cleanup in elevated and ordinary sessions.
- [ ] Measure foreground and tray CPU/memory usage in the packaged build, including owned WebView2 processes. Keep performance claims aligned with those measurements.

The development tray-lifecycle and native cleanup observations do not replace these installed-app checks. Earlier debug measurements exceeded the original foreground budgets; optimized release performance has not been established. Comparisons with Mem Reduct require matched workloads and controlled trials.

## Publish

- [ ] State the signing status in every release note: Tauri updater signature present; Windows Authenticode publisher signing absent.
- [ ] Confirm the 0.1.2 changelog entry is dated 2026-10-28 (Asia/Manila) and covers only tray-menu cleanup and the running App version display.
- [ ] Tag the exact commit whose checks and packaged artifacts were validated.
- [ ] Use tag `v0.1.2` and publish all four files from the same validated Package artifact together, preserving their filenames. Do not mix an installer with a signature or manifest from another build. In every release note, link the exact source commit and the Windows Checks and Package CI runs used for that release, and include the installer's SHA-256 checksum.
- [ ] In every release note, state that cleanup supports Windows build 26200 and other builds remain in monitoring mode. Record factual validation limits: what was checked, on which Windows build, and which installed-app checks or performance comparisons remain unverified. Do not describe source or CI checks as installed-app validation.
- [ ] Verify the public release page and all four download links. Confirm `/releases/latest/download/latest.json` resolves to the intended stable release and its platform URL downloads the exact matching installer. Retain the one-time manual upgrade explanation for 0.1.0 users.
