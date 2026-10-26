# Contributing

OpenMemDuct uses React and TypeScript for its interface, Tauri for its desktop shell, and Rust for Windows memory sampling, tray updates, settings, and cleanup.

## Prerequisites

- Windows 11 x64.
- Node.js 24 or newer and npm.
- Rust 1.97 or newer, using the MSVC toolchain.
- Visual Studio C++ Build Tools with Desktop development with C++ and a Windows SDK.
- Microsoft Edge WebView2 Runtime.

See [Tauri's Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows) for the native dependencies. The current source has been checked with Node.js 24.12.0 and Rust 1.97.0. Keep both dependency lockfiles in source control.

## Run the App

From the project root:

```powershell
npm ci
npm run tauri -- dev
```

Tauri starts Vite on port 1420. If Vite is already running, keep it open and start the native app separately:

```powershell
cargo run --locked --manifest-path src-tauri/Cargo.toml --bin openmemduct
```

For interface-only work:

```powershell
npm run dev
```

Open <http://127.0.0.1:1420/>. This preview is labeled Demo and uses simulated memory data and cleanup outcomes. Scenario controls are available in Preferences. The production desktop app uses native counters; its browser demo adapter is gated out of production builds.

## Checks

Run these from the project root:

```powershell
npm run typecheck
npm run lint
npm run test -- --run
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

The frontend build produces web assets. It does not package an application or installer. Native unit tests use read-only counters and no-op cleanup paths; they do not clear the workstation's memory. The Windows Checks workflow runs the same source and test lanes.

Read-only diagnostics and the no-op helper transport check are also available:

```powershell
cargo run --locked --manifest-path src-tauri/Cargo.toml --bin openmemduct -- --diagnose
cargo run --locked --manifest-path src-tauri/Cargo.toml --bin openmemduct -- --helper-self-test
```

## Project Structure

| Path              | Purpose                                                           |
| ----------------- | ----------------------------------------------------------------- |
| `src/components/` | Charts, Preferences, and window controls                          |
| `src/lib/`        | Frontend contracts, formatting, telemetry, and chart helpers      |
| `src/dev/`        | Development-only browser fixtures                                 |
| `src-tauri/src/`  | Native sampling, tray rendering, settings, cleanup, and elevation |
| `tests/fixtures/` | Shared Rust/TypeScript contract fixtures                          |
| `public/icon.svg` | App-icon source                                                   |
| `docs/`           | Public screenshots and release preparation                        |

## Implementation Notes

The dashboard is fixed at 1180 × 820 logical pixels. Rust owns bounded history, keeps up to one hour of readings, and samples less frequently when the window is closed or minimized. Closing the dashboard destroys its WebView and leaves the Rust tray monitor running.

Physical Memory measures usable physical total minus available memory. Pagefile measures actual pagefile usage and allocation, separately from system commit. System Working Set measures the system file-cache working set against its observed peak. Unknown counters remain unavailable rather than becoming zero.

Cleanup is restricted to Windows 11 x64 build family 26200. Some operations use private Native API classes; any additional build family or cleanup target requires compatibility validation. Keep the fixed cleanup allowlist, authenticated helper transport, request validation, concurrency guard, and observation period intact.

The helper requests elevation only when necessary. Automatic cleanup runs only in an already elevated session on a supported build. Global shortcuts pause while Preferences is open. Settings changes are validated and saved together; a failed save preserves the previous file.

Keep interface effects brief and finite, and preserve reduced-motion and reduced-transparency fallbacks. Chart gaps, cleanup results, and recorded history should reflect actual readings rather than fabricated interpolation or assumed savings.

## Pull Requests and Security

Use a branch and pull request for changes to `main`. The protected branch requires the GitHub Actions **Source checks** job to pass against an up-to-date base, with review conversations resolved. Version tags cannot be moved or deleted. The manual installer workflow runs only from `main`.

Workflow actions are pinned to reviewed commit SHAs. Keep workflow permissions minimal and do not add credentials to the source tree. Dependabot proposes dependency and action updates for review; it does not merge them automatically.

See the [security policy](.github/SECURITY.md) for private vulnerability reporting.

Use `public/icon.svg` when updating app icons:

```powershell
npm run tauri -- icon public/icon.svg --output src-tauri/icons
```

Raw local validation captures and investigation notes are ignored by Git. Keep screenshots intended for publication in `docs/images/`, and label any simulated data clearly.

See [Releasing](docs/RELEASING.md) before preparing a distributable build.
