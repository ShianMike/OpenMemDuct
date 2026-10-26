use crate::{memory::Sampler, models::*};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct CleanupCoordinator {
    pub state: CleanupState,
    seen: VecDeque<String>,
    completed_at: Option<Instant>,
    observation: Option<(String, Instant)>,
}

pub fn valid_request_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

impl CleanupCoordinator {
    pub fn begin(&mut self, id: &str) -> Result<(), String> {
        if !valid_request_id(id) {
            return Err("Invalid cleanup request identifier".into());
        }
        if self.state.busy {
            return Err("A cleanup is already running".into());
        }
        if self.seen.iter().any(|seen| seen == id) {
            return Err("This cleanup request was already handled".into());
        }
        if self
            .completed_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(60))
        {
            return Err("Wait for the observation period to finish".into());
        }
        self.seen.push_back(id.into());
        if self.seen.len() > 128 {
            self.seen.pop_front();
        }
        self.state.busy = true;
        Ok(())
    }

    pub fn finish(&mut self, report: CleanupReport) {
        if matches!(
            report.status,
            CleanupStatus::Completed | CleanupStatus::Partial
        ) {
            self.completed_at = Some(Instant::now());
            self.state.cooldown_until_ms = now_ms() + 60_000;
            self.observation = Some((report.id.clone(), Instant::now()));
        }
        self.state.reports.insert(0, report);
        self.state.reports.truncate(20);
        self.state.busy = false;
    }

    pub fn observe(&mut self, snapshot: &MemorySnapshot) -> bool {
        let Some((id, at)) = &self.observation else {
            return false;
        };
        let elapsed = at.elapsed().as_secs();
        let Some(report) = self.state.reports.iter_mut().find(|r| r.id == *id) else {
            self.observation = None;
            return false;
        };
        let mut changed = false;
        if let Some(target) = [1, 10, 60]
            .into_iter()
            .rev()
            .find(|&seconds| elapsed >= seconds)
        {
            if !report.observations.iter().any(|o| o.at_seconds >= target) {
                report.observations.push(Observation {
                    at_seconds: elapsed,
                    snapshot: snapshot.clone(),
                });
                changed = true;
            }
        }
        if elapsed >= 60 {
            self.observation = None;
        }
        changed
    }

    pub fn fast_observation(&self) -> bool {
        self.observation
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() < Duration::from_secs(10))
    }
}

pub fn empty_report(
    id: &str,
    regions: &[CleanupAction],
    status: CleanupStatus,
    message: String,
) -> CleanupReport {
    CleanupReport {
        id: id.into(),
        regions: regions.to_vec(),
        results: vec![],
        status,
        started_at_ms: now_ms(),
        duration_ms: None,
        before: None,
        after: None,
        message,
        error_code: None,
        observations: vec![],
    }
}

#[cfg(windows)]
pub fn windows_build() -> Option<u32> {
    use windows::{
        core::{s, w},
        Win32::{
            Foundation::NTSTATUS,
            System::{
                LibraryLoader::{GetModuleHandleW, GetProcAddress},
                SystemInformation::OSVERSIONINFOW,
            },
        },
    };
    unsafe {
        let module = GetModuleHandleW(w!("ntdll.dll")).ok()?;
        let address = GetProcAddress(module, s!("RtlGetVersion"))?;
        let query: unsafe extern "system" fn(*mut OSVERSIONINFOW) -> NTSTATUS =
            std::mem::transmute(address);
        let mut info = OSVERSIONINFOW {
            dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOW>() as u32,
            ..Default::default()
        };
        (query(&mut info).0 >= 0).then_some(info.dwBuildNumber)
    }
}

pub fn capabilities() -> Capabilities {
    #[cfg(windows)]
    let build = windows_build();
    #[cfg(not(windows))]
    let build = None;
    // shortcut: cleanup compatibility is limited to these Windows 11 build families; validate each new family before adding it.
    let supported = cfg!(windows) && cfg!(target_arch = "x86_64") && matches!(build, Some(26200));
    Capabilities {
        platform: std::env::consts::OS.into(),
        build,
        cleanup_supported: supported,
        elevated: crate::controls::is_elevated(),
        reason: (!supported).then(|| {
            "Cleanup is unavailable on this Windows build. Monitoring remains available.".into()
        }),
    }
}

pub fn validate_regions(regions: &[CleanupAction], allow_empty: bool) -> Result<(), String> {
    if (!allow_empty && regions.is_empty())
        || regions.len() > 8
        || regions
            .iter()
            .enumerate()
            .any(|(i, r)| regions[..i].contains(r))
    {
        return Err("Choose distinct memory regions".into());
    }
    Ok(())
}

