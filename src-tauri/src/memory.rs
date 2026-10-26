use crate::models::*;
use std::time::Instant;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Memory::{
            CreateMemoryResourceNotification, LowMemoryResourceNotification,
            QueryMemoryResourceNotification,
        },
        ProcessStatus::{
            EnumPageFilesW, GetPerformanceInfo, GetProcessMemoryInfo, ENUM_PAGE_FILE_INFORMATION,
            PERFORMANCE_INFORMATION, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
            PROCESS_MEMORY_COUNTERS_EX2,
        },
        Threading::{
            GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
        },
    },
};

#[cfg(windows)]
pub struct Handle(pub HANDLE);
#[cfg(windows)]
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
// A handle has one owner; Windows allows that owner to move it between threads.
#[cfg(windows)]
unsafe impl Send for Handle {}

pub struct Sampler {
    sequence: u64,
    started: Instant,
    #[cfg(windows)]
    notification: Option<Handle>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self {
            sequence: 0,
            started: Instant::now(),
            #[cfg(windows)]
            notification: unsafe {
                CreateMemoryResourceNotification(LowMemoryResourceNotification)
                    .ok()
                    .map(Handle)
            },
        }
    }
}

pub fn pages_to_bytes(pages: usize, size: usize) -> Result<u64, String> {
    (pages as u64)
        .checked_mul(size as u64)
        .filter(|&n| n <= MAX_SAFE_INTEGER)
        .ok_or_else(|| "Memory counter exceeds supported precision".into())
}

impl Sampler {
    pub fn sample(&mut self) -> MemorySnapshot {
        self.sequence += 1;
        let result = self.read();
        let (metrics, error) = match result {
            Ok(m) => (Some(m), None),
            Err(e) => (None, Some(e)),
        };
        MemorySnapshot {
            sequence: self.sequence,
            timestamp_ms: now_ms(),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            metrics,
            error,
        }
    }

    #[cfg(windows)]
    fn read(&self) -> Result<MemoryMetrics, String> {
        unsafe {
            let mut info = PERFORMANCE_INFORMATION {
                cb: std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32,
                ..Default::default()
            };
            GetPerformanceInfo(
                &mut info,
                std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32,
            )
            .map_err(|e| e.to_string())?;
            let mut low = windows::core::BOOL(0);
            let low_memory = self.notification.as_ref().and_then(|h| {
                QueryMemoryResourceNotification(h.0, &mut low)
                    .ok()
                    .map(|_| low.as_bool())
            });
            let pagefiles = pagefile_counters(info.PageSize);
            let working_set = system_working_set();
            let result = MemoryMetrics {
                physical_total: pages_to_bytes(info.PhysicalTotal, info.PageSize)?,
                physical_available: pages_to_bytes(info.PhysicalAvailable, info.PageSize)?,
                commit_used: pages_to_bytes(info.CommitTotal, info.PageSize)?,
                commit_limit: pages_to_bytes(info.CommitLimit, info.PageSize)?,
                system_cache: pages_to_bytes(info.SystemCache, info.PageSize)?,
                kernel_paged: pages_to_bytes(info.KernelPaged, info.PageSize)?,
                kernel_nonpaged: pages_to_bytes(info.KernelNonpaged, info.PageSize)?,
                low_memory,
                pagefile_total: pagefiles.as_ref().ok().map(|&(total, _)| total),
                pagefile_used: pagefiles.as_ref().ok().map(|&(_, used)| used),
                pagefile_error: pagefiles.err(),
                system_working_set: working_set.as_ref().ok().map(|&(current, _)| current),
                system_working_set_peak: working_set.as_ref().ok().map(|&(_, peak)| peak),
                system_working_set_error: working_set.err(),
            };
            result.validate()?;
            Ok(result)
        }
    }

    #[cfg(not(windows))]
    fn read(&self) -> Result<MemoryMetrics, String> {
        Err("Native memory monitoring requires Windows".into())
    }
}

