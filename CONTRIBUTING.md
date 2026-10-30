# Contributing

OpenMemDuct uses React and TypeScript for its interface, Tauri for its desktop shell, and Rust for Windows memory sampling, tray updates, settings, startup, cleanup, and automatic updates. The 0.1.2 scope is tray-menu Clean memory using saved cleanup regions and the running App version under Preferences → General → Updates.

## Prerequisites

- Windows 11 x64.
- Node.js 24.15 or newer on the 24.x line, or Node.js 26 or newer, and npm.
- Rust 1.97 or newer, using the MSVC toolchain.
- Visual Studio C++ Build Tools with Desktop development with C++ and a Windows SDK.
- Microsoft Edge WebView2 Runtime.

See [Tauri's Windows prerequisites](https://v2.tauri.app/start/prerequisites/#windows) for the native dependencies. Keep both dependency lockfiles in source control.

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

Record results against the exact 0.1.2 release commit. Local checks, hosted CI, and packaged-app runtime checks need separate evidence; the updater artifact self-check does not prove automatic update installation. See [Releasing](docs/RELEASING.md) for the release gates and evidence required before publication.

Read-only diagnostics and the no-op helper transport check are also available:

```powershell
cargo run --locked --manifest-path src-tauri/Cargo.toml --bin openmemduct -- --diagnose
cargo run --locked --manifest-path src-tauri/Cargo.toml --bin openmemduct -- --helper-self-test
```

The helper self-test sends two no-op requests through one helper and verifies that it exits on disconnect. It also runs in the Windows Checks and Package workflows.

For updater packaging changes, run the PowerShell 7 artifact self-check (also run without secrets by Checks on pull requests and by Package):

```powershell
pwsh -File scripts/prepare-updater-artifacts.ps1 -SelfCheck
```

This uses synthetic installer/signature fixtures and checks the generated manifest, checksums, and invalid inputs without signing secrets. Fixtures remain under the ignored `validation/` directory. Actual packaging requires the Tauri signing secrets described in [Releasing → Updater Artifacts](docs/RELEASING.md#updater-artifacts); updater signing does not provide a Windows Authenticode publisher identity.

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

The dashboard is fixed at 1180 × 820 logical pixels. Rust owns bounded history for up to two days and samples less frequently when the window is closed or minimized. The newest hour keeps readings at one-second resolution. Older five-minute buckets retain actual first/last readings, peaks and troughs for each series, and unavailable counters, with a maximum of 9,816 snapshots. Keep the retention contracts in `src/lib/history.ts` and `src-tauri/src/history.rs` aligned. Closing the dashboard destroys its WebView and leaves the Rust tray monitor running.

The saved `theme` preference supports `system`, `light`, and `dark`. The System choice follows Windows through WebView2; explicit themes use the document's `data-theme` and CSS color scheme. Appearance changes preview inside Preferences and become persistent on Save.

Physical Memory measures usable physical total minus available memory. Pagefile measures actual pagefile usage and allocation, separately from system commit. System Working Set measures the system file-cache working set against its observed peak. Unknown counters remain unavailable rather than becoming zero.

Cleanup is restricted to Windows 11 x64 build family 26200. Some operations use private Native API classes; any additional build family or cleanup target requires compatibility validation. Keep the fixed cleanup allowlist, authenticated helper transport, request validation, concurrency guard, and observation period intact.

The first manual cleanup requests elevation when necessary and keeps an authenticated cleanup helper for the app session. Closing the dashboard leaves that session available; quitting the app closes it. Automatic cleanup runs only in an already elevated session on a supported build. Global shortcuts pause while Preferences is open. Settings changes are validated and saved together; a failed save preserves the previous file.

Start with Windows registers the current user's startup entry and is managed through Preferences and Windows Startup Apps. Automatic updates are opt-in (`autoUpdateEnabled`, off by default), were introduced in 0.1.1, and use the native Tauri updater with signature and signed-version checks. Keep updater signing separate from Windows Authenticode publisher signing. Users of the original 0.1.0 installer must install 0.1.1 or a later release manually once before receiving subsequent automatic updates.

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