pub fn encode_regions(regions: &[CleanupAction]) -> String {
    regions
        .iter()
        .map(|r| match r {
            CleanupAction::WorkingSet => "6",
            CleanupAction::SystemFileCache => "7",
            CleanupAction::ModifiedPageList => "0",
            CleanupAction::FullStandby => "1",
            CleanupAction::LowPriorityStandby => "2",
            CleanupAction::ModifiedFileCache => "3",
            CleanupAction::RegistryCache => "4",
            CleanupAction::CombineMemory => "5",
        })
        .collect::<Vec<_>>()
        .join(",")
}

pub fn parse_regions(value: &str) -> Result<Vec<CleanupAction>, String> {
    if value.len() > 15 {
        return Err("Too many memory regions".into());
    }
    let regions = value
        .split(',')
        .map(|v| match v {
            "6" => Ok(CleanupAction::WorkingSet),
            "7" => Ok(CleanupAction::SystemFileCache),
            "0" => Ok(CleanupAction::ModifiedPageList),
            "1" => Ok(CleanupAction::FullStandby),
            "2" => Ok(CleanupAction::LowPriorityStandby),
            "3" => Ok(CleanupAction::ModifiedFileCache),
            "4" => Ok(CleanupAction::RegistryCache),
            "5" => Ok(CleanupAction::CombineMemory),
            _ => Err("Invalid memory region".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_regions(&regions, false)?;
    Ok(regions)
}

pub fn perform(id: &str, regions: &[CleanupAction], no_op: bool) -> CleanupReport {
    let mut sampler = Sampler::default();
    let mut report = empty_report(id, regions, CleanupStatus::Failed, String::new());
    if let Err(error) = validate_regions(regions, false) {
        report.message = error;
        return report;
    }
    if !no_op && !capabilities().cleanup_supported {
        report.status = CleanupStatus::Unsupported;
        report.message = "This Windows build has not been enabled for cleanup".into();
        return report;
    }
    report.before = Some(sampler.sample());
    report.started_at_ms = now_ms();
    let started = Instant::now();
    for &region in regions {
        let started = Instant::now();
        let result = if no_op {
            Ok(())
        } else {
            #[cfg(windows)]
            {
                enable_privilege(region)
                    .map_err(|e| (CleanupStatus::PermissionDenied, e))
                    .and_then(|_| native_operation(region))
            }
            #[cfg(not(windows))]
            {
                native_operation(region)
            }
        };
        let (status, error_code) = match result {
            Ok(()) => (CleanupStatus::Completed, None),
            Err((status, code)) => (status, Some(code)),
        };
        report.results.push(RegionResult {
            region,
            status,
            duration_ms: started.elapsed().as_millis() as u64,
            error_code,
        });
    }
    report.duration_ms = Some(started.elapsed().as_millis() as u64);
    report.after = Some(sampler.sample());
    report.status = aggregate_status(&report.results);
    report.message = if no_op {
        "No-op transport test completed".into()
    } else if report.status == CleanupStatus::Completed {
        "Selected operations completed. RAM changes are observations.".into()
    } else {
        "Some selected operations did not complete. Check individual results.".into()
    };
    report
}

fn aggregate_status(results: &[RegionResult]) -> CleanupStatus {
    if results.iter().all(|r| r.status == CleanupStatus::Completed) {
        CleanupStatus::Completed
    } else if results
        .iter()
        .any(|r| r.status == CleanupStatus::Completed || r.status == CleanupStatus::Partial)
    {
        CleanupStatus::Partial
    } else {
        results
            .first()
            .map(|r| r.status.clone())
            .unwrap_or(CleanupStatus::Failed)
    }
}

#[cfg(windows)]
fn enable_privilege(region: CleanupAction) -> Result<(), String> {
    use crate::memory::Handle;
    use windows::{
        core::w,
        Win32::{
            Foundation::{GetLastError, ERROR_NOT_ALL_ASSIGNED, HANDLE, LUID},
            Security::{
                AdjustTokenPrivileges, GetTokenInformation, LookupPrivilegeValueW, TokenElevation,
                LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
                TOKEN_ELEVATION, TOKEN_PRIVILEGES, TOKEN_QUERY,
            },
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        },
    };
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES,
            &mut token,
        )
        .map_err(|e| e.to_string())?;
        let token = Handle(token);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0;
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of_val(&elevation) as u32,
            &mut returned,
        )
        .map_err(|e| e.to_string())?;
        if elevation.TokenIsElevated == 0 {
            return Err("Administrator approval is required".into());
        }
        let privilege = match region {
            CleanupAction::ModifiedFileCache | CleanupAction::RegistryCache => return Ok(()),
            CleanupAction::SystemFileCache => w!("SeIncreaseQuotaPrivilege"),
            _ => w!("SeProfileSingleProcessPrivilege"),
        };
        let mut luid = LUID::default();
        LookupPrivilegeValueW(None, privilege, &mut luid).map_err(|e| e.to_string())?;
        let privileges = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        AdjustTokenPrivileges(token.0, false, Some(&privileges), 0, None, None)
            .map_err(|e| e.to_string())?;
        if GetLastError() == ERROR_NOT_ALL_ASSIGNED {
            return Err("Windows did not grant the required privilege".into());
        }
        Ok(())
    }
}

