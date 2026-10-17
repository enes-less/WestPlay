//! Where the game's camera is, for YouTube's sound to come from the cab as far off as the camera is (the
//! SDK doesn't say, and the GPU only gets positions relative to the camera).
//!
//! The game keeps it in a static of its executable: three 32-bit floats, the camera's world position (the
//! one it draws the world around). It's found by the game's own code around it, so it holds across updates
//! that move it: the code that writes it each frame (a position copied from the stack, x and y as one
//! eight-byte write, z as a four-byte one) and the code that reads it to make positions relative to the
//! camera (three subtractions). Both must be there, exactly once, and agree; otherwise the camera isn't
//! known. Found 2026-10-15 in 1.61 (at +0x36921d8), probed by reading the game's memory.

use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::System::LibraryLoader::GetModuleHandleW;

use crate::log;
use crate::navigation::Image;

static POSITION: AtomicUsize = AtomicUsize::new(0);

/// `movsd xmm0, [rsp+70h]; mov eax, [rsp+78h]; movsd [rip+X], xmm0; mov [rip+X+8], eax` (`None`: any byte).
const WRITER: &[Option<u8>] = &[
    Some(0xf2),
    Some(0x0f),
    Some(0x10),
    Some(0x44),
    Some(0x24),
    Some(0x70),
    Some(0x8b),
    Some(0x44),
    Some(0x24),
    Some(0x78),
    Some(0xf2),
    Some(0x0f),
    Some(0x11),
    Some(0x05),
    None,
    None,
    None,
    None,
    Some(0x89),
    Some(0x05),
    None,
    None,
    None,
    None,
];

/// `subss xmm7, [rip+X]; mulss xmm0, xmm6; subss xmm1, [rip+X+4]; subss xmm0, [rip+X+8]`.
const READER: &[Option<u8>] = &[
    Some(0xf3),
    Some(0x0f),
    Some(0x5c),
    Some(0x3d),
    None,
    None,
    None,
    None,
    Some(0xf3),
    Some(0x0f),
    Some(0x59),
    Some(0xc6),
    Some(0xf3),
    Some(0x0f),
    Some(0x5c),
    Some(0x0d),
    None,
    None,
    None,
    None,
    Some(0xf3),
    Some(0x0f),
    Some(0x5c),
    Some(0x05),
    None,
    None,
    None,
    None,
];

/// Finds the camera's position in the running game; logs whether it did.
///
/// # Safety
/// In the game's process (the plugin is loaded in it).
pub unsafe fn locate() {
    let found = (|| {
        // SAFETY: no name: the game's own executable, mapped for the process's life.
        let base = unsafe { GetModuleHandleW(None) }.ok()?.0 as usize;
        // SAFETY: `base` is the game's executable, as mapped.
        let image = unsafe { Image::of(base) }?;
        let writer = image.unique_masked(WRITER)?;
        let reader = image.unique_masked(READER)?;
        // Each RIP-relative address: the displacement at `at`, from the end of its instruction.
        // SAFETY: `at` is within an instruction the patterns matched, in the game's code.
        let target =
            |at: usize| (at + 4).wrapping_add_signed(i32::from_le_bytes(unsafe { *(at as *const [u8; 4]) }) as isize);
        let position = target(writer + 14);
        let agree = target(writer + 20) == position + 8
            && target(reader + 4) == position
            && target(reader + 16) == position + 4
            && target(reader + 24) == position + 8;
        agree.then(|| {
            log!("camera: found at +{:#x}", position - base);
            position
        })
    })();
    if found.is_none() {
        log!("camera: the game's camera code isn't the version WestPlay knows; the camera isn't known");
    }
    POSITION.store(found.unwrap_or(0), Ordering::Release);
}

/// The camera's world position now (metres), if it's known.
pub fn position() -> Option<[f32; 3]> {
    let at = POSITION.load(Ordering::Acquire);
    // A static of the game's executable: there for as long as the game is.
    // SAFETY: a static of the game's executable (found by `locate`), mapped as long as the game runs; read as
    // it lies.
    (at != 0).then(|| unsafe { (at as *const [f32; 3]).read_unaligned() })
}
