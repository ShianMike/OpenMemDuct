use crate::models::*;
use std::str::FromStr;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut};
use tauri_plugin_notification::NotificationExt;

pub fn shortcut_matches(p: &Preferences, shortcut: &Shortcut, editing: bool) -> bool {
    !editing && p.hotkey_enabled && parse_shortcut(&p.hotkey).is_ok_and(|key| key == *shortcut)
}

pub fn parse_shortcut(value: &str) -> Result<Shortcut, String> {
    if value.len() > 64 {
        return Err("Shortcut is too long".into());
    }
    let parts: Vec<_> = value.split('+').collect();
    let key = parts.last().copied().unwrap_or_default();
    let modifiers = &parts[..parts.len().saturating_sub(1)];
    let allowed_key = (key.len() == 1
        && key
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
        || key
            .strip_prefix('F')
            .and_then(|v| v.parse::<u8>().ok())
            .is_some_and(|v| (1..=24).contains(&v) && key == format!("F{v}"));
    if !allowed_key
        || modifiers.is_empty()
        || modifiers
            .iter()
            .any(|m| !["Ctrl", "Alt", "Shift", "Super"].contains(m))
        || modifiers
            .iter()
            .enumerate()
            .any(|(i, m)| modifiers[..i].contains(m))
    {
        return Err("Use Ctrl, Alt, or Windows with a letter, number, or function key".into());
    }
    let shortcut = Shortcut::from_str(value).map_err(|e| format!("Invalid shortcut: {e}"))?;
    if !shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        return Err("A global shortcut needs Ctrl, Alt, or Windows".into());
    }
    Ok(shortcut)
}

pub fn change_shortcut(
    app: &tauri::AppHandle,
    previous: &Preferences,
    next: &Preferences,
) -> Result<(), String> {
    let old = previous
        .hotkey_enabled
        .then(|| parse_shortcut(&previous.hotkey))
        .transpose()?;
    let new = next
        .hotkey_enabled
        .then(|| parse_shortcut(&next.hotkey))
        .transpose()?;
    let manager = app.global_shortcut();
    if old == new && new.is_none_or(|key| manager.is_registered(key)) {
        return Ok(());
    }
    let mut added = false;
    if let Some(key) = new {
        if !manager.is_registered(key) {
            manager
                .register(key)
                .map_err(|e| format!("Shortcut {} is unavailable: {e}", next.hotkey))?;
            added = true;
        }
    }
    if let Some(key) = old.filter(|key| Some(*key) != new && manager.is_registered(*key)) {
        if let Err(error) = manager.unregister(key) {
            let rollback = if added {
                manager.unregister(new.unwrap()).err()
            } else {
                None
            };
            return Err(format!(
                "Could not release the previous shortcut: {error}; rollback: {rollback:?}"
            ));
        }
    }
    Ok(())
}

pub fn is_elevated() -> bool {
    #[cfg(windows)]
    unsafe {
        use crate::memory::Handle;
        use windows::Win32::{
            Foundation::HANDLE,
            Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY},
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        };
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let token = Handle(token);
        let mut info = TOKEN_ELEVATION::default();
        let mut returned = 0;
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some(&mut info as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok()
            && returned as usize == std::mem::size_of::<TOKEN_ELEVATION>()
            && info.TokenIsElevated != 0
    }
    #[cfg(not(windows))]
    false
}

pub fn notify(app: &tauri::AppHandle, report: &CleanupReport) {
    if app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(true)
    }) {
        return;
    }
    let preferences = app
        .state::<crate::AppState>()
        .preferences
        .lock()
        .map(|p| p.clone())
        .ok();
    let Some(p) = preferences.filter(|p| p.show_cleanup_results) else {
        return;
    };
    let mut notification = app
        .notification()
        .builder()
        .title("OpenMemDuct")
        .body(notification_body(report));
    if p.notification_sound {
        notification = notification.sound("Default");
    }
    if let Err(error) = notification.show() {
        crate::report_control_error(app, format!("Notification could not be shown: {error}"));
    }
}

