//! The player's save games (`profiles/<profile>/save/<slot>/game.sii`, in the game's documents folder or
//! Steam Cloud's): which garages the player owns, and their headquarters. A save is SII units, as the
//! game's definitions are, written one of three ways:
//!
//! - encrypted (`ScsC`, the game's default): a 32-byte HMAC, a 16-byte IV and the unpacked size (u32),
//!   then AES-256-CBC (the game's fixed key) over zlib-packed data, either of the two below;
//! - binary (`BSII`, version 1 to 3): blocks, each starting with a u32. Zero begins a structure
//!   definition: a validity byte (none: the end), the structure's id (u32), its name, then fields (a value
//!   type, u32, and a name; an ordinal string type adds its table of u32 and string pairs) until a type of
//!   zero. Anything else is a unit of the structure with that id: its id (an ID, below), then its fields'
//!   values in order. Strings are a u32 length and bytes; an ID is a length byte, then that many tokens
//!   (u64, [`crate::token`]), or one u64 address for 0xFF (an unnamed unit); arrays are a u32 count and
//!   their values;
//! - text (`SiiNunit`, with `g_save_format 2` in the game's config);
//! - scrambled text (`3nK`, as some mods' manifests are: [`unscramble`]).
//!
//! The binary layout follows SII_Decrypt's decoder (TheLazyTomcat, <https://github.com/TheLazyTomcat/SII_Decrypt>),
//! learned from, not copied.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockDecryptMut, KeyIvInit};

use crate::reader::{OutOfBounds, Reader};
use crate::sii::{self, Unit};
use crate::token::token_to_string;

/// The key the game encrypts its saves (and other SII files) with.
const KEY: [u8; 32] = [
    0x2a, 0x5f, 0xcb, 0x17, 0x91, 0xd2, 0x2f, 0xb6, 0x02, 0x45, 0xb3, 0xd8, 0x36, 0x9e, 0xd0, 0xb2, 0xc2, 0x73, 0x71,
    0x56, 0x3f, 0xbf, 0x1f, 0x3c, 0x9e, 0xdf, 0x6b, 0x11, 0x82, 0x5a, 0x5d, 0x0a,
];

