# Security Policy

## Reporting a Vulnerability

Report vulnerabilities privately through [GitHub's vulnerability reporting form](https://github.com/ShianMike/OpenMemDuct/security/advisories/new). Please avoid public issues for vulnerabilities until a fix or disclosure plan is agreed.

Include the affected version, Windows build, reproduction steps, expected and actual behavior, and potential impact. Remove credentials, personal information, and unrelated diagnostic data from attachments.

## Supported Versions

Security fixes target the latest published release. Cleanup support is currently limited to Windows 11 x64 build 26200.

## Security Boundaries

The dashboard uses local assets and a limited set of Tauri commands. Cleanup runs through a short-lived elevated helper with a fixed operation allowlist. Bugs that bypass administrator approval, cross the helper's process or IPC boundary, expose secrets, or permit unintended file or command execution are relevant security reports.

Published installers are unsigned. Verify downloads against the release's SHA256SUMS; a checksum establishes file integrity, not publisher identity. Repository protection does not replace Windows account security or code signing.

## Reviewed Dependency Alerts

On October 30, 2026, the updater dependency was raised to `tauri-plugin-updater >=2.14.1` and the lockfile updated from `zip` 4.6.1 to 9.1.3. The [upstream patch](https://github.com/tauri-apps/plugins-workspace/releases/tag/updater-v2.14.1) supports ZIP 9 and resolves:

- [GHSA-m665-v7pm-7grj](https://github.com/zip-rs/zip2/security/advisories/GHSA-m665-v7pm-7grj): inconsistent local and central directory headers, fixed in ZIP 9.0.5.
- [GHSA-q7v9-x93c-xqv3](https://github.com/zip-rs/zip2/security/advisories/GHSA-q7v9-x93c-xqv3): unbounded symlink-target allocation during archive extraction, fixed in ZIP 9.0.0.
- [GHSA-8hxf-qr8p-62x4](https://github.com/zip-rs/zip2/security/advisories/GHSA-8hxf-qr8p-62x4): AES entry authentication bypass on premature EOF, fixed in ZIP 9.0.0.

ZIP is a real Windows dependency of Tauri's optional archive updater support. The app publishes raw NSIS installers and requires a valid updater signature bound to the release version before installation; these controls do not replace updating the affected dependency. Source and Package workflows require any ZIP version in the Windows runtime, build, and test graph to be a stable release at least 9.0.5. Signature and signed-version validation remain enabled.

On October 26, 2026, [GHSA-wrw7-89jp-8q8g](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) was reviewed as **Not used** for the distributed Windows application. `glib` 0.18.5 is present in the cross-platform lockfile through Tauri's Linux GTK backend, but is absent from the Windows runtime, build, and test dependency graph. Source checks enforce that boundary.

The upstream library remains affected. The patched `glib` 0.20 release is incompatible with the current GTK 0.18 dependency chain; [Tauri tracks the upgrade limitation](https://github.com/tauri-apps/tauri/issues/12564). Reassess this alert before supporting Linux or introducing `glib` into the Windows dependency graph.
