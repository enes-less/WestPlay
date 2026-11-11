//! A native crash in words, for the logs: what it was and whose code it was in. A crash that isn't a Rust panic
//! (a bad memory access, a stack overflow, a fault inside another DLL) ends WestPlay, or the game, at once,
//! without a line: players' reports said only "it closes" (2026-11-06: the companion closing as Install is
//! pressed, the game closing as it starts). The app's handler (`westplay_host::log`) and the plugin's
//! (`westplay::log`) write [`describe`]'s line straight to their log file.
//!
//! It runs as the process is failing, so it takes no locks and no memory: the line is made on the stack
//! ([`Line`]) and written by the caller with one `WriteFile`.

use core::fmt::{self, Write};

use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_RECORD, RtlCaptureStackBackTrace};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW,
    GetModuleHandleExW,
};
use windows::core::PCWSTR;

/// The crashes worth a line, by their codes: those that end a process when nothing catches them. A C++
/// exception (0xE06D7363) is left out: games throw and catch those as they run.
pub const FATAL: [u32; 8] = [
    0xC000_0005, // access violation
    0xC000_001D, // illegal instruction
    0xC000_0096, // privileged instruction
    0xC000_00FD, // stack overflow
    0xC000_0094, // integer divide by zero
    0xC000_0095, // integer overflow
    0xC000_0374, // heap corruption
    0xC000_0006, // in-page error (a file mapped into memory couldn't be read)
];

/// Whether `code` is one of the [`FATAL`] crashes.
pub fn is_fatal(code: u32) -> bool {
    FATAL.contains(&code)
}

/// A crash code's name.
pub fn name(code: u32) -> &'static str {
    match code {
        0xC000_0005 => "access violation",
        0xC000_001D => "illegal instruction",
        0xC000_0096 => "privileged instruction",
        0xC000_00FD => "stack overflow",
        0xC000_0094 => "integer divide by zero",
        0xC000_0095 => "integer overflow",
        0xC000_0374 => "heap corruption",
        0xC000_0006 => "in-page error",
        0xC000_0409 => "fail-fast (an abort)",
        _ => "exception",
    }
}

/// A line made on the stack: what doesn't fit is cut off.
pub struct Line {
    bytes: [u8; 768],
    len: usize,
}

