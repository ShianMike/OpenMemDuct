//! Read-only process measurements for development validation.

use crate::memory::Handle;
use serde::Serialize;
use std::collections::HashSet;
use windows::Win32::{
    Foundation::FILETIME,
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2,
        },
        Threading::{
            GetProcessHandleCount, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_VM_READ,
        },
    },
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMeasurement {
    pub pid: u32,
    pub name: String,
    pub private_working_set: u64,
    pub private_commit: u64,
    pub cpu_ticks: u64,
    pub handles: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessGroup {
    pub timestamp_ms: u64,
    pub root_pid: u32,
    pub processes: Vec<ProcessMeasurement>,
    pub unreadable: Vec<u32>,
}

fn ticks(time: FILETIME) -> u64 {
    ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
}

pub fn process_group(root_pid: u32) -> Result<ProcessGroup, String> {
    unsafe {
        let snapshot =
            Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?);
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        Process32FirstW(snapshot.0, &mut entry).map_err(|e| e.to_string())?;
        let mut entries = Vec::new();
        loop {
            entries.push(entry);
            if Process32NextW(snapshot.0, &mut entry).is_err() {
                break;
            }
        }
        if !entries.iter().any(|e| e.th32ProcessID == root_pid) {
            return Err("Root process is no longer running".into());
        }
        let mut owned = HashSet::from([root_pid]);
        loop {
            let before = owned.len();
            for e in &entries {
                if owned.contains(&e.th32ParentProcessID) {
                    owned.insert(e.th32ProcessID);
                }
            }
            if owned.len() == before {
                break;
            }
        }
        let mut result = ProcessGroup {
            timestamp_ms: crate::models::now_ms(),
            root_pid,
            processes: vec![],
            unreadable: vec![],
        };
        for e in entries
            .into_iter()
            .filter(|e| owned.contains(&e.th32ProcessID))
        {
            let read = || -> Result<ProcessMeasurement, windows::core::Error> {
                let process = Handle(OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                    false,
                    e.th32ProcessID,
                )?);
                let mut counters = PROCESS_MEMORY_COUNTERS_EX2 {
                    cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
                    ..Default::default()
                };
                GetProcessMemoryInfo(
                    process.0,
                    &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                    counters.cb,
                )?;
                let mut created = Default::default();
                let mut exit = Default::default();
                let mut kernel = Default::default();
                let mut user = Default::default();
                GetProcessTimes(process.0, &mut created, &mut exit, &mut kernel, &mut user)?;
                let mut handles = 0;
                GetProcessHandleCount(process.0, &mut handles)?;
                let length = e
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(e.szExeFile.len());
                Ok(ProcessMeasurement {
                    pid: e.th32ProcessID,
                    name: String::from_utf16_lossy(&e.szExeFile[..length]),
                    private_working_set: counters.PrivateWorkingSetSize as u64,
                    private_commit: counters.PrivateUsage as u64,
                    cpu_ticks: ticks(kernel) + ticks(user),
                    handles,
                })
            };
            match read() {
                Ok(p) => result.processes.push(p),
                Err(_) => result.unreadable.push(e.th32ProcessID),
            }
        }
        Ok(result)
    }
}

pub(crate) fn validate_lifecycle(app: &tauri::AppHandle) -> Result<(), String> {
    use std::{
        io::Write,
        sync::atomic::Ordering,
        time::{Duration, Instant},
    };
    use tauri::Manager;
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Workspace unavailable")?
        .join("validation");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let mut evidence = std::fs::File::create(directory.join("native-lifecycle.jsonl"))
        .map_err(|e| e.to_string())?;
    let mut record = |event: &str, cycle: usize| -> Result<(), String> {
        let group = process_group(std::process::id())?;
        let history = app
            .state::<crate::AppState>()
            .history
            .lock()
            .map_err(|e| e.to_string())?
            .len();
        let json = serde_json::json!({"event":event,"cycle":cycle,"history":history,"group":group});
        writeln!(evidence, "{json}").map_err(|e| e.to_string())?;
        evidence.flush().map_err(|e| e.to_string())?;
        println!("Native validation: {event} {cycle}");
        Ok(())
    };
    record("baseline", 0)?;
    for cycle in 1..=20 {
        let count = app
            .state::<crate::AppState>()
            .bootstrap_count
            .load(Ordering::Relaxed);
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = app.clone();
        app.run_on_main_thread(move || {
            let _ = sender.send(crate::open_dashboard(&handle).map_err(|e| e.to_string()));
        })
        .map_err(|e| e.to_string())?;
        receiver
            .recv_timeout(Duration::from_secs(15))
            .map_err(|e| e.to_string())??;
        let started = Instant::now();
        while app
            .state::<crate::AppState>()
            .bootstrap_count
            .load(Ordering::Relaxed)
            == count
        {
            if started.elapsed() > Duration::from_secs(15) {
                return Err("Dashboard did not load native bootstrap".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        record("loaded", cycle)?;
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = app.clone();
        app.run_on_main_thread(move || {
            let result = handle
                .get_webview_window("main")
                .ok_or_else(|| "Dashboard missing".to_owned())
                .and_then(|w| w.destroy().map_err(|e| e.to_string()));
            let _ = sender.send(result);
        })
        .map_err(|e| e.to_string())?;
        receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| e.to_string())??;
        let started = Instant::now();
        loop {
            let group = process_group(std::process::id())?;
            if !group
                .processes
                .iter()
                .any(|p| p.name.eq_ignore_ascii_case("msedgewebview2.exe"))
            {
                break;
            }
            if started.elapsed() > Duration::from_secs(15) {
                return Err("WebView2 remained alive after dashboard destruction".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        record("closed", cycle)?;
    }
    record("idle-start", 20)?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(1800) {
        std::thread::sleep(Duration::from_secs(30));
        if app.get_webview_window("main").is_some() {
            return Err("Idle validation interrupted by dashboard opening".into());
        }
        record("idle", 20)?;
    }
    record("passed", 20)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_group_contains_its_root() {
        let group = process_group(std::process::id()).unwrap();
        assert!(group.processes.iter().any(|p| p.pid == group.root_pid));
        assert!(process_group(u32::MAX).is_err());
    }
}
