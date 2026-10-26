use crate::{cleanup, models::*};

pub struct HelperArguments {
    pub id: String,
    pub regions: Vec<CleanupAction>,
    pub nonce: String,
    pub parent_pid: u32,
    pub parent_created: u64,
    pub no_op: bool,
}

pub fn parse_helper(args: &[String]) -> Result<HelperArguments, String> {
    if args.len() != 6 && args.len() != 7 {
        return Err("Invalid helper arguments".into());
    }
    let regions = cleanup::parse_regions(&args[1])?;
    if !cleanup::valid_request_id(&args[0])
        || args[2].len() != 32
        || !args[2].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Invalid helper identifier".into());
    }
    let parent_pid = args[3]
        .parse::<u32>()
        .map_err(|_| "Invalid parent process")?;
    let parent_created = args[4]
        .parse::<u64>()
        .map_err(|_| "Invalid parent identity")?;
    if parent_pid == 0 || parent_created == 0 || args[5] != "v2" {
        return Err("Invalid helper identity or protocol".into());
    }
    let no_op = args.len() == 7 && args[6] == "--noop" && cfg!(debug_assertions);
    if args.len() == 7 && !no_op {
        return Err("Test mode is unavailable".into());
    }
    Ok(HelperArguments {
        id: args[0].clone(),
        regions,
        nonce: args[2].clone(),
        parent_pid,
        parent_created,
        no_op,
    })
}

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::memory::Handle;
    use std::{
        os::windows::{ffi::OsStrExt, io::AsRawHandle, process::CommandExt},
        process::Child,
        time::{Duration, Instant},
    };
    use windows::{
        core::{w, PCWSTR, PWSTR},
        Win32::{
            Foundation::{
                GetLastError, LocalFree, ERROR_CANCELLED, ERROR_PIPE_CONNECTED,
                ERROR_PIPE_LISTENING, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, WAIT_OBJECT_0,
            },
            Security::{
                Authorization::{
                    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                    SDDL_REVISION_1,
                },
                Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG},
                GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
                TOKEN_QUERY, TOKEN_USER,
            },
            Storage::FileSystem::{
                CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_SHARE_NONE,
                OPEN_EXISTING, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
            },
            System::{
                Com::{
                    CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
                    COINIT_DISABLE_OLE1DDE,
                },
                Pipes::{
                    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId,
                    GetNamedPipeServerProcessId, PeekNamedPipe, PIPE_NOWAIT, PIPE_READMODE_BYTE,
                    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
                },
                Threading::{
                    GetCurrentProcess, GetCurrentProcessId, GetProcessId, GetProcessTimes,
                    OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, WaitForSingleObject,
                    CREATE_NO_WINDOW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
                },
            },
            UI::{
                Shell::{
                    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
                },
                WindowsAndMessaging::SW_HIDE,
            },
        },
    };

    const MAX_MESSAGE: usize = 16 * 1024;
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn pipe_name(nonce: &str) -> Vec<u16> {
        wide(&format!(r"\\.\pipe\OpenMemDuct-{nonce}"))
    }

    fn process_created(process: HANDLE) -> Result<u64, String> {
        let mut created = Default::default();
        let mut exit = Default::default();
        let mut kernel = Default::default();
        let mut user = Default::default();
        unsafe {
            GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user)
                .map_err(|e| e.to_string())?;
        }
        Ok(((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64)
    }

    fn current_sid() -> Result<String, String> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
                .map_err(|e| e.to_string())?;
            let token = Handle(token);
            let mut buffer = vec![0usize; 512];
            let mut returned = 0;
            GetTokenInformation(
                token.0,
                TokenUser,
                Some(buffer.as_mut_ptr() as *mut _),
                (buffer.len() * std::mem::size_of::<usize>()) as u32,
                &mut returned,
            )
            .map_err(|e| e.to_string())?;
            let user = &*(buffer.as_ptr() as *const TOKEN_USER);
            let mut sid = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut sid).map_err(|e| e.to_string())?;
            let result = sid.to_string().map_err(|e| e.to_string());
            let _ = LocalFree(Some(HLOCAL(sid.0 as *mut _)));
            result
        }
    }

    fn server(nonce: &str) -> Result<Handle, String> {
        unsafe {
            let sid = current_sid()?;
            let sddl = wide(&format!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{sid})"));
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
            .map_err(|e| e.to_string())?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            let name = pipe_name(nonce);
            let handle = CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_MESSAGE as u32,
                MAX_MESSAGE as u32,
                0,
                Some(&attributes),
            );
            let error = handle
                .is_invalid()
                .then(|| windows::core::Error::from_thread().to_string());
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            if let Some(error) = error {
                return Err(error);
            }
            Ok(Handle(handle))
        }
    }

    fn write_bytes(pipe: HANDLE, bytes: &[u8]) -> Result<(), String> {
        unsafe {
            let mut written = 0;
            WriteFile(pipe, Some(bytes), Some(&mut written), None).map_err(|e| e.to_string())?;
            if written as usize != bytes.len() {
                return Err("Incomplete helper message".into());
            }
            Ok(())
        }
    }

    fn read_bytes(pipe: HANDLE, size: usize, deadline: Instant) -> Result<Vec<u8>, String> {
        if size > MAX_MESSAGE {
            return Err("Helper message is too large".into());
        }
        let mut bytes = vec![0; size];
        let mut offset = 0;
        while offset < size {
            if Instant::now() >= deadline {
                return Err("Helper response timed out; the result is unknown".into());
            }
            let mut available = 0;
            unsafe {
                PeekNamedPipe(pipe, None, 0, None, Some(&mut available), None)
                    .map_err(|e| e.to_string())?;
            }
            if available == 0 {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            let end = (offset + available as usize).min(size);
            let mut read = 0;
            unsafe {
                ReadFile(pipe, Some(&mut bytes[offset..end]), Some(&mut read), None)
                    .map_err(|e| e.to_string())?;
            }
            if read == 0 {
                return Err("Helper closed the connection".into());
            }
            offset += read as usize;
        }
        Ok(bytes)
    }

    enum Launched {
        Elevated(Handle),
        Test(Child),
    }
    impl Launched {
        fn handle(&self) -> HANDLE {
            match self {
                Self::Elevated(h) => h.0,
                Self::Test(c) => HANDLE(c.as_raw_handle()),
            }
        }
    }

    fn launch(arguments: &[String], no_op: bool) -> Result<Launched, (CleanupStatus, String)> {
        let exe = std::env::current_exe().map_err(|e| (CleanupStatus::Failed, e.to_string()))?;
        if no_op {
            return std::process::Command::new(exe)
                .arg("--cleanup-helper")
                .args(arguments)
                .creation_flags(CREATE_NO_WINDOW.0)
                .spawn()
                .map(Launched::Test)
                .map_err(|e| (CleanupStatus::Failed, e.to_string()));
        }
        let exe: Vec<_> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
        // All parameters are validated hex identifiers, fixed enums, or unsigned numbers.
        let parameters = wide(&format!("--cleanup-helper {}", arguments.join(" ")));
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
                .ok()
                .map_err(|e| (CleanupStatus::Failed, e.to_string()))?;
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
                lpVerb: w!("runas"),
                lpFile: PCWSTR(exe.as_ptr()),
                lpParameters: PCWSTR(parameters.as_ptr()),
                nShow: SW_HIDE.0,
                ..Default::default()
            };
            let result = ShellExecuteExW(&mut info);
            let error = GetLastError();
            CoUninitialize();
            if let Err(e) = result {
                return Err((
                    if error == ERROR_CANCELLED {
                        CleanupStatus::Cancelled
                    } else {
                        CleanupStatus::Failed
                    },
                    if error == ERROR_CANCELLED {
                        "Administrator approval was cancelled".into()
                    } else {
                        e.to_string()
                    },
                ));
            }
            if info.hProcess.is_invalid() {
                return Err((
                    CleanupStatus::Unknown,
                    "Windows did not return the helper process handle".into(),
                ));
            }
            Ok(Launched::Elevated(Handle(info.hProcess)))
        }
    }

    pub fn request(
        id: &str,
        regions: &[CleanupAction],
        no_op: bool,
        on_unknown: impl Fn(CleanupReport),
    ) -> CleanupReport {
        let fallback = |status, message| cleanup::empty_report(id, regions, status, message);
        if let Err(e) = cleanup::validate_regions(regions, false) {
            return fallback(CleanupStatus::Failed, e);
        }
        let prepare = || -> Result<(Handle, Vec<String>), String> {
            let mut random = [0u8; 16];
            unsafe {
                BCryptGenRandom(None, &mut random, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
                    .ok()
                    .map_err(|e| e.to_string())?;
            }
            let nonce = random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let pipe = server(&nonce)?;
            let mut args = vec![
                id.into(),
                cleanup::encode_regions(regions),
                nonce,
                unsafe { GetCurrentProcessId() }.to_string(),
                process_created(unsafe { GetCurrentProcess() })?.to_string(),
                "v2".into(),
            ];
            if no_op {
                args.push("--noop".into());
            }
            Ok((pipe, args))
        };
        let (pipe, args) = match prepare() {
            Ok(p) => p,
            Err(e) => return fallback(CleanupStatus::Failed, e),
        };
        let child = match launch(&args, no_op) {
            Ok(child) => child,
            Err((status, e)) => return fallback(status, e),
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        let exchange = || -> Result<CleanupReport, String> {
            unsafe {
                loop {
                    match ConnectNamedPipe(pipe.0, None) {
                        Ok(()) => break,
                        Err(e) => {
                            let code = GetLastError();
                            if code == ERROR_PIPE_CONNECTED {
                                break;
                            } else if code != ERROR_PIPE_LISTENING {
                                return Err(e.to_string());
                            }
                        }
                    }
                    if Instant::now() >= deadline {
                        return Err("Helper connection timed out; result unknown".into());
                    }
                    if WaitForSingleObject(child.handle(), 0) == WAIT_OBJECT_0 {
                        return Err("Helper exited without a result".into());
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                let mut client = 0;
                GetNamedPipeClientProcessId(pipe.0, &mut client).map_err(|e| e.to_string())?;
                if client != GetProcessId(child.handle()) {
                    return Err("Unexpected helper process".into());
                }
                write_bytes(pipe.0, b"1")?;
                let length = read_bytes(pipe.0, 4, deadline)?;
                let length =
                    u32::from_le_bytes(length.try_into().map_err(|_| "Invalid message size")?)
                        as usize;
                let bytes = read_bytes(pipe.0, length, deadline)?;
                let report = validate_report(&bytes, id, regions)?;
                write_bytes(pipe.0, b"2")?;
                Ok(report)
            }
        };
        let report = match exchange() {
            Ok(report) => report,
            Err(e) => fallback(CleanupStatus::Unknown, e),
        };
        // A timed-out operation may still be running; retain the concurrency guard until the child exits.
        if unsafe { WaitForSingleObject(child.handle(), 0) } != WAIT_OBJECT_0 {
            if report.status == CleanupStatus::Unknown {
                on_unknown(report.clone());
            }
            while unsafe { WaitForSingleObject(child.handle(), 1000) } != WAIT_OBJECT_0 {}
        }
        report
    }

    pub fn helper(args: HelperArguments) -> Result<(), String> {
        unsafe {
            let parent = Handle(
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, args.parent_pid)
                    .map_err(|e| e.to_string())?,
            );
            if process_created(parent.0)? != args.parent_created {
                return Err("Parent identity changed".into());
            }
            let mut image = [0u16; 32768];
            let mut length = image.len() as u32;
            QueryFullProcessImageNameW(
                parent.0,
                PROCESS_NAME_WIN32,
                PWSTR(image.as_mut_ptr()),
                &mut length,
            )
            .map_err(|e| e.to_string())?;
            let image =
                std::path::PathBuf::from(String::from_utf16_lossy(&image[..length as usize]));
            if std::fs::canonicalize(image).map_err(|e| e.to_string())?
                != std::fs::canonicalize(std::env::current_exe().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?
            {
                return Err("Parent executable differs".into());
            }
            let name = pipe_name(&args.nonce);
            let pipe = Handle(
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    None,
                )
                .map_err(|e| e.to_string())?,
            );
            let mut server_pid = 0;
            GetNamedPipeServerProcessId(pipe.0, &mut server_pid).map_err(|e| e.to_string())?;
            if server_pid != args.parent_pid {
                return Err("Unexpected result server".into());
            }
            let ready = read_bytes(pipe.0, 1, Instant::now() + Duration::from_secs(10))?;
            if ready != b"1" {
                return Err("Invalid helper handshake".into());
            }
            let report = cleanup::perform(&args.id, &args.regions, args.no_op);
            let bytes = serde_json::to_vec(&report).map_err(|e| e.to_string())?;
            if bytes.len() > MAX_MESSAGE {
                return Err("Helper result exceeds size limit".into());
            }
            let mut packet = (bytes.len() as u32).to_le_bytes().to_vec();
            packet.extend(bytes);
            write_bytes(pipe.0, &packet)?;
            // Keep the pipe alive until the parent acknowledges consumption.
            if read_bytes(pipe.0, 1, Instant::now() + Duration::from_secs(10))? != b"2" {
                return Err("Invalid result acknowledgement".into());
            }
            Ok(())
        }
    }

    pub fn validate_report(
        bytes: &[u8],
        id: &str,
        regions: &[CleanupAction],
    ) -> Result<CleanupReport, String> {
        if bytes.len() > MAX_MESSAGE {
            return Err("Helper result exceeds size limit".into());
        }
        let report: CleanupReport = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if report.id != id
            || report.regions != regions
            || report.results.len() > regions.len()
            || (matches!(
                report.status,
                CleanupStatus::Completed | CleanupStatus::Partial
            ) && report.results.len() != regions.len())
            || report.results.iter().enumerate().any(|(i, r)| {
                r.region != regions[i]
                    || r.duration_ms > MAX_SAFE_INTEGER
                    || r.error_code.as_ref().is_some_and(|s| s.len() > 2048)
            })
            || !report.observations.is_empty()
            || report.message.len() > 2048
            || report.error_code.as_ref().is_some_and(|s| s.len() > 128)
            || report.started_at_ms > MAX_SAFE_INTEGER
            || report.duration_ms.is_some_and(|n| n > MAX_SAFE_INTEGER)
        {
            return Err("Helper result does not match request".into());
        }
        for snapshot in [&report.before, &report.after].into_iter().flatten() {
            if snapshot.sequence > MAX_SAFE_INTEGER
                || snapshot.timestamp_ms > MAX_SAFE_INTEGER
                || snapshot.elapsed_ms > MAX_SAFE_INTEGER
            {
                return Err("Invalid helper timestamp".into());
            }
            if let Some(m) = &snapshot.metrics {
                m.validate()?;
            }
        }
        Ok(report)
    }
}

#[cfg(windows)]
pub use platform::{helper, request};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_helper_parser() {
        let args = [
            "aabbccdd-1122-3344-5566-778899001122",
            "2",
            "11223344556677889900112233445566",
            "123",
            "456",
            "v2",
        ]
        .map(str::to_owned);
        assert!(parse_helper(&args).is_ok());
        let mut bad = args.clone();
        bad[1] = "anything".into();
        assert!(parse_helper(&bad).is_err());
        let mut bad = args.clone();
        bad[2] = "../arbitrary-pipe".into();
        assert!(parse_helper(&bad).is_err());
        let mut bad = args.to_vec();
        bad.push("--other".into());
        assert!(parse_helper(&bad).is_err());
    }
    #[test]
    #[cfg(windows)]
    fn rejects_mismatched_and_oversized_reports() {
        let report = cleanup::empty_report(
            "aabbccdd-1122-3344-5566-778899001122",
            &[CleanupAction::LowPriorityStandby],
            CleanupStatus::Failed,
            "Test".into(),
        );
        let bytes = serde_json::to_vec(&report).unwrap();
        assert!(platform::validate_report(
            &bytes,
            &report.id,
            &[CleanupAction::LowPriorityStandby]
        )
        .is_ok());
        assert!(platform::validate_report(
            &bytes,
            "different",
            &[CleanupAction::LowPriorityStandby]
        )
        .is_err());
        assert!(platform::validate_report(
            &vec![0; 20000],
            &report.id,
            &[CleanupAction::LowPriorityStandby]
        )
        .is_err());
    }
}