#[cfg(windows)]
fn pagefile_counters(page_size: usize) -> Result<(u64, u64), String> {
    struct Totals {
        total: usize,
        used: usize,
        valid: bool,
    }
    unsafe extern "system" fn collect(
        context: *mut core::ffi::c_void,
        info: *mut ENUM_PAGE_FILE_INFORMATION,
        _: windows::core::PCWSTR,
    ) -> windows::core::BOOL {
        let totals = unsafe { &mut *(context as *mut Totals) };
        let info = unsafe { &*info };
        match (
            totals.total.checked_add(info.TotalSize),
            totals.used.checked_add(info.TotalInUse),
        ) {
            (Some(total), Some(used)) => {
                totals.total = total;
                totals.used = used;
            }
            _ => totals.valid = false,
        }
        windows::core::BOOL(1)
    }
    let mut totals = Totals {
        total: 0,
        used: 0,
        valid: true,
    };
    unsafe {
        EnumPageFilesW(Some(collect), &mut totals as *mut _ as *mut _)
            .map_err(|e| format!("Pagefile counters unavailable: {e}"))?;
    }
    if !totals.valid {
        return Err("Pagefile counters exceed supported precision".into());
    }
    Ok((
        pages_to_bytes(totals.total, page_size)?,
        pages_to_bytes(totals.used, page_size)?,
    ))
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
pub(crate) struct FileCacheInformation {
    pub current_size: usize,
    pub peak_size: usize,
    pub page_fault_count: u32,
    pub minimum_working_set: usize,
    pub maximum_working_set: usize,
    pub current_transition_pages: usize,
    pub peak_transition_pages: usize,
    pub transition_repurpose_count: u32,
    pub flags: u32,
}

#[cfg(all(windows, target_pointer_width = "64"))]
const _: () = {
    assert!(std::mem::size_of::<FileCacheInformation>() == 64);
    assert!(std::mem::offset_of!(FileCacheInformation, minimum_working_set) == 24);
    assert!(std::mem::offset_of!(FileCacheInformation, maximum_working_set) == 32);
};

#[cfg(windows)]
fn system_working_set() -> Result<(u64, u64), String> {
    use windows::Wdk::System::SystemInformation::{
        NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS,
    };
    let mut info = FileCacheInformation::default();
    let mut returned = 0;
    // SystemFileCacheInformation reports current/peak cache working set in bytes.
    let status = unsafe {
        NtQuerySystemInformation(
            SYSTEM_INFORMATION_CLASS(21),
            &mut info as *mut _ as *mut _,
            std::mem::size_of_val(&info) as u32,
            &mut returned,
        )
    };
    if status.0 < 0 {
        return Err(format!(
            "System working set unavailable (0x{:08X})",
            status.0 as u32
        ));
    }
    if returned as usize != std::mem::size_of_val(&info) {
        return Err("Unsupported system working-set layout".into());
    }
    Ok((info.current_size as u64, info.peak_size as u64))
}

#[cfg(windows)]
pub fn top_processes() -> Result<ProcessList, String> {
    unsafe {
        let snapshot =
            Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?);
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut rows = Vec::new();
        let mut inaccessible = 0;
        if Process32FirstW(snapshot.0, &mut entry).is_err() {
            return Err("Could not enumerate Windows processes".into());
        }
        loop {
            if entry.th32ProcessID != 0 {
                let process = OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
                    false,
                    entry.th32ProcessID,
                );
                if let Ok(process) = process {
                    let process = Handle(process);
                    let mut counters = PROCESS_MEMORY_COUNTERS_EX2 {
                        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
                        ..Default::default()
                    };
                    let result = GetProcessMemoryInfo(
                        process.0,
                        &mut counters as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                        counters.cb,
                    );
                    let (bytes, metric) = if result.is_ok() {
                        (
                            counters.PrivateWorkingSetSize as u64,
                            ProcessMetric::PrivateWorkingSet,
                        )
                    } else {
                        let mut basic = PROCESS_MEMORY_COUNTERS_EX {
                            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
                            ..Default::default()
                        };
                        if GetProcessMemoryInfo(
                            process.0,
                            &mut basic as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
                            basic.cb,
                        )
                        .is_err()
                        {
                            inaccessible += 1;
                            if Process32NextW(snapshot.0, &mut entry).is_err() {
                                break;
                            }
                            continue;
                        }
                        (basic.PrivateUsage as u64, ProcessMetric::PrivateCommit)
                    };
                    let mut created = Default::default();
                    let mut exited = Default::default();
                    let mut kernel = Default::default();
                    let mut user = Default::default();
                    if GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user)
                        .is_ok()
                        && bytes <= MAX_SAFE_INTEGER
                    {
                        let created_at = (((created.dwHighDateTime as u64) << 32)
                            | created.dwLowDateTime as u64)
                            / 10_000;
                        let len = entry
                            .szExeFile
                            .iter()
                            .position(|&c| c == 0)
                            .unwrap_or(entry.szExeFile.len());
                        rows.push(ProcessRow {
                            pid: entry.th32ProcessID,
                            created_at,
                            name: String::from_utf16_lossy(&entry.szExeFile[..len]),
                            bytes,
                            metric,
                        });
                    } else {
                        inaccessible += 1;
                    }
                } else {
                    inaccessible += 1;
                }
            }
            if Process32NextW(snapshot.0, &mut entry).is_err() {
                break;
            }
        }
        rows.sort_unstable_by_key(|row| std::cmp::Reverse(row.bytes));
        rows.truncate(8);
        Ok(ProcessList {
            rows,
            inaccessible,
            timestamp_ms: now_ms(),
        })
    }
}

#[cfg(not(windows))]
pub fn top_processes() -> Result<ProcessList, String> {
    Err("Process monitoring requires Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_units_and_overflow() {
        assert_eq!(pages_to_bytes(1024, 4096).unwrap(), 4_194_304);
        assert!(pages_to_bytes(usize::MAX, 4096).is_err());
        assert!(pages_to_bytes(1, MAX_SAFE_INTEGER as usize + 1).is_err());
    }
    #[test]
    #[cfg(windows)]
    fn live_counters_are_valid_without_elevation() {
        let reading = Sampler::default().sample();
        assert!(reading.error.is_none(), "{:?}", reading.error);
        reading.metrics.unwrap().validate().unwrap();
    }
}