#[derive(Debug)]
pub enum SaveError {
    /// Neither encrypted, binary nor text SII.
    UnknownFormat,
    /// Doesn't decrypt or unpack.
    Undecryptable,
    UnsupportedVersion(u32),
    /// A value type this reader doesn't know (it can't tell how long the value is).
    UnknownValueType(u32),
    UnknownStructure(u32),
    Truncated,
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFormat => write!(f, "not a save game"),
            Self::Undecryptable => write!(f, "doesn't decrypt"),
            Self::UnsupportedVersion(version) => write!(f, "binary SII version {version} isn't read"),
            Self::UnknownValueType(kind) => write!(f, "unknown value type {kind:#x}"),
            Self::UnknownStructure(id) => write!(f, "a unit of undefined structure {id}"),
            Self::Truncated => write!(f, "ends early"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<OutOfBounds> for SaveError {
    fn from(_: OutOfBounds) -> Self {
        Self::Truncated
    }
}

/// The units of a save file (`game.sii`), however it's written.
pub fn read(data: &[u8]) -> Result<Vec<Unit>, SaveError> {
    match data.get(..4) {
        Some(b"ScsC") => read(&decrypt(data)?),
        Some(b"3nK\x01") => read(&unscramble(data)?),
        Some(b"BSII") => binary(data),
        Some(b"SiiN") => Ok(sii::parse(&String::from_utf8_lossy(data))),
        _ => Err(SaveError::UnknownFormat),
    }
}

/// A scrambled file's text (`3nK`, a byte, then the seed: SCS's own light scrambling, which the game reads as it
/// reads text): each byte XORed with a key from the seed and its place, `((((x << 2) ^ !x) << 3) ^ x)` for `x` the
/// seed plus the place, in a byte. Measured on mods' manifests (2026-10-24): each one so comes to `SiiNunit…`.
fn unscramble(data: &[u8]) -> Result<Vec<u8>, SaveError> {
    let seed = *data.get(5).ok_or(SaveError::Truncated)?;
    let key = |place: usize| {
        let x = seed.wrapping_add(place as u8);
        (((x << 2) ^ !x) << 3) ^ x
    };
    Ok(data[6..].iter().enumerate().map(|(place, &byte)| byte ^ key(place)).collect())
}

fn decrypt(data: &[u8]) -> Result<Vec<u8>, SaveError> {
    let iv: [u8; 16] = data.get(36..52).ok_or(SaveError::Truncated)?.try_into().unwrap();
    let size = u32::from_le_bytes(data.get(52..56).ok_or(SaveError::Truncated)?.try_into().unwrap()) as usize;
    let mut body = data[56..].to_vec();
    let packed = cbc::Decryptor::<aes::Aes256>::new(&KEY.into(), &iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut body)
        .map_err(|_| SaveError::Undecryptable)?;
    let mut unpacked = Vec::with_capacity(crate::archive::inflated_capacity(packed.len(), size));
    flate2::read::ZlibDecoder::new(packed).read_to_end(&mut unpacked).map_err(|_| SaveError::Undecryptable)?;
    Ok(unpacked)
}

/// A structure: its name, and its fields' value types, names and ordinal string tables.
struct Structure {
    name: String,
    fields: Vec<(u32, String, HashMap<u32, String>)>,
}

fn binary(data: &[u8]) -> Result<Vec<Unit>, SaveError> {
    let mut r = Reader::new(data);
    r.skip(4)?;
    let version = r.u32()?;
    if !(1..=3).contains(&version) {
        return Err(SaveError::UnsupportedVersion(version));
    }
    let mut structures: HashMap<u32, Structure> = HashMap::new();
    let mut units = Vec::new();
    loop {
        let block = r.u32()?;
        if block == 0 {
            if r.u8()? == 0 {
                return Ok(units);
            }
            let id = r.u32()?;
            let name = string(&mut r)?;
            let mut fields = Vec::new();
            loop {
                let kind = r.u32()?;
                if kind == 0 {
                    break;
                }
                let field = string(&mut r)?;
                let mut ordinals = HashMap::new();
                if kind == ORDINAL {
                    for _ in 0..r.u32()? {
                        let value = r.u32()?;
                        ordinals.insert(value, string(&mut r)?);
                    }
                }
                fields.push((kind, field, ordinals));
            }
            structures.insert(id, Structure { name, fields });
            continue;
        }
        let structure = structures.get(&block).ok_or(SaveError::UnknownStructure(block))?;
        let mut unit = Unit { class: structure.name.clone(), name: id(&mut r)?, attributes: Vec::new() };
        for (kind, field, ordinals) in &structure.fields {
            for value in values(&mut r, *kind, version, ordinals)? {
                unit.attributes.push((field.clone(), value));
            }
        }
        units.push(unit);
    }
}

const ORDINAL: u32 = 0x37;

fn string(r: &mut Reader) -> Result<String, SaveError> {
    let length = r.u32()? as usize;
    Ok(String::from_utf8_lossy(r.bytes(length)?).into_owned())
}

/// An ID as the text format writes it: its tokens joined with dots, `null` if empty, or an unnamed unit's
/// address (`_nameless.1234.5678`).
fn id(r: &mut Reader) -> Result<String, SaveError> {
    match r.u8()? {
        0 => Ok("null".into()),
        0xFF => {
            let address = r.u64()?;
            let parts: Vec<String> = (0..4).rev().map(|i| format!("{:04x}", (address >> (i * 16)) & 0xFFFF)).collect();
            Ok(format!("_nameless.{}", parts.join(".")))
        }
        length => {
            let tokens: Vec<String> = (0..length)
                .map(|_| Ok(token_to_string(r.u64()?).unwrap_or_default()))
                .collect::<Result<_, SaveError>>()?;
            Ok(tokens.join("."))
        }
    }
}

/// A field's value as text, or an array's values (arrays are a u32 count and their values).
fn values(r: &mut Reader, kind: u32, version: u32, ordinals: &HashMap<u32, String>) -> Result<Vec<String>, SaveError> {
    let floats = |r: &mut Reader, n: usize| -> Result<String, SaveError> {
        let values: Vec<String> = (0..n).map(|_| r.f32().map(|v| v.to_string())).collect::<Result<_, _>>()?;
        Ok(format!("({})", values.join(", ")))
    };
    let one = |r: &mut Reader, kind: u32| -> Result<String, SaveError> {
        Ok(match kind {
            0x01 => string(r)?,
            0x03 => token_to_string(r.u64()?).unwrap_or_default(),
            0x05 => r.f32()?.to_string(),
            0x07 => floats(r, 2)?,
            0x09 => floats(r, 3)?,
            0x11 => format!("({}, {}, {})", r.i32()?, r.i32()?, r.i32()?),
            0x17 => floats(r, 4)?,
            // Position and rotation: from version 2, eight floats (the fourth extending the position's
            // range), before it seven.
            0x19 => floats(r, if version >= 2 { 8 } else { 7 })?,
            0x25 => r.i32()?.to_string(),
            0x27 | 0x2F => r.u32()?.to_string(),
            0x2B => r.u16()?.to_string(),
            0x31 => (r.u64()? as i64).to_string(),
            0x33 => r.u64()?.to_string(),
            0x35 => (r.u8()? != 0).to_string(),
            ORDINAL => {
                let value = r.u32()?;
                ordinals.get(&value).cloned().unwrap_or_else(|| value.to_string())
            }
            0x39 | 0x3B | 0x3D => id(r)?,
            other => return Err(SaveError::UnknownValueType(other)),
        })
    };
    // Array types are their element's type plus one.
    let array_of = match kind {
        0x02 | 0x04 | 0x06 | 0x08 | 0x0A | 0x12 | 0x18 | 0x1A | 0x26 | 0x28 | 0x2C | 0x32 | 0x34 | 0x36 | 0x3A
        | 0x3C => Some(kind - 1),
        _ => None,
    };
    match array_of {
        Some(element) => (0..r.u32()?).map(|_| one(r, element)).collect(),
        None => Ok(vec![one(r, kind)?]),
    }
}

/// What a save says of the player's garages: the cities whose garages they own, and their headquarters'.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Garages {
    pub owned: BTreeSet<String>,
    pub headquarters: Option<String>,
}

/// The player's garages in a save's units: `garage : garage.<city>` units with a `status` other than 0
/// (0: not bought; then its size), and the player's `hq_city`.
pub fn garages(units: &[Unit]) -> Garages {
    let owned = units
        .iter()
        .filter(|unit| unit.class == "garage")
        .filter(|unit| unit.get("status").is_some_and(|status| status != "0"))
        .filter_map(|unit| unit.name.strip_prefix("garage.").map(str::to_owned))
        .collect();
    let headquarters = units
        .iter()
        .find(|unit| unit.class == "player")
        .and_then(|unit| unit.get("hq_city"))
        .filter(|city| !city.is_empty() && *city != "null")
        .map(str::to_owned);
    Garages { owned, headquarters }
}

/// The mods a profile has enabled (its `profile.sii`, read with [`read`]), as the game lists them:
/// `<package>|<name as shown>` (a Steam Workshop mod's package is `mod_workshop_package.<id>`). The list's
/// length, which a text profile writes as an entry of its own (`active_mods: 50`), isn't a mod.
pub fn active_mods(units: &[Unit]) -> Vec<String> {
    units
        .iter()
        .filter(|unit| unit.class == "user_profile")
        .flat_map(|unit| unit.all("active_mods"))
        .filter(|entry| entry.contains('|'))
        .map(str::to_owned)
        .collect()
}

/// A profile (`profile.sii`) without the active mods `drop` picks (by how the profile lists them,
/// `<package>|<name>`), changed in nothing else: the file as the game reads it, decrypted (binary or text, as
/// it was inside; the game reads either, and encrypts it again on its next save). `None` when it lists none
/// of them (it's then left as it is).
///
/// A binary profile keeps every byte but the list's: its count and the entries kept, copied as they were
/// (decoding and writing it again would round its numbers). A text one keeps every line but the dropped
/// entries', the others numbered again and the count said again.
pub fn without_mods(data: &[u8], drop: impl Fn(&str) -> bool) -> Result<Option<Vec<u8>>, SaveError> {
    match data.get(..4) {
        Some(b"ScsC") => without_mods(&decrypt(data)?, drop),
        Some(b"BSII") => binary_without_mods(data, &drop),
        Some(b"SiiN") => Ok(text_without_mods(&String::from_utf8_lossy(data), &drop).map(String::into_bytes)),
        _ => Err(SaveError::UnknownFormat),
    }
}

/// The binary profile's `user_profile` unit's `active_mods`, found by walking the file as [`binary`] does,
/// and spliced without the dropped entries.
fn binary_without_mods(data: &[u8], drop: &dyn Fn(&str) -> bool) -> Result<Option<Vec<u8>>, SaveError> {
    let mut r = Reader::new(data);
    r.skip(4)?;
    let version = r.u32()?;
    if !(1..=3).contains(&version) {
        return Err(SaveError::UnsupportedVersion(version));
    }
    let mut structures: HashMap<u32, Structure> = HashMap::new();
    loop {
        let block = r.u32()?;
        if block == 0 {
            if r.u8()? == 0 {
                return Ok(None);
            }
            let id = r.u32()?;
            let name = string(&mut r)?;
            let mut fields = Vec::new();
            loop {
                let kind = r.u32()?;
                if kind == 0 {
                    break;
                }
                let field = string(&mut r)?;
                let mut ordinals = HashMap::new();
                if kind == ORDINAL {
                    for _ in 0..r.u32()? {
                        let value = r.u32()?;
                        ordinals.insert(value, string(&mut r)?);
                    }
                }
                fields.push((kind, field, ordinals));
            }
            structures.insert(id, Structure { name, fields });
            continue;
        }
        let structure = structures.get(&block).ok_or(SaveError::UnknownStructure(block))?;
        id(&mut r)?;
        let profile = structure.name == "user_profile";
        for (kind, field, ordinals) in &structure.fields {
            // An array of strings: a u32 count, then each a u32 length and its bytes.
            if profile && field == "active_mods" && *kind == 0x02 {
                let start = r.pos();
                let mut kept = Vec::new();
                let mut dropped = 0;
                for _ in 0..r.u32()? {
                    let entry = r.pos();
                    let text = string(&mut r)?;
                    if drop(&text) {
                        dropped += 1;
                    } else {
                        kept.extend_from_slice(&data[entry..r.pos()]);
                    }
                }
                if dropped == 0 {
                    return Ok(None);
                }
                let count = (data[start..start + 4].try_into().map(u32::from_le_bytes).unwrap_or(0)) - dropped;
                let mut out = Vec::with_capacity(data.len());
                out.extend_from_slice(&data[..start]);
                out.extend_from_slice(&count.to_le_bytes());
                out.extend_from_slice(&kept);
                out.extend_from_slice(&data[r.pos()..]);
                return Ok(Some(out));
            }
            values(&mut r, *kind, version, ordinals)?;
        }
    }
}

/// The text profile without the dropped `active_mods[i]: "…"` lines, the rest numbered again, and its
/// `active_mods: n` count said again.
fn text_without_mods(text: &str, drop: &dyn Fn(&str) -> bool) -> Option<String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let entry = |line: &str| -> Option<String> {
        let rest = line.trim_start().strip_prefix("active_mods[")?;
        let (index, value) = rest.split_once("]:")?;
        (index.is_empty() || index.bytes().all(|b| b.is_ascii_digit()))
            .then(|| value.trim().trim_matches('"').to_owned())
    };
    let lines: Vec<&str> = text.split(newline).collect();
    if !lines.iter().any(|line| entry(line).is_some_and(|value| drop(&value))) {
        return None;
    }
    let kept_count = lines.iter().filter(|line| entry(line).is_some_and(|value| !drop(&value))).count();
    let mut next = 0;
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        if let Some(value) = entry(line) {
            if drop(&value) {
                continue;
            }
            let indent = &line[..line.len() - line.trim_start().len()];
            out.push(format!("{indent}active_mods[{next}]: \"{value}\""));
            next += 1;
        } else if line.trim_start().starts_with("active_mods:") {
            let indent = &line[..line.len() - line.trim_start().len()];
            out.push(format!("{indent}active_mods: {kept_count}"));
        } else {
            out.push(line.to_owned());
        }
    }
    Some(out.join(newline))
}

