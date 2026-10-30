# Changelog

## 0.1.3 — 2026-10-30 (Asia/Manila)

Windows Authenticode publisher signing remains absent. Tauri updater signatures authenticate in-app updates separately and do not establish a verified Windows publisher identity.

- Add a custom chart-duration dropdown with eight ranges: 30 minutes, 1 hour, 2 hours, 4 hours, 8 hours, 16 hours, 1 day, and 2 days.
- Retain up to 48 hours of in-memory history for the current app session, bounded to 9,816 snapshots. The recent hour uses one-second buckets; older five-minute buckets preserve first and last readings, per-series minima and maxima, and unavailable readings.
- Include dates in chart labels, inspected readings, and cleanup timestamps for one-day and two-day ranges.
- Add a warm graphite and moss dark palette with copper accents across the dashboard, preferences, dropdowns, and cleanup details.
- Add Light, Dark, and System choices under Preferences → General → Appearance, with immediate preview, Save to persist across launches, and Cancel to restore the saved appearance.
- Keep older preferences compatible by defaulting a missing appearance setting to System. System follows Windows appearance changes while the app runs.

Cleanup remains limited to Windows 11 x64 build 26200; other builds remain in monitoring mode. Session history is not saved to disk and clears when the app quits.

## 0.1.2 — 2026-10-28

- Clean memory from the tray icon's right-click menu using saved cleanup regions.
- Show the running app version in Preferences under General → Updates.

Cleanup remains limited to Windows 11 x64 build 26200; other builds remain in monitoring mode. Windows Authenticode signing remains absent.

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
