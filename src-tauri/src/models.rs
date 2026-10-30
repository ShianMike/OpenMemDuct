use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryMetrics {
    pub physical_total: u64,
    pub physical_available: u64,
    pub commit_used: u64,
    pub commit_limit: u64,
    pub system_cache: u64,
    pub kernel_paged: u64,
    pub kernel_nonpaged: u64,
    pub pagefile_total: Option<u64>,
    pub pagefile_used: Option<u64>,
    pub pagefile_error: Option<String>,
    pub system_working_set: Option<u64>,
    pub system_working_set_peak: Option<u64>,
    pub system_working_set_error: Option<String>,
    pub low_memory: Option<bool>,
}

impl MemoryMetrics {
    pub fn validate(&self) -> Result<(), String> {
        let values = [
            self.physical_total,
            self.physical_available,
            self.commit_used,
            self.commit_limit,
            self.system_cache,
            self.kernel_paged,
            self.kernel_nonpaged,
        ];
        if values.iter().any(|&v| v > MAX_SAFE_INTEGER)
            || self.physical_total == 0
            || self.physical_available > self.physical_total
            || self.commit_limit == 0
        {
            return Err("Windows returned invalid memory counters".into());
        }
        if [
            self.pagefile_total,
            self.pagefile_used,
            self.system_working_set,
            self.system_working_set_peak,
        ]
        .into_iter()
        .flatten()
        .any(|v| v > MAX_SAFE_INTEGER)
            || self.pagefile_used.is_some() != self.pagefile_total.is_some()
            || self.system_working_set.is_some() != self.system_working_set_peak.is_some()
            || self
                .pagefile_used
                .zip(self.pagefile_total)
                .is_some_and(|(used, total)| used > total)
            || self
                .system_working_set
                .zip(self.system_working_set_peak)
                .is_some_and(|(used, peak)| used > peak)
        {
            return Err("Windows returned invalid secondary memory counters".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySnapshot {
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub elapsed_ms: u64,
    pub metrics: Option<MemoryMetrics>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Preferences {
    pub theme: ThemePreference,
    pub start_with_windows: bool,
    pub start_in_tray: bool,
    pub auto_update_enabled: bool,
    #[serde(default = "default_regions")]
    pub cleanup_regions: Vec<CleanupAction>,
    pub warning_percent: u8,
    pub danger_percent: u8,
    pub single_click_action: TrayAction,
    pub middle_click_action: TrayAction,
    pub show_cleanup_results: bool,
    pub notification_sound: bool,
    pub auto_threshold_enabled: bool,
    pub auto_threshold_percent: u8,
    pub auto_interval_enabled: bool,
    pub auto_interval_minutes: u16,
    pub hotkey_enabled: bool,
    pub hotkey: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum TrayAction {
    ToggleWindow,
    CleanMemory,
    None,
}

pub fn default_regions() -> Vec<CleanupAction> {
    vec![
        CleanupAction::WorkingSet,
        CleanupAction::SystemFileCache,
        CleanupAction::LowPriorityStandby,
        CleanupAction::ModifiedFileCache,
        CleanupAction::RegistryCache,
        CleanupAction::CombineMemory,
    ]
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
            start_with_windows: false,
            start_in_tray: false,
            auto_update_enabled: false,
            cleanup_regions: default_regions(),
            warning_percent: 70,
            danger_percent: 90,
            single_click_action: TrayAction::ToggleWindow,
            middle_click_action: TrayAction::CleanMemory,
            show_cleanup_results: true,
            notification_sound: true,
            auto_threshold_enabled: false,
            auto_threshold_percent: 80,
            auto_interval_enabled: false,
            auto_interval_minutes: 90,
            hotkey_enabled: false,
            hotkey: "Ctrl+F1".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CleanupAction {
    WorkingSet,
    SystemFileCache,
    ModifiedPageList,
    LowPriorityStandby,
    FullStandby,
    ModifiedFileCache,
    RegistryCache,
    CombineMemory,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CleanupStatus {
    Completed,
    Partial,
    Cancelled,
    Unsupported,
    PermissionDenied,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub at_seconds: u64,
    pub snapshot: MemorySnapshot,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CleanupReport {
    pub id: String,
    pub regions: Vec<CleanupAction>,
    pub results: Vec<RegionResult>,
    pub status: CleanupStatus,
    pub started_at_ms: u64,
    pub duration_ms: Option<u64>,
    pub before: Option<MemorySnapshot>,
    pub after: Option<MemorySnapshot>,
    pub message: String,
    pub error_code: Option<String>,
    pub observations: Vec<Observation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegionResult {
    pub region: CleanupAction,
    pub status: CleanupStatus,
    pub duration_ms: u64,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupState {
    pub reports: Vec<CleanupReport>,
    pub busy: bool,
    pub cooldown_until_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub platform: String,
    pub build: Option<u32>,
    pub cleanup_supported: bool,
    pub reason: Option<String>,
    pub elevated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub preferences: Preferences,
    pub snapshot: Option<MemorySnapshot>,
    pub history: Vec<MemorySnapshot>,
    pub capabilities: Capabilities,
    pub demo: bool,
    pub settings_error: Option<String>,
    #[serde(flatten)]
    pub cleanup: CleanupState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessMetric {
    PrivateWorkingSet,
    PrivateCommit,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRow {
    pub pid: u32,
    pub created_at: u64,
    pub name: String,
    pub bytes: u64,
    pub metric: ProcessMetric,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessList {
    pub rows: Vec<ProcessRow>,
    pub inaccessible: u32,
    pub timestamp_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_theme_roundtrips_all_values() {
        assert_eq!(ThemePreference::default(), ThemePreference::System);
        assert_eq!(Preferences::default().theme, ThemePreference::System);

        for (theme, expected) in [
            (ThemePreference::System, "system"),
            (ThemePreference::Light, "light"),
            (ThemePreference::Dark, "dark"),
        ] {
            let preferences = Preferences {
                theme,
                start_in_tray: true,
                ..Default::default()
            };
            let encoded = serde_json::to_value(&preferences).unwrap();
            assert_eq!(encoded["theme"], expected);
            assert_eq!(
                serde_json::from_value::<Preferences>(encoded).unwrap(),
                preferences
            );
        }
    }

    #[test]
    fn preferences_reject_invalid_theme_values() {
        for invalid in [
            serde_json::json!("sepia"),
            serde_json::json!(""),
            serde_json::json!("Dark"),
            serde_json::json!("SYSTEM"),
            serde_json::Value::Null,
            serde_json::json!(42),
            serde_json::json!(true),
            serde_json::json!([]),
            serde_json::json!({ "mode": "dark" }),
        ] {
            assert!(
                serde_json::from_value::<Preferences>(serde_json::json!({ "theme": invalid }))
                    .is_err(),
                "Accepted invalid theme: {invalid}"
            );
        }
    }

    #[test]
    fn shared_frontend_contract_roundtrips() {
        let bytes = include_str!("../../tests/fixtures/snapshot.json");
        let snapshot: MemorySnapshot = serde_json::from_str(bytes).unwrap();
        snapshot.metrics.as_ref().unwrap().validate().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(bytes).unwrap(),
            serde_json::to_value(snapshot).unwrap()
        );
    }
}
