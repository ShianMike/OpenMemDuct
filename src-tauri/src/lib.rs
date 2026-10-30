mod cleanup;
mod controls;
#[cfg(windows)]
pub mod diagnostics;
mod elevation;
mod history;
mod memory;
mod models;
mod settings;
mod startup;
#[cfg(windows)]
mod tray_icon;
mod updater;

use history::push_history;
use models::*;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};

struct AppState {
    history: Mutex<VecDeque<MemorySnapshot>>,
    preferences: Mutex<Preferences>,
    settings_update: Mutex<()>,
    preferences_open: AtomicBool,
    settings_error: Mutex<Option<String>>,
    settings_path: PathBuf,
    cleanup: Mutex<cleanup::CleanupCoordinator>,
    helper_session: Mutex<elevation::Session>,
    update_running: AtomicBool,
    installing_update: AtomicBool,
    stopped: AtomicBool,
    bootstrap_count: AtomicUsize,
}

#[tauri::command]
fn get_bootstrap(state: tauri::State<'_, AppState>) -> Result<Bootstrap, String> {
    state.bootstrap_count.fetch_add(1, Ordering::Relaxed);
    let history: Vec<_> = state
        .history
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .cloned()
        .collect();
    Ok(Bootstrap {
        preferences: state.preferences.lock().map_err(|e| e.to_string())?.clone(),
        snapshot: history.last().cloned(),
        history,
        capabilities: cleanup::capabilities(),
        demo: false,
        settings_error: state
            .settings_error
            .lock()
            .map_err(|e| e.to_string())?
            .clone(),
        cleanup: state
            .cleanup
            .lock()
            .map_err(|e| e.to_string())?
            .state
            .clone(),
    })
}

#[tauri::command]
async fn get_top_processes() -> Result<ProcessList, String> {
    tauri::async_runtime::spawn_blocking(memory::top_processes)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command(async)]
