//! Recycle Bin size + empty via the shell API, backing the "Recycle Bin"
//! cleaner. Kept separate from the path-based actions because it goes through the
//! shell rather than the filesystem walk.

/// Total bytes currently in the Recycle Bin across all drives.
pub fn size() -> u64 {
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        i64Size: 0,
        i64NumItems: 0,
    };
    let ok = unsafe { SHQueryRecycleBinW(windows::core::PCWSTR::null(), &mut info) };
    if ok.is_ok() && info.i64Size > 0 {
        info.i64Size as u64
    } else {
        0
    }
}

/// Empty the Recycle Bin, no prompts or progress UI.
pub fn empty() {
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };
    unsafe {
        let _ = SHEmptyRecycleBinW(
            None,
            windows::core::PCWSTR::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        );
    }
}
