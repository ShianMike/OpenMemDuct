use crate::models::Preferences;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

pub fn load(path: &Path) -> Result<Preferences, String> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(4097)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > 4096 {
                return Err("Preferences file is unexpectedly large".into());
            }
            let preferences: Preferences = serde_json::from_slice(&bytes)
                .map_err(|e| format!("Preferences could not be read: {e}"))?;
            validate(&preferences)?;
            Ok(preferences)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(e) => Err(format!("Preferences could not be read: {e}")),
    }
}

pub fn save(path: &Path, preferences: &Preferences) -> Result<(), String> {
    validate(preferences)?;
    let directory = path.parent().ok_or("Preferences directory is missing")?;
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("pending");
    let bytes = serde_json::to_vec_pretty(preferences).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| format!("Preferences were not saved; previous settings are intact: {e}"))?;
    let result = (|| -> Result<(), String> {
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        replace(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn validate(p: &Preferences) -> Result<(), String> {
    crate::cleanup::validate_regions(&p.cleanup_regions, true)?;
    if p.warning_percent < 1 || p.danger_percent > 100 || p.warning_percent >= p.danger_percent {
        return Err("Warning must be lower than danger, between 1% and 100%".into());
    }
    if !(1..=100).contains(&p.auto_threshold_percent)
        || !(1..=1440).contains(&p.auto_interval_minutes)
    {
        return Err("Invalid automatic cleanup threshold or interval".into());
    }
    if (p.auto_threshold_enabled || p.auto_interval_enabled) && p.cleanup_regions.is_empty() {
        return Err("Select at least one cleanup region for automatic cleanup".into());
    }
    crate::controls::parse_shortcut(&p.hotkey)?;
    Ok(())
}

#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        },
    };
    let from: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unknown_fields_and_invalid_values() {
        assert!(serde_json::from_str::<Preferences>(r#"{"startInTray":"yes"}"#).is_err());
        assert!(serde_json::from_str::<Preferences>(r#"{"autoUpdateEnabled":"yes"}"#).is_err());
        assert!(serde_json::from_str::<Preferences>(
            r#"{"startInTray":false,"command":"anything"}"#
        )
        .is_err());
        let preferences = serde_json::from_str::<Preferences>(r#"{"startInTray":true}"#).unwrap();
        assert!(preferences.start_in_tray);
        assert!(!preferences.start_with_windows);
        assert!(!preferences.auto_update_enabled);
        assert_eq!(
            preferences.cleanup_regions,
            crate::models::default_regions()
        );
        assert_eq!(preferences.warning_percent, 70);
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/preferences.json")).unwrap();
        assert_eq!(
            serde_json::to_value(Preferences::default()).unwrap(),
            expected
        );
        let mut legacy = expected;
        legacy.as_object_mut().unwrap().remove("autoUpdateEnabled");
        assert_eq!(
            serde_json::from_value::<Preferences>(legacy).unwrap(),
            Preferences::default()
        );
        assert!(validate(&Preferences {
            warning_percent: 90,
            danger_percent: 90,
            ..Default::default()
        })
        .is_err());
        assert!(validate(&Preferences {
            auto_interval_minutes: 0,
            ..Default::default()
        })
        .is_err());
        assert!(validate(&Preferences {
            auto_interval_minutes: 1441,
            ..Default::default()
        })
        .is_err());
        assert!(validate(&Preferences {
            hotkey: "Ctrl+F25".into(),
            ..Default::default()
        })
        .is_err());
        assert!(validate(&Preferences {
            auto_threshold_enabled: true,
            cleanup_regions: vec![],
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn saves_roundtrip_and_preserves_previous_bytes_on_failure() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("validation/settings-io")
            .join(format!(
                "{}-{}",
                crate::models::now_ms(),
                std::process::id()
            ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("preferences.json");
        assert_eq!(load(&path).unwrap(), Preferences::default());
        let first = Preferences::default();
        save(&path, &first).unwrap();
        assert_eq!(load(&path).unwrap(), first);
        let second = Preferences {
            start_with_windows: true,
            start_in_tray: true,
            auto_update_enabled: true,
            cleanup_regions: vec![crate::models::CleanupAction::SystemFileCache],
            ..Default::default()
        };
        save(&path, &second).unwrap();
        assert_eq!(load(&path).unwrap(), second);
        let before = fs::read(&path).unwrap();
        let pending = path.with_extension("pending");
        fs::write(&pending, b"Existing interrupted write").unwrap();
        assert!(save(&path, &first).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read(&pending).unwrap(), b"Existing interrupted write");
        let duplicate = Preferences {
            start_in_tray: false,
            cleanup_regions: vec![crate::models::CleanupAction::WorkingSet; 2],
            ..Default::default()
        };
        assert!(save(&path, &duplicate).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
