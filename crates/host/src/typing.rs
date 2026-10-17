//! Typing on the player's own keyboard (the bridge's `TypingBlock`): the page's field selection passed to the
//! game plugin, and what the plugin's keys typed passed to the page, with Ctrl+V's paste filled in from the
//! PC's clipboard (the plugin, in the game, doesn't read it).

use westplay_shared::{TypedKey, TypingState};
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// The most text a paste brings (the cabin's fields are a line long).
const PASTE_MOST: usize = 500;

/// `{"type":"typed","events":[{"text":"ab"},{"key":"backspace"},...]}`: what the plugin's keys typed (`codes`,
/// oldest first: characters run together, keys by name, a paste as the text `paste` gives), `None` for nothing.
pub fn typed_message(codes: &[u32], paste: impl Fn() -> Option<String>) -> Option<String> {
    let mut events: Vec<serde_json::Value> = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, events: &mut Vec<serde_json::Value>| {
        if !text.is_empty() {
            events.push(serde_json::json!({ "text": std::mem::take(text) }));
        }
    };
    for &code in codes {
        if let Some(key) = TypedKey::from_code(code) {
            flush(&mut text, &mut events);
            match key {
                TypedKey::Paste => {
                    if let Some(pasted) = paste().map(|pasted| one_line(&pasted)).filter(|pasted| !pasted.is_empty()) {
                        events.push(serde_json::json!({ "text": pasted }));
                    }
                }
                TypedKey::Backspace => events.push(serde_json::json!({ "key": "backspace" })),
                TypedKey::WordBackspace => events.push(serde_json::json!({ "key": "wordBackspace" })),
                TypedKey::Enter => events.push(serde_json::json!({ "key": "enter" })),
                TypedKey::Escape => events.push(serde_json::json!({ "key": "escape" })),
            }
        } else if let Some(character) = char::from_u32(code) {
            text.push(character);
        }
    }
    flush(&mut text, &mut events);
    (!events.is_empty()).then(|| serde_json::json!({ "type": "typed", "events": events }).to_string())
}

/// `{"type":"typingState","state":"typing"}`: whether the player's keyboard types into the field selected
/// (`off`: none selected; `paused`: selected, but the keys are the game's).
pub fn state_message(state: TypingState) -> String {
    let state = match state {
        TypingState::Off => "off",
        TypingState::Typing => "typing",
        TypingState::Paused => "paused",
    };
    serde_json::json!({ "type": "typingState", "state": state }).to_string()
}

/// Pasted text as one line: line breaks and tabs as spaces, at most [`PASTE_MOST`] characters, no other
/// control characters.
fn one_line(text: &str) -> String {
    let line: String = text
        .chars()
        .map(|c| if c == '\r' || c == '\n' || c == '\t' { ' ' } else { c })
        .filter(|c| !c.is_control())
        .take(PASTE_MOST)
        .collect();
    line.trim().to_owned()
}

/// The text on the PC's clipboard, if any.
pub fn clipboard_text() -> Option<String> {
    // SAFETY: the clipboard is opened and closed on this thread around its data, which is read only while
    // locked and only within the block's own size (its text ends at a nul, but another program's need not).
    unsafe {
        OpenClipboard(None).ok()?;
        let text = (|| {
            let HANDLE(data) = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            let memory = HGLOBAL(data);
            let start = GlobalLock(memory) as *const u16;
            if start.is_null() {
                return None;
            }
            let units = GlobalSize(memory) / size_of::<u16>();
            let length = (0..units).take_while(|&i| *start.add(i) != 0).count();
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(start, length));
            let _ = GlobalUnlock(memory);
            Some(text)
        })();
        let _ = CloseClipboard();
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }

    #[test]
    fn passes_what_was_typed_to_the_page() {
        let mut typed = codes("Kok");
        typed.push(TypedKey::Backspace as u32);
        typed.extend(codes("kola"));
        typed.push(TypedKey::Enter as u32);
        let message = typed_message(&typed, || None).unwrap();
        assert_eq!(
            message,
            r#"{"events":[{"text":"Kok"},{"key":"backspace"},{"text":"kola"},{"key":"enter"}],"type":"typed"}"#
        );
        assert_eq!(typed_message(&[], || None), None);
    }

    #[test]
    fn pastes_the_clipboard_as_one_line() {
        let paste = [TypedKey::Paste as u32];
        let message = typed_message(&paste, || Some("  Never Gonna\r\nGive\tYou Up \n".into())).unwrap();
        assert_eq!(message, r#"{"events":[{"text":"Never Gonna  Give You Up"}],"type":"typed"}"#);
        // Nothing on the clipboard: nothing typed.
        assert_eq!(typed_message(&paste, || None), None);
        assert_eq!(one_line(&"x".repeat(900)).len(), PASTE_MOST);
    }

    #[test]
    fn says_whether_the_keyboard_types() {
        assert_eq!(state_message(TypingState::Paused), r#"{"state":"paused","type":"typingState"}"#);
    }
}