impl Line {
    pub fn new() -> Self {
        Self { bytes: [0; 768], len: 0 }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for Line {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let room = self.bytes.len() - self.len;
        // Cut on a character's edge.
        let mut take = text.len().min(room);
        while !text.is_char_boundary(take) {
            take -= 1;
        }
        self.bytes[self.len..self.len + take].copy_from_slice(&text.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

/// Writes what `record` is onto `line`: its name and code, the module its address is in and where in it, and for
/// a bad memory access, what it did at which address (`access violation (0xC0000005) at C:\…\westplay.dll+0x1a2b3,
/// reading 0x0000000000000010`).
pub fn describe(record: &EXCEPTION_RECORD, line: &mut Line) {
    let code = record.ExceptionCode.0 as u32;
    let address = record.ExceptionAddress as usize;
    let _ = write!(line, "{} ({code:#010X}) at ", name(code));
    write_place(address, line);
    if code == 0xC000_0005 && record.NumberParameters >= 2 {
        let what = match record.ExceptionInformation[0] {
            0 => "reading",
            1 => "writing",
            8 => "running code at",
            _ => "touching",
        };
        let _ = write!(line, ", {what} {:#018x}", record.ExceptionInformation[1]);
    }
}

/// How many calls [`callers`] reads at the most (the handler's own among them).
pub const CALLERS: usize = 62;

/// The calls the thread was in when it raised an exception at `fault`, the innermost first (their return
/// addresses), read by a handler running on that thread as it handles it (the plugin's vectored handler, an
/// unhandled-exception filter): Windows' own walk of the stack, taking no memory, from the handler through the
/// exception's dispatch into the calls that led to the fault. The handler's own calls, up to the fault, are left
/// out; all are kept if the fault isn't among them. A crash place alone can't say whose call it was (a player's
/// game, 2026-11-07, crashing in ntdll as a career loaded: called by the game, or by the plugin?).
pub fn callers(fault: usize, frames: &mut [*mut core::ffi::c_void; CALLERS]) -> &[*mut core::ffi::c_void] {
    // SAFETY: the frames' array, as long as it says.
    let count = unsafe { RtlCaptureStackBackTrace(0, frames, None) } as usize;
    let all = &frames[..count.min(CALLERS)];
    match all.iter().position(|frame| *frame as usize == fault) {
        Some(at) => &all[at + 1..],
        None => all,
    }
}

/// Writes `frames` onto `line`, ` <- ` between them, each as its module's file name and the offset in it.
pub fn write_callers(frames: &[*mut core::ffi::c_void], line: &mut Line) {
    for (at, frame) in frames.iter().enumerate() {
        if at > 0 {
            let _ = line.write_str(" <- ");
        }
        write_place_named(*frame as usize, line, true);
    }
}

/// `module+0xoffset` for `address`, or the bare address if it's in no module.
fn write_place(address: usize, line: &mut Line) {
    write_place_named(address, line, false);
}

/// [`write_place`], the module by its path, or (`short`) its file's name alone.
fn write_place_named(address: usize, line: &mut Line, short: bool) {
    let mut module = HMODULE::default();
    let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
    // SAFETY: `address` is only looked up, never followed; `module` and `path` are locals.
    unsafe {
        if GetModuleHandleExW(flags, PCWSTR(address as *const u16), &mut module).is_err() {
            let _ = write!(line, "{address:#x} (in no module)");
            return;
        }
        let mut path = [0u16; 260];
        let length = GetModuleFileNameW(Some(module), &mut path) as usize;
        let name = if short {
            path[..length].iter().rposition(|&c| c == u16::from(b'\\') || c == u16::from(b'/')).map_or(0, |at| at + 1)
        } else {
            0
        };
        for character in char::decode_utf16(path[name..length].iter().copied()) {
            let _ = line.write_char(character.unwrap_or(char::REPLACEMENT_CHARACTER));
        }
        let _ = write!(line, "+{:#x}", address.wrapping_sub(module.0 as usize));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::NTSTATUS;

    #[test]
    fn says_what_a_bad_read_was_and_whose_code_it_was_in() {
        let mut record = EXCEPTION_RECORD {
            ExceptionCode: NTSTATUS(0xC000_0005_u32 as i32),
            ExceptionAddress: describe as *mut core::ffi::c_void,
            NumberParameters: 2,
            ..Default::default()
        };
        record.ExceptionInformation[0] = 0;
        record.ExceptionInformation[1] = 0x10;
        let mut line = Line::new();
        describe(&record, &mut line);
        let said = core::str::from_utf8(line.as_bytes()).unwrap();
        assert!(said.starts_with("access violation (0xC0000005) at "), "{said}");
        // This test's own executable, and where in it.
        assert!(said.contains(".exe+0x"), "{said}");
        assert!(said.ends_with(", reading 0x0000000000000010"), "{said}");
    }

    /// The calls leading to a place: this test's among them, by the file name of the executable it's in.
    #[test]
    #[inline(never)]
    fn tells_the_calls_that_led_somewhere() {
        let mut frames = [core::ptr::null_mut(); CALLERS];
        // A place not among them: all kept.
        let found = callers(0x10, &mut frames);
        assert!(found.len() > 2, "{}", found.len());
        let mut line = Line::new();
        write_callers(found, &mut line);
        let said = core::str::from_utf8(line.as_bytes()).unwrap();
        assert!(said.contains(" <- ") && said.contains(".exe+0x"), "{said}");
        assert!(!said.contains('\\'), "file names alone: {said}");
        // The first of them as the fault: the ones after it.
        let mut again = [core::ptr::null_mut(); CALLERS];
        let first = callers(0x10, &mut again)[0] as usize;
        let mut after = [core::ptr::null_mut(); CALLERS];
        let later = callers(first, &mut after);
        assert!(later.len() < found.len());
    }

    #[test]
    fn an_address_in_no_module_is_said_as_it_is() {
        let mut line = Line::new();
        write_place(0x10, &mut line);
        assert_eq!(core::str::from_utf8(line.as_bytes()).unwrap(), "0x10 (in no module)");
    }

    #[test]
    fn a_long_line_is_cut_on_a_characters_edge() {
        let mut line = Line::new();
        for _ in 0..400 {
            let _ = line.write_str("ğ");
        }
        assert_eq!(line.as_bytes().len(), 768);
        assert!(core::str::from_utf8(line.as_bytes()).is_ok());
    }

    #[test]
    fn only_crashes_that_end_a_process_are_fatal() {
        assert!(is_fatal(0xC000_0005) && is_fatal(0xC000_00FD));
        assert!(!is_fatal(0xE06D_7363), "a C++ exception, thrown and caught as games run");
        assert!(!is_fatal(0x8000_0003), "a breakpoint");
    }
}