/// The profile file of the profile a save belongs to (`profiles/<profile>/save/<slot>/game.sii` →
/// `profiles/<profile>/profile.sii`).
pub fn profile_of(save: &Path) -> Option<PathBuf> {
    Some(save.parent()?.parent()?.parent()?.join("profile.sii"))
}

/// The save the game wrote last, in any profile, in the game's documents folder or Steam Cloud's: the
/// one being played, unless the game has loaded a profile that hasn't saved since (see [`latest_of`]).
pub fn latest(documents: &Path, steam_userdata: Option<&Path>) -> Option<PathBuf> {
    latest_of(documents, steam_userdata, |_| true)
}

/// [`latest`], of the profiles whose folder's name (the profile's name in hexadecimal) `profile` takes.
pub fn latest_of(documents: &Path, steam_userdata: Option<&Path>, profile: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let mut profile_dirs = vec![documents.join("profiles"), documents.join("steam_profiles")];
    if let Some(userdata) = steam_userdata {
        for user in std::fs::read_dir(userdata).into_iter().flatten().flatten() {
            profile_dirs.push(user.path().join("227300/remote/profiles"));
        }
    }
    let saves = profile_dirs
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
        .filter(|folder| folder.file_name().to_str().is_some_and(&profile))
        .flat_map(|profile| std::fs::read_dir(profile.path().join("save")).into_iter().flatten().flatten())
        .map(|slot| slot.path().join("game.sii"));
    saves
        .filter_map(|path| Some((std::fs::metadata(&path).ok()?.modified().ok()?, path)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scrambled_text() {
        // Scrambled as the game scrambles (the same XOR undoes it), seed 0x50: the key's first bytes 0x28 and 0x01,
        // as measured on a mod's manifest against its `Si`.
        let text = b"SiiNunit\n{\nmod_package : .package_name\n{\n\tauthor: \"Someone\"\n}\n}\n";
        let mut data = b"3nK\x01\x00\x50".to_vec();
        data.extend(text.iter().enumerate().map(|(place, &byte)| {
            let x = 0x50u8.wrapping_add(place as u8);
            byte ^ ((((x << 2) ^ !x) << 3) ^ x)
        }));
        assert_eq!((data[6] ^ b'S', data[7] ^ b'i'), (0x28, 0x01));
        let units = read(&data).unwrap();
        assert_eq!(units[0].get("author"), Some("Someone"));
        assert!(matches!(read(b"3nK\x01\x00"), Err(SaveError::Truncated)));
    }

    #[test]
    fn finds_the_newest_save_of_the_profile_asked_for() {
        // Two profiles' saves: W's newer than Eren's (Eren loaded, not saved since).
        let root = std::env::temp_dir().join(format!("westplay-latest-of-{}", std::process::id()));
        let documents = root.join("documents");
        let save = |folder: &str, slot: &str| {
            let dir = documents.join("profiles").join(folder).join("save").join(slot);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("game.sii"), "x").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(30));
            dir.join("game.sii")
        };
        let eren = save("4572656E", "autosave");
        let w = save("57", "autosave");
        assert_eq!(latest(&documents, None), Some(w));
        assert_eq!(latest_of(&documents, None, |folder| folder == "4572656E"), Some(eren));
        assert_eq!(latest_of(&documents, None, |folder| folder == "00"), None);
        let _ = std::fs::remove_dir_all(&root);
    }
    use crate::token::string_to_token;

    /// A binary profile: `user_profile { profile_name, active_mods[], cached_experience }`, as the game writes
    /// one.
    fn binary_profile(mods: &[&str]) -> Vec<u8> {
        let mut b = b"BSII".to_vec();
        let u32le = |b: &mut Vec<u8>, v: u32| b.extend(v.to_le_bytes());
        let text = |b: &mut Vec<u8>, s: &str| {
            b.extend((s.len() as u32).to_le_bytes());
            b.extend(s.as_bytes());
        };
        u32le(&mut b, 3);
        u32le(&mut b, 0);
        b.push(1);
        u32le(&mut b, 4);
        text(&mut b, "user_profile");
        u32le(&mut b, 0x01);
        text(&mut b, "profile_name");
        u32le(&mut b, 0x02);
        text(&mut b, "active_mods");
        u32le(&mut b, 0x05);
        text(&mut b, "cached_experience");
        u32le(&mut b, 0);
        // The unit: unnamed, then its values.
        u32le(&mut b, 4);
        b.push(0xFF);
        b.extend(0x1234_5678_9abc_def0u64.to_le_bytes());
        text(&mut b, "Western");
        u32le(&mut b, mods.len() as u32);
        for m in mods {
            text(&mut b, m);
        }
        b.extend(1234.567f32.to_le_bytes());
        u32le(&mut b, 0);
        b.push(0);
        b
    }

    fn westplay(entry: &str) -> bool {
        entry.split('|').next() == Some("westplay")
    }

    /// A profile without WestPlay's mod is the same profile in everything else, byte for byte where it's
    /// binary; one that doesn't list it is left alone.
    #[test]
    fn takes_a_mod_out_of_a_binary_profile_and_nothing_else() {
        let before = binary_profile(&["promods|ProMods", "westplay|WestPlay", "rta|Real Traffic"]);
        let after = without_mods(&before, westplay).unwrap().expect("changed");
        assert_eq!(after, binary_profile(&["promods|ProMods", "rta|Real Traffic"]));
        let units = read(&after).unwrap();
        assert_eq!(active_mods(&units), ["promods|ProMods", "rta|Real Traffic"]);
        assert_eq!(units[0].get("cached_experience"), read(&before).unwrap()[0].get("cached_experience"));
        assert_eq!(without_mods(&after, westplay).unwrap(), None);
        assert_eq!(without_mods(&binary_profile(&[]), westplay).unwrap(), None);
        assert_eq!(without_mods(&binary_profile(&["westplay|WestPlay"]), westplay).unwrap(), Some(binary_profile(&[])));
    }

    #[test]
    fn takes_a_mod_out_of_a_text_profile_numbering_the_rest_again() {
        let before = "SiiNunit\r\n{\r\nuser_profile : _nameless.1 {\r\n profile_name: Western\r\n active_mods: 3\r\n \
             active_mods[0]: \"promods|ProMods\"\r\n active_mods[1]: \"westplay|WestPlay\"\r\n \
             active_mods[2]: \"rta|Real Traffic\"\r\n cached_experience: 12\r\n}\r\n\r\n}\r\n";
        let after = String::from_utf8(without_mods(before.as_bytes(), westplay).unwrap().unwrap()).unwrap();
        assert_eq!(
            after,
            "SiiNunit\r\n{\r\nuser_profile : _nameless.1 {\r\n profile_name: Western\r\n active_mods: 2\r\n \
             active_mods[0]: \"promods|ProMods\"\r\n active_mods[1]: \"rta|Real Traffic\"\r\n cached_experience: 12\r\n}\r\n\r\n}\r\n"
        );
        assert_eq!(without_mods(after.as_bytes(), westplay).unwrap(), None);
    }

    /// A binary save: a structure `garage` with a token `name`... built as the game writes one.
    fn binary_save() -> Vec<u8> {
        let mut b = b"BSII".to_vec();
        let u32le = |b: &mut Vec<u8>, v: u32| b.extend(v.to_le_bytes());
        let text = |b: &mut Vec<u8>, s: &str| {
            b.extend((s.len() as u32).to_le_bytes());
            b.extend(s.as_bytes());
        };
        u32le(&mut b, 3);
        // Structure 7: garage { status: u32, vehicles: array of IDs, size: ordinal (0 tiny, 2 big) }.
        u32le(&mut b, 0);
        b.push(1);
        u32le(&mut b, 7);
        text(&mut b, "garage");
        u32le(&mut b, 0x27);
        text(&mut b, "status");
        u32le(&mut b, 0x3A);
        text(&mut b, "vehicles");
        u32le(&mut b, ORDINAL);
        text(&mut b, "size");
        u32le(&mut b, 2);
        u32le(&mut b, 0);
        text(&mut b, "tiny");
        u32le(&mut b, 2);
        text(&mut b, "big");
        u32le(&mut b, 0);
        // Two garages: garage.calais (status 2, two trucks), garage.dover (not bought).
        for (city, status, trucks) in [("calais", 2, 2), ("dover", 0, 0)] {
            u32le(&mut b, 7);
            b.push(2);
            b.extend(string_to_token("garage").unwrap().to_le_bytes());
            b.extend(string_to_token(city).unwrap().to_le_bytes());
            u32le(&mut b, status);
            u32le(&mut b, trucks);
            for i in 0..trucks {
                b.push(0xFF);
                b.extend((0x1234_0000u64 + u64::from(i)).to_le_bytes());
            }
            u32le(&mut b, if status == 0 { 0 } else { 2 });
        }
        // Structure 8: player { hq_city: token }.
        u32le(&mut b, 0);
        b.push(1);
        u32le(&mut b, 8);
        text(&mut b, "player");
        u32le(&mut b, 0x03);
        text(&mut b, "hq_city");
        u32le(&mut b, 0);
        u32le(&mut b, 8);
        b.push(0xFF);
        b.extend(1u64.to_le_bytes());
        b.extend(string_to_token("calais").unwrap().to_le_bytes());
        // The end.
        u32le(&mut b, 0);
        b.push(0);
        b
    }

    #[test]
    fn reads_a_binary_save_into_units() {
        let units = read(&binary_save()).unwrap();
        assert_eq!(units.len(), 3);
        assert_eq!((units[0].class.as_str(), units[0].name.as_str()), ("garage", "garage.calais"));
        assert_eq!(units[0].get("status"), Some("2"));
        assert_eq!(
            units[0].all("vehicles").collect::<Vec<_>>(),
            ["_nameless.0000.0000.1234.0000", "_nameless.0000.0000.1234.0001"]
        );
        assert_eq!(units[0].get("size"), Some("big"));
        assert_eq!(units[2].name, "_nameless.0000.0000.0000.0001");
    }

    #[test]
    fn finds_the_garages_the_player_owns() {
        let garages = garages(&read(&binary_save()).unwrap());
        assert_eq!(garages.owned.into_iter().collect::<Vec<_>>(), ["calais"]);
        assert_eq!(garages.headquarters.as_deref(), Some("calais"));
        // A text save reads the same way.
        let text =
            "SiiNunit\n{\ngarage : garage.berlin {\n status: 1\n}\nplayer : _nameless.1 {\n hq_city: berlin\n}\n}\n";
        let garages = super::garages(&read(text.as_bytes()).unwrap());
        assert_eq!((garages.owned.len(), garages.headquarters.as_deref()), (1, Some("berlin")));
    }

    #[test]
    fn lists_a_profiles_mods_and_finds_it_from_a_save() {
        let text = "SiiNunit
{
user_profile : _nameless.1 {
 active_mods: 2
 active_mods[0]: \"a|Mod A\"
 active_mods[1]: \"b|Mod B\"
}
}
";
        let mods = active_mods(&read(text.as_bytes()).unwrap());
        // A text profile lists its count first, as text arrays do: not a mod.
        assert_eq!(mods, ["a|Mod A", "b|Mod B"]);
        let save = Path::new("profiles/57/save/autosave/game.sii");
        assert_eq!(profile_of(save), Some(PathBuf::from("profiles/57/profile.sii")));
    }

    #[test]
    fn refuses_what_isnt_a_save() {
        assert!(matches!(read(b"PNG?"), Err(SaveError::UnknownFormat)));
        let mut truncated = binary_save();
        truncated.truncate(40);
        assert!(matches!(read(&truncated), Err(SaveError::Truncated)));
    }
}