#[cfg(windows)]
fn native_operation(action: CleanupAction) -> Result<(), (CleanupStatus, String)> {
    use windows::{
        core::{s, w},
        Win32::{
            Foundation::NTSTATUS,
            System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        },
    };
    unsafe {
        let module = GetModuleHandleW(w!("ntdll.dll"))
            .map_err(|e| (CleanupStatus::Unsupported, e.to_string()))?;
        let address = GetProcAddress(module, s!("NtSetSystemInformation")).ok_or((
            CleanupStatus::Unsupported,
            "NtSetSystemInformation unavailable".into(),
        ))?;
        let set: unsafe extern "system" fn(i32, *mut core::ffi::c_void, u32) -> NTSTATUS =
            std::mem::transmute(address);
        // Private Native API information classes and layouts are isolated here.
        let status = match action {
            CleanupAction::SystemFileCache => {
                let mut info = crate::memory::FileCacheInformation {
                    minimum_working_set: usize::MAX,
                    maximum_working_set: usize::MAX,
                    ..Default::default()
                };
                set(
                    81,
                    &mut info as *mut _ as *mut _,
                    std::mem::size_of_val(&info) as u32,
                )
            }
            CleanupAction::ModifiedFileCache => return flush_fixed_volumes(),
            CleanupAction::RegistryCache => set(155, std::ptr::null_mut(), 0),
            CleanupAction::CombineMemory => {
                #[repr(C)]
                #[derive(Default)]
                struct CombineInformation {
                    handle: usize,
                    pages_combined: usize,
                    flags: u32,
                }
                let mut info = CombineInformation::default();
                set(
                    130,
                    &mut info as *mut _ as *mut _,
                    std::mem::size_of_val(&info) as u32,
                )
            }
            _ => {
                let mut command: u32 = match action {
                    CleanupAction::WorkingSet => 2,
                    CleanupAction::ModifiedPageList => 3,
                    CleanupAction::FullStandby => 4,
                    _ => 5,
                };
                set(
                    80,
                    &mut command as *mut _ as *mut _,
                    std::mem::size_of_val(&command) as u32,
                )
            }
        };
        if status.0 >= 0 {
            return Ok(());
        }
        let category = match status.0 as u32 {
            0xC0000061 | 0xC0000022 => CleanupStatus::PermissionDenied,
            0xC0000002 | 0xC0000003 | 0xC00000BB => CleanupStatus::Unsupported,
            _ => CleanupStatus::Failed,
        };
        Err((category, format!("0x{:08X}", status.0 as u32)))
    }
}

