use crate::AppState;
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tauri::Manager;
use tauri_plugin_updater::UpdaterExt;

const CHECK_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Default)]
pub struct Schedule {
    next_check: Option<Instant>,
}

impl Schedule {
    pub fn due(&mut self, enabled: bool, now: Instant) -> bool {
        if !enabled {
            self.next_check = None;
            return false;
        }
        if self.next_check.is_some_and(|next| now < next) {
            return false;
        }
        self.next_check = Some(now + CHECK_INTERVAL);
        true
    }
}

pub fn start(app: tauri::AppHandle) {
    if app
        .state::<AppState>()
        .update_running
        .swap(true, Ordering::SeqCst)
    {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(error) = check_and_install(&app).await {
            crate::report_control_error(
                &app,
                format!("Automatic update failed. Retrying in 6 hours. {error}"),
            );
        }
        app.state::<AppState>()
            .update_running
            .store(false, Ordering::SeqCst);
    });
}

async fn check_and_install(app: &tauri::AppHandle) -> Result<(), String> {
    let builder = app.updater_builder().timeout(Duration::from_secs(30));
    // Keep the UI intact if Windows refuses to launch the installer.
    #[cfg(windows)]
    let builder = builder.on_before_exit(|| {});
    let updater = builder.build().map_err(|e| e.to_string())?;
    let mut update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return Ok(()),
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => {
            eprintln!("No automatic-update manifest is available from GitHub releases.");
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
    };
    if !release_url_allowed(update.download_url.as_str()) {
        return Err("The update must come from the OpenMemDuct GitHub releases.".into());
    }
    update.timeout = Some(Duration::from_secs(300));
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        loop {
            let _settings = state.settings_update.lock().map_err(|e| e.to_string())?;
            let enabled = state
                .preferences
                .lock()
                .map_err(|e| e.to_string())?
                .auto_update_enabled;
            if !enabled || state.stopped.load(Ordering::Relaxed) {
                return Ok(());
            }
            let cleanup = state.cleanup.lock().map_err(|e| e.to_string())?;
            if can_install(
                state.preferences_open.load(Ordering::Relaxed),
                cleanup.state.busy,
                cleanup.state.cooldown_until_ms,
                crate::now_ms(),
            ) {
                // Share the cleanup lock so no new cleanup starts during installation.
                state.installing_update.store(true, Ordering::SeqCst);
                break;
            }
            drop(cleanup);
            drop(_settings);
            std::thread::sleep(Duration::from_secs(1));
        }
        let result = (|| {
            let mut session = state.helper_session.lock().map_err(|e| e.to_string())?;
            // Release the elevated executable before the installer replaces it.
            drop(std::mem::take(&mut *session));
            drop(session);
            update.install(bytes).map_err(|e| e.to_string())
        })();
        state.installing_update.store(false, Ordering::SeqCst);
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

fn release_url_allowed(url: &str) -> bool {
    url.starts_with("https://github.com/ShianMike/OpenMemDuct/releases/download/")
}

fn can_install(preferences_open: bool, busy: bool, cooldown_until: u64, now: u64) -> bool {
    !preferences_open && !busy && cooldown_until <= now
}

#[cfg(debug_assertions)]
pub fn self_test(app: &tauri::AppHandle, manifest: &str, installer: &str) -> Result<(), String> {
    let updater = app
        .updater_builder()
        .timeout(Duration::from_secs(30))
        .version_comparator(|_, _| true)
        .build()
        .map_err(|e| e.to_string())?;
    let manifest = std::fs::read_to_string(manifest).map_err(|e| e.to_string())?;
    let mut update = updater
        .restore_update(serde_json::from_str(&manifest).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?
        .ok_or("No updater manifest")?;
    if !release_url_allowed(update.download_url.as_str()) {
        return Err("Unexpected release download URL".into());
    }
    let mut bytes = std::fs::read(installer).map_err(|e| e.to_string())?;
    update.verify(&bytes).map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("Empty release installer".into());
    }
    bytes[0] ^= 1;
    if update.verify(&bytes).is_ok() {
        return Err("Tampered installer was accepted".into());
    }
    bytes[0] ^= 1;
    let release_version = update.version.clone();
    update.version = format!("{release_version}-mismatched");
    if update.verify(&bytes).is_ok() {
        return Err("Mismatched signed version was accepted".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "releaseVersion": release_version,
            "verifiedBytes": bytes.len(),
            "rejectsTamperedInstaller": true,
            "rejectsMismatchedVersion": true,
            "installed": false,
        })
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn updates_are_opt_in_immediate_on_enable_and_checked_every_six_hours() {
        let mut schedule = Schedule::default();
        let now = Instant::now();
        assert!(!schedule.due(false, now));
        assert!(schedule.due(true, now));
        assert!(!schedule.due(true, now + CHECK_INTERVAL - Duration::from_secs(1)));
        assert!(schedule.due(true, now + CHECK_INTERVAL));
        assert!(!schedule.due(false, now + CHECK_INTERVAL));
        assert!(schedule.due(true, now + CHECK_INTERVAL));
    }

    #[test]
    fn installation_waits_for_preferences_cleanup_and_observation() {
        assert!(!can_install(true, false, 100, 100));
        assert!(!can_install(false, true, 100, 100));
        assert!(!can_install(false, false, 101, 100));
        assert!(can_install(false, false, 100, 100));
    }

    #[test]
    fn downloads_are_limited_to_this_repository_over_https() {
        assert!(release_url_allowed("https://github.com/ShianMike/OpenMemDuct/releases/download/v0.2.0/OpenMemDuct_0.2.0_x64-setup.exe"));
        for url in [
            "http://github.com/ShianMike/OpenMemDuct/releases/download/v0.2.0/app.exe",
            "https://github.com.evil.example/ShianMike/OpenMemDuct/releases/download/app.exe",
            "https://github.com/other/OpenMemDuct/releases/download/app.exe",
            "https://github.com/ShianMike/OpenMemDuct/releases/download-evil/app.exe",
        ] {
            assert!(!release_url_allowed(url));
        }
    }

    #[test]
    fn production_updates_require_tls_and_version_bound_signatures() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let updater = &config["plugins"]["updater"];
        assert_eq!(updater["requireSignedVersion"], true);
        assert!(updater["pubkey"]
            .as_str()
            .is_some_and(|key| !key.is_empty()));
        assert_eq!(
            updater["endpoints"][0],
            "https://github.com/ShianMike/OpenMemDuct/releases/latest/download/latest.json"
        );
        assert_eq!(updater["windows"]["installMode"], "passive");
        for key in [
            "dangerousInsecureTransportProtocol",
            "dangerousAcceptInvalidCerts",
            "dangerousAcceptInvalidHostnames",
            "allowDowngrades",
        ] {
            assert_ne!(updater[key], true);
        }
    }
}
