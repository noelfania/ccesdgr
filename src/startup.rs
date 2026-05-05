use std::io;
use std::mem::size_of;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ,
};

use crate::win;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "GMTL Mouse Click";

pub fn set_enabled(enabled: bool) -> io::Result<()> {
    if enabled {
        enable()
    } else {
        disable()
    }
}

fn enable() -> io::Result<()> {
    let exe_path = std::env::current_exe()?;
    let quoted_path = format!("\"{}\"", exe_path.display());
    let path_wide = win::to_wide(&quoted_path);
    let run_key = win::to_wide(RUN_KEY);
    let value_name = win::to_wide(VALUE_NAME);

    let mut key: HKEY = null_mut();
    let result = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, run_key.as_ptr(), &mut key) };

    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result as i32));
    }

    let bytes = (path_wide.len() * size_of::<u16>()) as u32;
    let result = unsafe {
        RegSetValueExW(
            key,
            value_name.as_ptr(),
            0,
            REG_SZ,
            path_wide.as_ptr().cast(),
            bytes,
        )
    };
    unsafe {
        RegCloseKey(key);
    }

    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result as i32))
    }
}

fn disable() -> io::Result<()> {
    let run_key = win::to_wide(RUN_KEY);
    let value_name = win::to_wide(VALUE_NAME);

    let mut key: HKEY = null_mut();
    let result = unsafe { RegCreateKeyW(HKEY_CURRENT_USER, run_key.as_ptr(), &mut key) };

    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result as i32));
    }

    let result = unsafe { RegDeleteValueW(key, value_name.as_ptr()) };
    unsafe {
        RegCloseKey(key);
    }

    if result == ERROR_SUCCESS || result == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result as i32))
    }
}