fn set_preferences_open(state: tauri::State<'_, AppState>, open: bool) -> Result<(), String> {
    let _guard = state.settings_update.lock().map_err(|e| e.to_string())?;
    if open && state.installing_update.load(Ordering::SeqCst) {
        return Err("An update is being installed. OpenMemDuct will restart shortly.".into());
    }
    state.preferences_open.store(open, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
async fn save_preferences(
    app: tauri::AppHandle,
    preferences: Preferences,
) -> Result<Preferences, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _guard = state.settings_update.lock().map_err(|e| e.to_string())?;
        if state.installing_update.load(Ordering::SeqCst) {
            return Err("An update is being installed. OpenMemDuct will restart shortly.".into());
        }
        settings::validate(&preferences)?;
        let previous = state.preferences.lock().map_err(|e| e.to_string())?.clone();
        controls::change_shortcut(&app, &previous, &preferences)?;
        let startup_changed = previous.start_with_windows != preferences.start_with_windows;
        let result = (|| {
            if startup_changed {
                startup::set_enabled(preferences.start_with_windows)?;
            }
            settings::save(&state.settings_path, &preferences)
        })();
        if let Err(mut error) = result {
            if startup_changed {
                if let Err(rollback) = startup::set_enabled(previous.start_with_windows) {
                    error = format!("{error}; startup rollback failed: {rollback}");
                    report_control_error(&app, error.clone());
                }
            }
            if let Err(rollback) = controls::change_shortcut(&app, &preferences, &previous) {
                report_control_error(
                    &app,
                    format!("{error}; shortcut rollback failed: {rollback}"),
                );
                return Err(format!("{error}; shortcut rollback failed: {rollback}"));
            }
            return Err(error);
        }
        *state.preferences.lock().map_err(|e| e.to_string())? = preferences.clone();
        *state.settings_error.lock().map_err(|e| e.to_string())? = None;
        Ok(preferences)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn request_cleanup(
    app: tauri::AppHandle,
    regions: Vec<CleanupAction>,
    id: String,
) -> Result<(), String> {
    start_cleanup(app, regions, id)
}

fn start_cleanup(
    app: tauri::AppHandle,
    regions: Vec<CleanupAction>,
    id: String,
) -> Result<(), String> {
    if !cleanup::capabilities().cleanup_supported {
        return Err("Cleanup unavailable on this Windows build".into());
    }
    cleanup::validate_regions(&regions, false)?;
    {
        let state = app.state::<AppState>();
        let mut cleanup = state.cleanup.lock().map_err(|e| e.to_string())?;
        if state.installing_update.load(Ordering::SeqCst) {
            return Err("An update is being installed. OpenMemDuct will restart shortly.".into());
        }
        cleanup.begin(&id)?;
        let _ = app.emit_to("main", "cleanup:state", &cleanup.state);
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let report = match state.helper_session.lock() {
            Ok(mut session) => session.request(&id, &regions, false, |report| {
                if let Ok(mut cleanup) = state.cleanup.lock() {
                    cleanup.state.reports.insert(0, report);
                    cleanup.state.reports.truncate(20);
                    let _ = app.emit_to("main", "cleanup:state", &cleanup.state);
                };
            }),
            Err(e) => cleanup::empty_report(&id, &regions, CleanupStatus::Failed, e.to_string()),
        };
        if let Ok(mut cleanup) = state.cleanup.lock() {
            cleanup.state.reports.retain(|r| r.id != report.id);
            cleanup.finish(report.clone());
            let _ = app.emit_to("main", "cleanup:state", &cleanup.state);
        };
        controls::notify(&app, &report);
    });
    Ok(())
}

fn report_control_error(app: &tauri::AppHandle, error: String) {
    if let Ok(mut current) = app.state::<AppState>().settings_error.lock() {
        *current = Some(error.clone());
    }
    let _ = app.emit_to("main", "settings:error", &error);
    eprintln!("{error}");
}

fn trigger_action(app: &tauri::AppHandle, action: TrayAction) {
    let result = match action {
        TrayAction::None => Ok(()),
        TrayAction::ToggleWindow => {
            if let Some(window) = app
                .get_webview_window("main")
                .filter(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(true))
            {
                window.destroy().map_err(|e| e.to_string())
            } else {
                open_dashboard(app).map_err(|e| e.to_string())
            }
        }
        TrayAction::CleanMemory => {
            let regions = app
                .state::<AppState>()
                .preferences
                .lock()
                .map(|p| p.cleanup_regions.clone())
                .map_err(|e| e.to_string());
            regions.and_then(|regions| {
                windows::core::GUID::new()
                    .map_err(|e| e.to_string())
                    .and_then(|id| start_cleanup(app.clone(), regions, format!("{id:?}")))
            })
        }
    };
    if let Err(error) = result {
        report_control_error(app, error);
    }
}

fn open_dashboard(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }
    let directory = app.path().app_local_data_dir()?.join("WebView2");
    tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
        .title(if validation_mode() {
            "OpenMemDuct validation"
        } else {
            "OpenMemDuct"
        })
        .inner_size(1180.0, 820.0)
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .visible(!validation_mode())
        // Let WebView2 report Windows appearance for the System preference.
        // Explicit app themes are applied by the dashboard's color scheme.
        .theme(None)
        .background_color(tauri::webview::Color(255, 255, 255, 255))
        .data_directory(directory)
        .center()
        .on_navigation(|url| {
            (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
                || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"))
                || (cfg!(debug_assertions)
                    && url.scheme() == "http"
                    && url.host_str() == Some("127.0.0.1")
                    && url.port() == Some(1420))
        })
        .build()?;
    Ok(())
}

fn physical_usage(snapshot: &MemorySnapshot) -> Option<f64> {
    let metrics = snapshot.metrics.as_ref()?;
    let used = metrics
        .physical_total
        .checked_sub(metrics.physical_available)?;
    (metrics.physical_total > 0).then(|| 100.0 * used as f64 / metrics.physical_total as f64)
}

pub fn dispatch_cli() -> Option<Result<(), String>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--cleanup-helper") {
        return Some(elevation::parse_helper(&args[1..]).and_then(elevation::helper));
    }
    if cfg!(debug_assertions) && args == ["--helper-self-test"] {
        return Some(elevation::self_test());
    }
    if args == ["--diagnose"] {
        let reading = memory::Sampler::default().sample();
        println!("{}", serde_json::to_string_pretty(&reading).unwrap());
        return Some(if reading.metrics.is_some() {
            Ok(())
        } else {
            Err("No native reading available".into())
        });
    }
    if args.is_empty()
        || args == ["--tray"]
        || (cfg!(debug_assertions)
            && (args == ["--validate-lifecycle"]
                || (args.len() == 3 && args[0] == "--updater-self-test")))
    {
        None
    } else {
        Some(Err("Unknown command-line arguments".into()))
    }
}

fn validation_mode() -> bool {
    cfg!(debug_assertions)
        && std::env::args().any(|a| a == "--validate-lifecycle" || a == "--updater-self-test")
}

