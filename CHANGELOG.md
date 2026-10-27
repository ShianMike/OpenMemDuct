# Changelog

## 0.1.1 — 2026-10-27

- Optional Start with Windows setting, with registration in Windows Startup Apps.
- Reuse administrator approval for memory cleanup throughout an app session.
- Opt-in automatic updates, off by default, with a Tauri updater signature and `latest.json` in the manual packaging artifact. Windows Authenticode signing remains absent; 0.1.0 users need one manual installer upgrade.
- Improved frosted-glass styling for cleanup detail popovers.

Cleanup remains limited to Windows 11 x64 build 26200. The installer remains unsigned for Windows publisher identity. Artifact checks do not establish successful automatic update installation.

## 0.1.0 — 2026-10-26

Initial Windows x64 release.

- Physical Memory, Pagefile, and System Working Set graphs with up to one hour of history.
- Live physical-memory percentage in the system tray and monitoring while the dashboard is closed.
- Eight configurable memory-cleanup targets, with results and follow-up measurements shown on the graphs.
- Optional threshold/interval cleanup, global shortcut, tray actions, and result notifications.
- A light interface with glass panels, custom window controls, keyboard support, and reduced-motion fallbacks.
- Local preferences and a short-lived elevated helper for cleanup.
- A per-user Windows installer, SHA-256 checksum, and manual packaging workflow.

Cleanup is enabled on Windows 11 x64 build 26200. The installer is unsigned; broader compatibility and controlled performance comparisons remain unverified.
