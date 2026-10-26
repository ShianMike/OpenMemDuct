# Security Policy

## Reporting a Vulnerability

Report vulnerabilities privately through [GitHub's vulnerability reporting form](https://github.com/ShianMike/OpenMemDuct/security/advisories/new). Please avoid public issues for vulnerabilities until a fix or disclosure plan is agreed.

Include the affected version, Windows build, reproduction steps, expected and actual behavior, and potential impact. Remove credentials, personal information, and unrelated diagnostic data from attachments.

## Supported Versions

Security fixes target the latest published release. Cleanup support is currently limited to Windows 11 x64 build 26200.

## Security Boundaries

The dashboard uses local assets and a limited set of Tauri commands. Cleanup runs through a short-lived elevated helper with a fixed operation allowlist. Bugs that bypass administrator approval, cross the helper's process or IPC boundary, expose secrets, or permit unintended file or command execution are relevant security reports.

Published installers are unsigned. Verify downloads against the release's SHA256SUMS; a checksum establishes file integrity, not publisher identity. Repository protection does not replace Windows account security or code signing.

## Reviewed Dependency Alert

On October 26, 2026, [GHSA-wrw7-89jp-8q8g](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) was reviewed as **Not used** for the distributed Windows application. `glib` 0.18.5 is present in the cross-platform lockfile through Tauri's Linux GTK backend, but is absent from the Windows runtime, build, and test dependency graph. Source checks enforce that boundary.

The upstream library remains affected. The patched `glib` 0.20 release is incompatible with the current GTK 0.18 dependency chain; [Tauri tracks the upgrade limitation](https://github.com/tauri-apps/tauri/issues/12564). Reassess this alert before supporting Linux or introducing `glib` into the Windows dependency graph.