pub fn run() {
    let mut context = tauri::generate_context!();
    if validation_mode() {
        context.config_mut().identifier.push_str(".validation");
    }
    let app = tauri::Builder::default()
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                window
                    .app_handle()
                    .state::<AppState>()
                    .preferences_open
                    .store(false, Ordering::Relaxed);
            }
        })
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }
                    let enabled = app
                        .state::<AppState>()
                        .preferences
                        .lock()
                        .map(|p| {
                            controls::shortcut_matches(
                                &p,
                                shortcut,
                                app.state::<AppState>()
                                    .preferences_open
                                    .load(Ordering::Relaxed),
                            )
                        })
                        .unwrap_or(false);
                    if enabled {
                        trigger_action(app, TrayAction::CleanMemory);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Err(e) = open_dashboard(app) {
                eprintln!("{e}");
            }
        }))
        .invoke_handler(tauri::generate_handler![
            get_bootstrap,
            get_top_processes,
            save_preferences,
            request_cleanup,
            set_preferences_open
        ])
        .setup(|app| {
            let path = app.path().app_local_data_dir()?.join("preferences.json");
            let (preferences, settings_error) = match settings::load(&path) {
                Ok(p) => (p, None),
                Err(e) => (Preferences::default(), Some(e)),
            };
            let start_in_tray = preferences.start_in_tray
                || validation_mode()
                || std::env::args().any(|a| a == "--tray");
            let mut sampler = memory::Sampler::default();
            let first = sampler.sample();
            #[cfg(windows)]
            let first_usage = physical_usage(&first);
            #[cfg(windows)]
            let first_icon = tray_icon::icon(first_usage.map(|p| p.round() as u8))?;
            #[cfg(not(windows))]
            let first_icon = app.default_window_icon().ok_or("App icon missing")?.clone();
            app.manage(AppState {
                history: Mutex::new(VecDeque::from([first])),
                preferences: Mutex::new(preferences),
                settings_update: Mutex::new(()),
                preferences_open: AtomicBool::new(false),
                settings_error: Mutex::new(settings_error),
                settings_path: path,
                cleanup: Mutex::new(cleanup::CleanupCoordinator::default()),
                helper_session: Mutex::new(elevation::Session::default()),
                update_running: AtomicBool::new(false),
                installing_update: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
                bootstrap_count: AtomicUsize::new(0),
            });
            let saved = app
                .state::<AppState>()
                .preferences
                .lock()
                .map_err(|e| e.to_string())?
                .clone();
            if !cfg!(debug_assertions) {
                if let Err(error) = startup::set_enabled(saved.start_with_windows) {
                    report_control_error(
                        app.handle(),
                        format!("Could not restore Windows startup: {error}"),
                    );
                }
            }
            if let Err(error) =
                controls::change_shortcut(app.handle(), &Preferences::default(), &saved)
            {
                report_control_error(app.handle(), error);
            }
            let open = tauri::menu::MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
            let clean =
                tauri::menu::MenuItem::with_id(app, "clean", "Clean memory", true, None::<&str>)?;
            let quit = tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(app, &[&open, &clean, &quit])?;
            let tray = tauri::tray::TrayIconBuilder::with_id("memory")
                .icon(first_icon)
                .tooltip("OpenMemDuct")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        let _ = open_dashboard(app);
                    }
                    "clean" => trigger_action(app, TrayAction::CleanMemory),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        let action =
                            app.state::<AppState>()
                                .preferences
                                .lock()
                                .ok()
                                .and_then(|p| match button {
                                    tauri::tray::MouseButton::Left => Some(p.single_click_action),
                                    tauri::tray::MouseButton::Middle => Some(p.middle_click_action),
                                    _ => None,
                                });
                        if let Some(action) = action {
                            trigger_action(app, action);
                        }
                    }
                })
                .build(app)?;
            if validation_mode() {
                tray.set_visible(false)?;
            }
            if !start_in_tray {
                open_dashboard(app.handle())?;
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_tooltip = String::new();
                #[cfg(windows)]
                let mut last_percentage = first_usage.map(|p| p.round() as u8);
                let mut automation = controls::AutoCleaner::default();
                let mut updates = updater::Schedule::default();
                let automatic_allowed = controls::is_elevated()
                    && cleanup::capabilities().cleanup_supported
                    && !validation_mode();
                loop {
                    let state = handle.state::<AppState>();
                    if state.stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    let visible = handle.get_webview_window("main").is_some_and(|w| {
                        w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(true)
                    });
                    let snapshot = sampler.sample();
                    let preferences = state
                        .preferences
                        .lock()
                        .map(|p| p.clone())
                        .unwrap_or_default();
                    if let Ok(mut history) = state.history.lock() {
                        push_history(&mut history, snapshot.clone());
                    }
                    if visible {
                        let _ = handle.emit_to("main", "memory:snapshot", &snapshot);
                    }
                    let prefix = if validation_mode() {
                        "OpenMemDuct validation"
                    } else {
                        "OpenMemDuct"
                    };
                    let usage = physical_usage(&snapshot);
                    #[cfg(windows)]
                    {
                        let percentage = usage.map(|p| p.round() as u8);
                        if percentage != last_percentage {
                            if let Some(tray) = handle.tray_by_id("memory") {
                                match tray_icon::icon(percentage).and_then(|image| {
                                    tray.set_icon(Some(image)).map_err(|e| e.to_string())
                                }) {
                                    Ok(()) => last_percentage = percentage,
                                    Err(error) => {
                                        eprintln!("Could not update tray percentage: {error}")
                                    }
                                }
                            }
                        }
                    }
                    let tooltip = if let (Some(m), Some(usage)) = (&snapshot.metrics, usage) {
                        format!(
                            "{prefix} · {}% · {:.1} GiB available · {}",
                            usage.round() as u8,
                            m.physical_available as f64 / 1073741824.0,
                            if usage >= preferences.danger_percent as f64 {
                                "Danger"
                            } else if usage >= preferences.warning_percent as f64 {
                                "Warning"
                            } else {
                                "Normal"
                            }
                        )
                    } else {
                        format!("{prefix} · memory readings unavailable")
                    };
                    if tooltip != last_tooltip {
                        if let Some(tray) = handle.tray_by_id("memory") {
                            let _ = tray.set_tooltip(Some(&tooltip));
                        }
                        last_tooltip = tooltip;
                    }
                    let fast = if let Ok(mut cleanup) = state.cleanup.lock() {
                        if cleanup.observe(&snapshot) {
                            let _ = handle.emit_to("main", "cleanup:state", &cleanup.state);
                        }
                        cleanup.fast_observation()
                    } else {
                        false
                    };
                    let available = state
                        .cleanup
                        .lock()
                        .map(|c| !c.state.busy && c.state.cooldown_until_ms <= now_ms())
                        .unwrap_or(false);
                    if automation.due(
                        &preferences,
                        usage,
                        snapshot.elapsed_ms,
                        automatic_allowed && available,
                    ) {
                        trigger_action(&handle, TrayAction::CleanMemory);
                    }
                    if !cfg!(debug_assertions)
                        && updates.due(preferences.auto_update_enabled, std::time::Instant::now())
                    {
                        updater::start(handle.clone());
                    }
                    std::thread::sleep(Duration::from_secs(if visible || fast { 1 } else { 5 }));
                }
            });
            if validation_mode() {
                let handle = app.handle().clone();
                #[cfg(debug_assertions)]
                if std::env::args().any(|a| a == "--updater-self-test") {
                    std::thread::spawn(move || {
                        let args: Vec<_> = std::env::args().collect();
                        if let Err(error) = updater::self_test(&handle, &args[2], &args[3]) {
                            eprintln!("Updater validation failed: {error}");
                            handle.exit(1);
                        } else {
                            handle.exit(0);
                        }
                    });
                    return Ok(());
                }
                std::thread::spawn(move || {
                    if let Err(error) = diagnostics::validate_lifecycle(&handle) {
                        eprintln!("Native validation failed: {error}");
                        handle.exit(1);
                    } else {
                        handle.exit(0);
                    }
                });
            }
            Ok(())
        })
        .build(context)
        .expect("OpenMemDuct could not start");
    app.run(|app, event| match event {
        tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        tauri::RunEvent::Exit => app
            .state::<AppState>()
            .stopped
            .store(true, Ordering::Relaxed),
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_percentage_uses_used_ram_and_handles_unavailable_readings() {
        let mut sample: MemorySnapshot =
            serde_json::from_str(include_str!("../../tests/fixtures/snapshot.json")).unwrap();
        for (available, expected, rounded) in [(1000, 0.0, 0), (335, 66.5, 67), (0, 100.0, 100)] {
            let metrics = sample.metrics.as_mut().unwrap();
            metrics.physical_total = 1000;
            metrics.physical_available = available;
            assert_eq!(physical_usage(&sample), Some(expected));
            assert_eq!(
                physical_usage(&sample).map(|p| p.round() as u8),
                Some(rounded)
            );
        }
        sample.metrics.as_mut().unwrap().physical_available = 1001;
        assert_eq!(physical_usage(&sample), None);
        sample.metrics.as_mut().unwrap().physical_total = 0;
        sample.metrics.as_mut().unwrap().physical_available = 0;
        assert_eq!(physical_usage(&sample), None);
        sample.metrics = None;
        assert_eq!(physical_usage(&sample), None);
    }
}
