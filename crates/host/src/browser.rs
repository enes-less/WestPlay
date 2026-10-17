//! Web pages opened in the player's own browser, outside WestPlay.

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

/// Opens `url` in the player's default browser. Only for addresses WestPlay itself names (each of the app's
/// commands opens one fixed page), never one a page asks for.
pub fn open(url: &str) {
    let address = HSTRING::from(url);
    // SAFETY: the operation and the address are null-terminated wide strings that outlive the call; the
    // parameters and the folder are null (none), and no window owns it.
    let result = unsafe { ShellExecuteW(None, w!("open"), &address, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    // ShellExecute's convention: above 32 is success, the rest an error code.
    let code = result.0 as usize;
    if code <= 32 {
        log!("[browser] {url} didn't open (ShellExecute {code})");
    }
}
