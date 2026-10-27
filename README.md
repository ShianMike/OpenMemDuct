# OpenMemDuct

OpenMemDuct is a Windows memory monitor and cleanup app built with **React, Tauri, and Rust**.

Designed for a lightweight tray workflow, it pairs clear memory graphs with a live physical-memory percentage and configurable cleanup operations.

[Download 0.1.1 for Windows](https://github.com/ShianMike/OpenMemDuct/releases/download/v0.1.1/OpenMemDuct_0.1.1_x64-setup.exe) · [Report an Issue](https://github.com/ShianMike/OpenMemDuct/issues)

**Unsigned release:** The Windows installer is currently unsigned. Windows may show **Unknown publisher** or a **SmartScreen** warning. Review the release details and checksum below before deciding whether to run it; keep Windows protections enabled.

![OpenMemDuct dashboard showing physical memory, pagefile, and system working set graphs](docs/images/dashboard.jpg)

## Features

- **Three memory graphs** — Physical Memory, Pagefile, and System Working Set, with up to one hour of history.
- **Live tray percentage** — See physical-memory usage while the dashboard is closed.
- **Start with Windows** — Launch automatically at sign-in and manage the entry in Windows Startup Apps.
- **One cleanup approval per session** — Approve the first cleanup, then clean again without repeated administrator prompts while the tray app keeps running.
- **Optional automatic updates** — Opt in to updates authenticated by the app's updater signature. Off by default.
- **One-click cleanup** — Choose from eight cleanup targets and inspect before/after results directly on the graphs.
- **Optional automation** — Configure a usage threshold, cleanup interval, or global keyboard shortcut.
- **A focused light interface** — Glass panels, improved frost on cleanup detail popovers, a custom title bar, keyboard controls, and reduced-motion support.
- **Local data** — Settings are saved locally; graph history stays in memory.

## Requirements

- Windows 11, x64.
- Microsoft Edge WebView2 Runtime.
- Administrator approval for memory cleanup. Monitoring works without elevation.

Cleanup is currently enabled for **Windows build 26200**. Other builds remain in monitoring mode until their cleanup compatibility is validated.

## Getting Started

Download the [0.1.1 Windows x64 setup EXE](https://github.com/ShianMike/OpenMemDuct/releases/download/v0.1.1/OpenMemDuct_0.1.1_x64-setup.exe), run the installer, and open OpenMemDuct.

1. Open **Preferences → Memory** and choose the regions to clean.
2. Select **Clean Memory**. Results appear on each graph; select a result label for details.
3. Close the dashboard to keep the tray monitor running. Use **Open** to return or **Quit** from the tray menu to exit.

To launch at sign-in, enable **Preferences → General → Start with Windows**. Enable **Start in Tray** as well to keep the dashboard closed when the app starts.

To opt in to automatic updates, enable **Preferences → General → Install updates automatically**. OpenMemDuct checks GitHub Releases at startup and every six hours while enabled. Closing the dashboard keeps the cleanup approval session active; quitting OpenMemDuct ends it.

Automatic cleanup and the global shortcut are off by default. Automatic cleanup runs only in an existing administrator session on a supported Windows build.

Cleanup results report observed memory changes. Trimming caches can increase subsequent disk activity, and performance depends on the workload.

## Development

Install the [Windows development prerequisites](CONTRIBUTING.md#prerequisites), then run:

```powershell
npm ci
npm run tauri -- dev
```

For UI work with simulated data, use `npm run dev` and open <http://127.0.0.1:1420/>.

See [Contributing](CONTRIBUTING.md) for checks and project structure.

## Release Status

Version **0.1.1** targets Windows 11 x64. See the [v0.1.1 release notes](https://github.com/ShianMike/OpenMemDuct/releases/tag/v0.1.1) for published assets, the exact source commit, Windows Checks and Package CI results, installer checksum, and runtime validation coverage.

The release artifact set is `OpenMemDuct_0.1.1_x64-setup.exe`, `OpenMemDuct_0.1.1_x64-setup.exe.sig`, `latest.json`, and `SHA256SUMS`. The checksum file covers the installer, signature, and manifest.

Download the installer and [SHA256SUMS](https://github.com/ShianMike/OpenMemDuct/releases/download/v0.1.1/SHA256SUMS) from the same release into one folder, open PowerShell there, and run:

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath .\OpenMemDuct_0.1.1_x64-setup.exe
Get-Content -LiteralPath .\SHA256SUMS
```

Compare the reported hash with the entry for `OpenMemDuct_0.1.1_x64-setup.exe` in `SHA256SUMS` (letter case does not matter). If they differ, do not run the installer. A match confirms the downloaded bytes match the published checksum; it is not a trusted publisher signature or a safety guarantee.

The original 0.1.0 installer has no in-app updater, and its immutable release has no updater signature or manifest. Install 0.1.1 manually once to gain optional automatic updates for subsequent releases. Live updates require the installer, matching `.exe.sig`, and `latest.json` to be published together; the 0.1.0 release remains unchanged. The Tauri updater signature authenticates updates to the app; it is separate from paid Windows Authenticode signing, so the installer and app binaries still have no verified Windows publisher identity.

Cleanup remains limited to build 26200. Clean-machine coverage, additional Windows builds, and controlled performance comparisons are ongoing.

See the [changelog](CHANGELOG.md) and [release preparation guide](docs/RELEASING.md) for the current scope and remaining checks.

## License

[MIT](LICENSE).

See the [security policy](.github/SECURITY.md) to report a vulnerability privately.
