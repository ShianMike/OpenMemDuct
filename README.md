# OpenMemDuct

OpenMemDuct is a Windows memory monitor and cleanup app built with **React, Tauri, and Rust**.

Designed for a lightweight tray workflow, it pairs clear memory graphs with a live physical-memory percentage and configurable cleanup operations.

[Download for Windows](https://github.com/ShianMike/OpenMemDuct/releases) · [Report an Issue](https://github.com/ShianMike/OpenMemDuct/issues)

![OpenMemDuct dashboard showing physical memory, pagefile, and system working set graphs](docs/images/dashboard.jpg)

## Features

- **Three memory graphs** — Physical Memory, Pagefile, and System Working Set, with up to one hour of history.
- **Live tray percentage** — See physical-memory usage while the dashboard is closed.
- **One-click cleanup** — Choose from eight cleanup targets and inspect before/after results directly on the graphs.
- **Optional automation** — Configure a usage threshold, cleanup interval, or global keyboard shortcut.
- **A focused light interface** — Glass panels, a custom title bar, keyboard controls, and reduced-motion support.
- **Local data** — Settings are saved locally; graph history stays in memory.

## Requirements

- Windows 11, x64.
- Microsoft Edge WebView2 Runtime.
- Administrator approval for memory cleanup. Monitoring works without elevation.

Cleanup is currently enabled for **Windows build 26200**. Other builds remain in monitoring mode until their cleanup compatibility is validated.

## Getting Started

Download the Windows x64 setup EXE from [GitHub Releases](https://github.com/ShianMike/OpenMemDuct/releases), run the installer, and open OpenMemDuct.

1. Open **Preferences → Memory** and choose the regions to clean.
2. Select **Clean Memory**. Results appear on each graph; select a result label for details.
3. Close the dashboard to keep the tray monitor running. Use **Open** to return or **Quit** from the tray menu to exit.

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

Version 0.1.0 is a prerelease for Windows 11 x64. Release downloads include the NSIS setup EXE and SHA256SUMS. The installer is unsigned; Windows may display an unknown-publisher warning.

Cleanup remains limited to build 26200. Clean-machine coverage, additional Windows builds, and controlled performance comparisons are ongoing.

See the [changelog](CHANGELOG.md) and [release preparation guide](docs/RELEASING.md) for the current scope and remaining checks.

## License

[MIT](LICENSE).
