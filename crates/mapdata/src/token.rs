//! The game's tokens: short lowercase names (city, company and overlay ids) packed into 64 bits, base 38.

const ALPHABET: &[u8; 38] = b"\x000123456789abcdefghijklmnopqrstuvwxyz_";

/// The name packed in `token` (`calais`, `port_overlay`), or `None` if it isn't a valid token.
pub fn token_to_string(mut token: u64) -> Option<String> {
    let mut name = String::new();
    while token != 0 {
        let digit = (token % 38) as usize;
        if digit == 0 {
            // A gap inside the name: not a token.
            return None;
        }
        name.push(ALPHABET[digit] as char);
        token /= 38;
    }
    Some(name)
}

/// Packs `name` as a token, or `None` if it's too long or has characters a token can't hold.
pub fn string_to_token(name: &str) -> Option<u64> {
    if name.len() > 12 {
        return None;
    }
    let mut token = 0u64;
    for byte in name.bytes().rev() {
        let digit = ALPHABET[1..].iter().position(|&c| c == byte)? as u64 + 1;
        token = token * 38 + digit;
    }
    Some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_names() {
        for name in ["calais", "port_overlay", "a", "dlc_east", "zzzzzzzzzzzz"] {
            let token = string_to_token(name).unwrap();
            assert_eq!(token_to_string(token).as_deref(), Some(name));
        }
        assert_eq!(string_to_token(""), Some(0));
        assert_eq!(token_to_string(0).as_deref(), Some(""));
    }

    #[test]
    fn packs_the_first_character_lowest() {
        assert_eq!(string_to_token("0"), Some(1));
        assert_eq!(string_to_token("a"), Some(11));
        assert_eq!(string_to_token("ab"), Some(11 + 12 * 38));
    }

    #[test]
    fn rejects_what_a_token_cannot_hold() {
        assert_eq!(string_to_token("Calais"), None);
        assert_eq!(string_to_token("thirteen_char"), None);
        assert_eq!(token_to_string(38), None, "a gap before the last character");
    }
}
