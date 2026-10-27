use crate::{cleanup, models::*};

pub struct HelperArguments {
    pub nonce: String,
    pub parent_pid: u32,
    pub parent_created: u64,
    pub no_op: bool,
}

pub fn parse_helper(args: &[String]) -> Result<HelperArguments, String> {
    if args.len() != 4 && args.len() != 5 {
        return Err("Invalid helper arguments".into());
    }
    if args[0].len() != 32 || !args[0].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid helper identifier".into());
    }
    let parent_pid = args[1]
        .parse::<u32>()
        .map_err(|_| "Invalid parent process")?;
    let parent_created = args[2]
        .parse::<u64>()
        .map_err(|_| "Invalid parent identity")?;
    if parent_pid == 0 || parent_created == 0 || args[3] != "v3" {
        return Err("Invalid helper identity or protocol".into());
    }
    let no_op = args.len() == 5 && args[4] == "--noop" && cfg!(debug_assertions);
    if args.len() == 5 && !no_op {
        return Err("Test mode is unavailable".into());
    }
    Ok(HelperArguments {
        nonce: args[0].clone(),
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
                WAIT_TIMEOUT,
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
                    PROCESS_SYNCHRONIZE,
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

    fn read_bytes(
        pipe: HANDLE,
        size: usize,
        deadline: Option<Instant>,
        parent: Option<HANDLE>,
    ) -> Result<Vec<u8>, String> {
        if size > MAX_MESSAGE {
            return Err("Helper message is too large".into());
        }
        let mut bytes = vec![0; size];
        let mut offset = 0;
        while offset < size {
            if parent.is_some_and(|p| unsafe { WaitForSingleObject(p, 0) } != WAIT_TIMEOUT) {
                return Err("Parent process exited or could not be checked".into());
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
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

    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        id: String,
        regions: String,
    }

    fn validate_request(id: &str, regions: &[CleanupAction]) -> Result<(), String> {
        if !cleanup::valid_request_id(id) {
            return Err("Invalid cleanup request identifier".into());
        }
        cleanup::validate_regions(regions, false)
    }

    pub fn parse_request(bytes: &[u8]) -> Result<(String, Vec<CleanupAction>), String> {
        if bytes.len() > MAX_MESSAGE {
            return Err("Helper request exceeds size limit".into());
        }
        let request: Request = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let regions = cleanup::parse_regions(&request.regions)?;
        validate_request(&request.id, &regions)?;
        Ok((request.id, regions))
    }

    fn write_packet(pipe: HANDLE, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > MAX_MESSAGE {
            return Err("Invalid helper message size".into());
        }
        let mut packet = (bytes.len() as u32).to_le_bytes().to_vec();
        packet.extend(bytes);
        write_bytes(pipe, &packet)
    }

    fn read_packet(
        pipe: HANDLE,
        deadline: Option<Instant>,
        parent: Option<HANDLE>,
    ) -> Result<Vec<u8>, String> {
        // Only the idle first byte may wait indefinitely; partial frames have a deadline.
        let mut length = read_bytes(pipe, 1, deadline, parent)?;
        let deadline = deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(30));
        length.extend(read_bytes(pipe, 3, Some(deadline), parent)?);
        let length = u32::from_le_bytes(length.try_into().map_err(|_| "Invalid message size")?);
        if length == 0 {
            return Err("Empty helper message".into());
        }
        read_bytes(pipe, length as usize, Some(deadline), parent)
    }

    struct Connection {
        pipe: Option<Handle>,
        child: Launched,
        no_op: bool,
        connected: bool,
    }

    impl Drop for Connection {
        fn drop(&mut self) {
            // Disconnect first so an idle helper exits; an outstanding operation must finish.
            self.pipe.take();
            while unsafe { WaitForSingleObject(self.child.handle(), 1000) } != WAIT_OBJECT_0 {}
        }
    }

    #[derive(Default)]
    pub struct Session {
        connection: Option<Connection>,
    }

    impl Session {
        fn launch(&mut self, no_op: bool) -> Result<(), (CleanupStatus, String)> {
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
                    nonce,
                    unsafe { GetCurrentProcessId() }.to_string(),
                    process_created(unsafe { GetCurrentProcess() })?.to_string(),
                    "v3".into(),
                ];
                if no_op {
                    args.push("--noop".into());
                }
                Ok((pipe, args))
            };
            let (pipe, args) = prepare().map_err(|e| (CleanupStatus::Failed, e))?;
            let child = launch(&args, no_op)?;
            self.connection = Some(Connection {
                pipe: Some(pipe),
                child,
                no_op,
                connected: false,
            });
            Ok(())
        }

        pub fn request(
            &mut self,
            id: &str,
            regions: &[CleanupAction],
            no_op: bool,
            on_unknown: impl Fn(CleanupReport),
        ) -> CleanupReport {
            let fallback = |status, message| cleanup::empty_report(id, regions, status, message);
            if let Err(e) = validate_request(id, regions) {
                return fallback(CleanupStatus::Failed, e);
            }
            if no_op && !cfg!(debug_assertions) {
                return fallback(CleanupStatus::Failed, "Test mode is unavailable".into());
            }
            if self.connection.as_ref().is_some_and(|c| {
                (unsafe { WaitForSingleObject(c.child.handle(), 0) }) == WAIT_OBJECT_0
            }) {
                self.connection.take();
            }
            if self.connection.as_ref().is_some_and(|c| c.no_op != no_op) {
                return fallback(CleanupStatus::Failed, "Helper session mode differs".into());
            }
            if self.connection.is_none() {
                if let Err((status, e)) = self.launch(no_op) {
                    return fallback(status, e);
                }
            }
            let connection = self.connection.as_mut().unwrap();
            let pipe = connection.pipe.as_ref().unwrap().0;
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut exchange = || -> Result<CleanupReport, String> {
                unsafe {
                    if !connection.connected {
                        loop {
                            match ConnectNamedPipe(pipe, None) {
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
                            if WaitForSingleObject(connection.child.handle(), 0) == WAIT_OBJECT_0 {
                                return Err("Helper exited without a result".into());
                            }
                            std::thread::sleep(Duration::from_millis(25));
                        }
                    }
                    let mut client = 0;
                    GetNamedPipeClientProcessId(pipe, &mut client).map_err(|e| e.to_string())?;
                    if client != GetProcessId(connection.child.handle()) {
                        return Err("Unexpected helper process".into());
                    }
                    if !connection.connected {
                        write_bytes(pipe, b"1")?;
                        connection.connected = true;
                    }
                }
                let bytes = serde_json::to_vec(&Request {
                    id: id.into(),
                    regions: cleanup::encode_regions(regions),
                })
                .map_err(|e| e.to_string())?;
                write_packet(pipe, &bytes)?;
                let bytes = read_packet(pipe, Some(deadline), None)?;
                let report = validate_report(&bytes, id, regions)?;
                write_bytes(pipe, b"2")?;
                Ok(report)
            };
            match exchange() {
                Ok(report) => report,
                Err(e) => {
                    let report = fallback(CleanupStatus::Unknown, e);
                    let mut connection = self.connection.take().unwrap();
                    connection.pipe.take();
                    on_unknown(report.clone());
                    // Drop waits for completion before the caller can release its busy guard.
                    drop(connection);
                    report
                }
            }
        }
    }

    pub fn self_test() -> Result<(), String> {
        if !cfg!(debug_assertions) {
            return Err("Test mode is unavailable".into());
        }
        let mut session = Session::default();
        let first = session.request(
            "aabbccdd-1122-3344-5566-778899001122",
            &[
                CleanupAction::WorkingSet,
                CleanupAction::SystemFileCache,
                CleanupAction::ModifiedPageList,
                CleanupAction::FullStandby,
                CleanupAction::LowPriorityStandby,
                CleanupAction::ModifiedFileCache,
                CleanupAction::RegistryCache,
                CleanupAction::CombineMemory,
            ],
            true,
            |_| {},
        );
        let first_pid = session
            .connection
            .as_ref()
            .map(|c| unsafe { GetProcessId(c.child.handle()) });
        let second = session.request(
            "aabbccdd-1122-3344-5566-778899001123",
            &[CleanupAction::LowPriorityStandby, CleanupAction::WorkingSet],
            true,
            |_| {},
        );
        let second_pid = session
            .connection
            .as_ref()
            .map(|c| unsafe { GetProcessId(c.child.handle()) });
        let reused = first_pid.is_some_and(|pid| pid != 0) && first_pid == second_pid;
        let exited = session.connection.take().is_some_and(|mut c| {
            let alive = unsafe { WaitForSingleObject(c.child.handle(), 0) } == WAIT_TIMEOUT;
            c.pipe.take();
            alive && (unsafe { WaitForSingleObject(c.child.handle(), 5000) }) == WAIT_OBJECT_0
        });
        let passed = reused
            && exited
            && first.status == CleanupStatus::Completed
            && second.status == CleanupStatus::Completed;
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": if passed { "completed" } else { "failed" },
                "reports": [first, second],
                "helperPids": [first_pid, second_pid],
                "helperReused": reused,
                "helperExitedOnDisconnect": exited,
            }))
            .map_err(|e| e.to_string())?
        );
        if passed {
            Ok(())
        } else {
            Err("Helper session self-test failed".into())
        }
    }

    pub fn helper(args: HelperArguments) -> Result<(), String> {
        unsafe {
            let parent = Handle(
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    false,
                    args.parent_pid,
                )
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
            let ready = read_bytes(
                pipe.0,
                1,
                Some(Instant::now() + Duration::from_secs(10)),
                Some(parent.0),
            )?;
            if ready != b"1" {
                return Err("Invalid helper handshake".into());
            }
            loop {
                let bytes = match read_packet(pipe.0, None, Some(parent.0)) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        if WaitForSingleObject(parent.0, 0) != WAIT_TIMEOUT
                            || PeekNamedPipe(pipe.0, None, 0, None, None, None).is_err()
                        {
                            return Ok(());
                        }
                        return Err(error);
                    }
                };
                // Every request uses the fixed allowlist; test mode is set only at launch.
                let (id, regions) = parse_request(&bytes)?;
                let report = cleanup::perform(&id, &regions, args.no_op);
                let bytes = serde_json::to_vec(&report).map_err(|e| e.to_string())?;
                write_packet(pipe.0, &bytes)?;
                if read_bytes(
                    pipe.0,
                    1,
                    Some(Instant::now() + Duration::from_secs(10)),
                    Some(parent.0),
                )? != b"2"
                {
                    return Err("Invalid result acknowledgement".into());
                }
            }
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn rejects_invalid_frames_and_detects_disconnect() {
            let nonce = format!("{:?}", windows::core::GUID::new().unwrap());
            let server = server(&nonce).unwrap();
            let name = pipe_name(&nonce);
            let client = Handle(unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    None,
                )
                .unwrap()
            });
            for size in [0, MAX_MESSAGE as u32 + 1, u32::MAX] {
                write_bytes(client.0, &size.to_le_bytes()).unwrap();
                assert!(read_packet(
                    server.0,
                    Some(Instant::now() + Duration::from_secs(1)),
                    None
                )
                .is_err());
            }
            assert!(write_packet(client.0, &[]).is_err());
            assert!(write_packet(client.0, &vec![0; MAX_MESSAGE + 1]).is_err());
            write_packet(client.0, b"{}").unwrap();
            let bytes = read_packet(
                server.0,
                Some(Instant::now() + Duration::from_secs(1)),
                None,
            )
            .unwrap();
            assert!(parse_request(&bytes).is_err());
            drop(client);
            assert!(read_packet(
                server.0,
                Some(Instant::now() + Duration::from_secs(1)),
                None
            )
            .is_err());
        }
    }
}

