#[cfg(windows)]
fn command(executable: &std::path::Path) -> Result<Vec<u16>, String> {
    use std::os::windows::ffi::OsStrExt;

    let command: Vec<_> = std::iter::once(b'"' as u16)
        .chain(executable.as_os_str().encode_wide())
        .chain([b'"' as u16, 0])
        .collect();
    if command.len() > 261 {
        return Err("The application path is too long for Windows startup registration".into());
    }
    Ok(command)
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::{
            core::w,
            Win32::{
                Foundation::ERROR_FILE_NOT_FOUND,
                System::Registry::{
                    RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
                },
            },
        };

        let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        // The NSIS uninstaller removes the Run value named after the product.
        let name = w!("OpenMemDuct");
        let status = if enabled {
            let executable = std::env::current_exe().map_err(|e| e.to_string())?;
            let command = command(&executable)?;
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key,
                    name,
                    REG_SZ.0,
                    Some(command.as_ptr().cast()),
                    (command.len() * size_of::<u16>()) as u32,
                )
            }
        } else {
            let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key, name) };
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            status
        };
        status
            .ok()
            .map_err(|e| format!("Could not change Start with Windows: {e}"))
    }
    #[cfg(not(windows))]
    {
        if enabled {
            Err("Start with Windows is only available on Windows".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn quotes_unicode_paths_and_rejects_commands_beyond_the_run_key_limit() {
        let path = Path::new(r"C:\Program Files\Mémory\openmemduct.exe");
        let command = command(path).unwrap();
        assert_eq!(command.last(), Some(&0));
        assert_eq!(
            String::from_utf16(&command[..command.len() - 1]).unwrap(),
            format!("\"{}\"", path.display())
        );
        assert!(super::command(Path::new(&"a".repeat(258))).is_ok());
        assert!(super::command(Path::new(&"a".repeat(259))).is_err());
    }
}