fn notification_body(report: &CleanupReport) -> String {
    let status = match report.status {
        CleanupStatus::Completed => "Cleanup completed",
        CleanupStatus::Partial => "Cleanup partly completed",
        CleanupStatus::Cancelled => "Cleanup cancelled",
        CleanupStatus::PermissionDenied => "Cleanup permission denied",
        CleanupStatus::Unsupported => "Cleanup unavailable",
        CleanupStatus::Failed => "Cleanup failed",
        CleanupStatus::Unknown => "Cleanup result unknown",
    };
    match report
        .before
        .as_ref()
        .and_then(|s| s.metrics.as_ref())
        .zip(report.after.as_ref().and_then(|s| s.metrics.as_ref()))
    {
        Some((before, after)) => format!(
            "{status}. Available RAM change: {:+.1} MiB.",
            (after.physical_available as i64 - before.physical_available as i64) as f64 / 1048576.0
        ),
        None => format!("{status}. {}", report.message),
    }
}

#[derive(Default)]
pub struct AutoCleaner {
    configuration: Option<(bool, u8, bool, u16)>,
    interval_started: u64,
    threshold_latched: bool,
}
impl AutoCleaner {
    pub fn due(&mut self, p: &Preferences, usage: Option<f64>, now: u64, allowed: bool) -> bool {
        let key = (
            p.auto_threshold_enabled,
            p.auto_threshold_percent,
            p.auto_interval_enabled,
            p.auto_interval_minutes,
        );
        if self.configuration != Some(key) {
            self.configuration = Some(key);
            self.interval_started = now;
            self.threshold_latched = false;
        }
        if usage.is_some_and(|v| v <= p.auto_threshold_percent.saturating_sub(5) as f64) {
            self.threshold_latched = false;
        }
        if !allowed || usage.is_none() || p.cleanup_regions.is_empty() {
            return false;
        }
        let threshold = p.auto_threshold_enabled
            && !self.threshold_latched
            && usage.is_some_and(|v| v >= p.auto_threshold_percent as f64);
        let interval = p.auto_interval_enabled
            && now.saturating_sub(self.interval_started) >= p.auto_interval_minutes as u64 * 60_000;
        if threshold || interval {
            self.threshold_latched = true;
            self.interval_started = now;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_needs_an_explicit_modifier() {
        let p = Preferences {
            hotkey_enabled: true,
            ..Default::default()
        };
        let key = parse_shortcut("Ctrl+F1").unwrap();
        assert!(shortcut_matches(&p, &key, false));
        assert!(!shortcut_matches(&p, &key, true));
        assert!(parse_shortcut("Ctrl+F1").is_ok());
        assert!(parse_shortcut("F1").is_err());
        assert!(parse_shortcut("Shift+A").is_err());
        assert!(parse_shortcut("Ctrl+Anything").is_err());
        assert!(parse_shortcut("Ctrl+Ctrl+F1").is_err());
        assert!(crate::cleanup::valid_request_id(&format!(
            "{:?}",
            windows::core::GUID::new().unwrap()
        )));
    }
    #[test]
    fn automation_latches_rearms_and_respects_session_and_cooldown() {
        let mut p = Preferences {
            auto_threshold_enabled: true,
            ..Default::default()
        };
        let mut timer = AutoCleaner::default();
        assert!(!timer.due(&p, Some(90.0), 0, false));
        assert!(timer.due(&p, Some(90.0), 1000, true));
        assert!(!timer.due(&p, Some(90.0), 61_000, true));
        assert!(!timer.due(&p, Some(74.0), 62_000, true));
        assert!(!timer.due(&p, Some(85.0), 63_000, false));
        assert!(timer.due(&p, Some(85.0), 64_000, true));
        p.auto_threshold_enabled = false;
        p.auto_interval_enabled = true;
        p.auto_interval_minutes = 90;
        assert!(!timer.due(&p, Some(50.0), 100_000, true));
        assert!(!timer.due(&p, Some(50.0), 5_499_999, true));
        assert!(!timer.due(&p, None, 5_500_000, true));
        assert!(timer.due(&p, Some(50.0), 5_500_000, true));
        assert!(!timer.due(&p, Some(50.0), 5_501_000, true));
    }
    #[test]
    fn notifications_preserve_a_negative_observation() {
        let mut report = crate::cleanup::empty_report(
            "aabbccdd-1122-3344-5566-778899001122",
            &[],
            CleanupStatus::Completed,
            String::new(),
        );
        let mut sample: MemorySnapshot =
            serde_json::from_str(include_str!("../../tests/fixtures/snapshot.json")).unwrap();
        report.before = Some(sample.clone());
        sample.metrics.as_mut().unwrap().physical_available -= 64 * 1024 * 1024;
        report.after = Some(sample);
        assert!(notification_body(&report).contains("-64.0 MiB"));
    }
}