#[cfg(windows)]
pub use platform::{helper, self_test, Session};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_helper_parser() {
        let args = ["11223344556677889900112233445566", "123", "456", "v3"].map(str::to_owned);
        assert!(parse_helper(&args).is_ok());
        let mut bad = args.clone();
        bad[1] = "0".into();
        assert!(parse_helper(&bad).is_err());
        let mut bad = args.clone();
        bad[0] = "../arbitrary-pipe".into();
        assert!(parse_helper(&bad).is_err());
        let mut bad = args.to_vec();
        bad.push("--other".into());
        assert!(parse_helper(&bad).is_err());
        for (index, value) in [(1, "other"), (2, "0"), (2, "-1"), (3, "v2")] {
            let mut bad = args.clone();
            bad[index] = value.into();
            assert!(parse_helper(&bad).is_err());
        }
        let mut test = args.to_vec();
        test.push("--noop".into());
        assert_eq!(parse_helper(&test).is_ok(), cfg!(debug_assertions));
        assert!(!parse_helper(&args).unwrap().no_op);
        assert!(parse_helper(&args[..3]).is_err());
    }
    #[test]
    #[cfg(windows)]
    fn strict_session_request_parser() {
        let id = "aabbccdd-1122-3344-5566-778899001122";
        let valid = serde_json::json!({ "id": id, "regions": "6,7,0,1,2,3,4,5" });
        let bytes = serde_json::to_vec(&valid).unwrap();
        let (parsed_id, regions) = platform::parse_request(&bytes).unwrap();
        assert_eq!(parsed_id, id);
        assert_eq!(regions.len(), 8);
        for bad_regions in ["", "2,2", "8", "-1", "2,", "02", "2,0,1,2,3,4,5,6,7"] {
            let mut bad = valid.clone();
            bad["regions"] = bad_regions.into();
            assert!(platform::parse_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        for extra in ["no_op", "noOp", "nonce", "parent_pid"] {
            let mut bad = valid.clone();
            bad[extra] = true.into();
            assert!(platform::parse_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        for bad in [
            br#"{"id":"bad","regions":"2"}"#.as_slice(),
            br#"{"id":"aabbccdd-1122-3344-5566-778899001122","regions":[]}"#,
            br#"{"regions":"2"}"#,
            br#"{"id":"aabbccdd-1122-3344-5566-778899001122","regions":"2","regions":"1"}"#,
            b"{} trailing",
            &vec![b' '; 16385],
        ] {
            assert!(platform::parse_request(bad).is_err());
        }
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