#[cfg(windows)]
fn flush_fixed_volumes() -> Result<(), (CleanupStatus, String)> {
    use crate::memory::Handle;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{GetLastError, ERROR_NO_MORE_FILES, GENERIC_WRITE},
            Storage::FileSystem::{
                CreateFileW, FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, FlushFileBuffers,
                GetDriveTypeW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE,
                OPEN_EXISTING,
            },
            System::WindowsProgramming::DRIVE_FIXED,
        },
    };
    unsafe {
        let mut buffer = [0u16; 1024];
        let search =
            FindFirstVolumeW(&mut buffer).map_err(|e| (CleanupStatus::Failed, e.to_string()))?;
        let mut flushed = 0;
        let mut errors = Vec::new();
        loop {
            let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            if length < buffer.len() && GetDriveTypeW(PCWSTR(buffer.as_ptr())) == DRIVE_FIXED {
                let mut name = buffer[..length].to_vec();
                if name.last() == Some(&(b'\\' as u16)) {
                    name.pop();
                }
                name.push(0);
                let result = CreateFileW(
                    PCWSTR(name.as_ptr()),
                    GENERIC_WRITE.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
                .and_then(|h| {
                    let h = Handle(h);
                    FlushFileBuffers(h.0)
                });
                match result {
                    Ok(()) => flushed += 1,
                    Err(e) => errors.push(e.code().0),
                }
            }
            if FindNextVolumeW(search, &mut buffer).is_err() {
                if GetLastError() != ERROR_NO_MORE_FILES {
                    errors.push(GetLastError().0 as i32);
                }
                break;
            }
        }
        let close = FindVolumeClose(search);
        if let Err(e) = close {
            errors.push(e.code().0);
        }
        if !errors.is_empty() {
            return Err((
                if flushed > 0 {
                    CleanupStatus::Partial
                } else {
                    CleanupStatus::Failed
                },
                format!(
                    "{flushed} volumes flushed; {} errors, first 0x{:08X}",
                    errors.len(),
                    errors[0] as u32
                ),
            ));
        }
        if flushed == 0 {
            return Err((
                CleanupStatus::Unsupported,
                "No writable local fixed volumes found".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(not(windows))]
fn native_operation(_: CleanupAction) -> Result<(), (CleanupStatus, String)> {
    Err((CleanupStatus::Unsupported, "Windows required".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "aabbccdd-1122-3344-5566-778899001122";
    #[test]
    fn validates_id_and_serializes_requests() {
        assert!(!valid_request_id("../path"));
        let mut c = CleanupCoordinator::default();
        c.begin(ID).unwrap();
        assert!(c.begin("11223344-1122-3344-5566-778899001122").is_err());
        c.finish(empty_report(
            ID,
            &[CleanupAction::LowPriorityStandby],
            CleanupStatus::Cancelled,
            "Cancelled".into(),
        ));
        assert!(c.begin(ID).is_err());
        c.begin("11223344-1122-3344-5566-778899001122").unwrap();
        c.finish(empty_report(
            ID,
            &[CleanupAction::LowPriorityStandby],
            CleanupStatus::Completed,
            "Complete".into(),
        ));
        assert!(c.begin("22334455-1122-3344-5566-778899001122").is_err());
    }
    #[test]
    fn no_op_does_not_modify_memory_and_history_is_bounded() {
        let report = perform(ID, &[CleanupAction::LowPriorityStandby], true);
        assert_eq!(report.status, CleanupStatus::Completed);
        let mut c = CleanupCoordinator::default();
        for _ in 0..25 {
            c.finish(empty_report(
                ID,
                &[CleanupAction::LowPriorityStandby],
                CleanupStatus::Cancelled,
                String::new(),
            ));
        }
        assert_eq!(c.state.reports.len(), 20);
    }

    #[test]
    fn regions_are_allowlisted_unique_and_roundtrip() {
        let all = parse_regions("6,7,0,1,2,3,4,5").unwrap();
        assert_eq!(encode_regions(&all), "6,7,0,1,2,3,4,5");
        for value in ["", "2,2", "8", "1;cmd", "0,1,2,3,4,5,6,7,0"] {
            assert!(parse_regions(value).is_err());
        }
        assert!(validate_regions(&[], true).is_ok());
        assert!(validate_regions(&[], false).is_err());
        let report = perform(ID, &all, true);
        assert_eq!(report.results.len(), 8);
        assert_eq!(report.status, CleanupStatus::Completed);
        let mut results = report.results;
        results[0].status = CleanupStatus::PermissionDenied;
        assert_eq!(aggregate_status(&results), CleanupStatus::Partial);
        for result in &mut results {
            result.status = CleanupStatus::Unsupported;
        }
        assert_eq!(aggregate_status(&results), CleanupStatus::Unsupported);
    }

    #[test]
    fn delayed_observations_keep_the_actual_elapsed_time() {
        let mut coordinator = CleanupCoordinator::default();
        coordinator.finish(empty_report(
            ID,
            &[CleanupAction::WorkingSet],
            CleanupStatus::Completed,
            String::new(),
        ));
        let sample = Sampler::default().sample();
        coordinator.observation = Some((ID.into(), Instant::now() - Duration::from_secs(16)));
        assert!(coordinator.observe(&sample));
        assert_eq!(coordinator.state.reports[0].observations[0].at_seconds, 16);
        assert!(!coordinator.observe(&sample));
        coordinator.observation = Some((ID.into(), Instant::now() - Duration::from_secs(65)));
        assert!(coordinator.observe(&sample));
        let times: Vec<_> = coordinator.state.reports[0]
            .observations
            .iter()
            .map(|o| o.at_seconds)
            .collect();
        assert_eq!(times, vec![16, 65]);
        assert!(coordinator.observation.is_none());
    }
}
