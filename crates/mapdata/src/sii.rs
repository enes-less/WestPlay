//! The game's text definition files (`.sii`, and `.sui` fragments they include): units of named
//! attributes,
//!
//! ```text
//! SiiNunit
//! {
//! road_look : road.template1
//! {
//!     name: "*Road 1 scan temp"
//!     lanes_left[]: traffic_lane.road.local
//! }
//! @include "city/calais.sui"
//! }
//! ```
//!
//! Only what the map needs is supported: units, attributes (arrays as repeated keys), comments and
//! includes. Values are kept as written, except quoted strings, which lose their quotes and escapes
//! (`\"`, `\\`, `\n`, `\t`, and `\xNN`: bytes of UTF-8).

use crate::files::{FileError, GameFiles};

#[derive(Debug, PartialEq)]
pub struct Unit {
    /// `road_look`
    pub class: String,
    /// `road.template1`
    pub name: String,
    /// In file order; array entries (`key[]`) repeat their key, without the brackets.
    pub attributes: Vec<(String, String)>,
}

impl Unit {
    /// The first value of `key`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attributes.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// All values of an array attribute.
    pub fn all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> {
        self.attributes.iter().filter(move |(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn number(&self, key: &str) -> Option<f32> {
        self.get(key)?.parse().ok()
    }
}

/// Reads the units of a definition file, following its includes (relative to the file, or from the
/// game's root with a leading slash). Missing includes are skipped.
pub fn read_units(files: &GameFiles, path: &str) -> Result<Vec<Unit>, FileError> {
    let mut units = Vec::new();
    let mut current: Option<Unit> = None;
    read_into(files, path, &mut units, &mut current, 0)?;
    Ok(units)
}

/// How deep includes are followed (a file including itself would go on for ever).
const MOST_INCLUDES: u32 = 8;

/// [`read_family`], and if it can't be read, nothing, noted ([`GameFiles::note_unread`]): what it defines is left out.
pub fn read_family_or_note(files: &GameFiles, dir: &str, base: &str) -> Vec<Unit> {
    read_family(files, dir, base).unwrap_or_else(|error| {
        files.note_unread(&format!("{dir}/{base}.sii"), &error);
        Vec::new()
    })
}

/// The units of `<dir>/<base>.sii` and every `<dir>/<base>.*.sii` (the extensions DLCs and mods add), base
/// file first.
pub fn read_family(files: &GameFiles, dir: &str, base: &str) -> Result<Vec<Unit>, FileError> {
    let mut names: Vec<String> = files
        .list(dir)?
        .into_iter()
        .filter(|name| {
            name.strip_prefix(base)
                .is_some_and(|rest| rest == ".sii" || (rest.starts_with('.') && rest.ends_with(".sii")))
        })
        .collect();
    names.sort_by_key(|name| (name != &format!("{base}.sii"), name.clone()));
    let mut units = Vec::new();
    for name in &names {
        units.extend(read_units(files, &format!("{dir}/{name}"))?);
    }
    Ok(units)
}

fn read_into(
    files: &GameFiles,
    path: &str,
    units: &mut Vec<Unit>,
    current: &mut Option<Unit>,
    depth: u32,
) -> Result<(), FileError> {
    let Some(bytes) = files.read(path)? else { return Ok(()) };
    if depth > MOST_INCLUDES {
        files.note_unread(path, &format_args!("included more than {MOST_INCLUDES} deep"));
        return Ok(());
    }
    // However it's written: encrypted, scrambled or binary as the game reads them (only text was read: a mod's
    // definitions written otherwise were skipped, the review of 2026-11-11, M9); one that can't be read, noted.
    let text = match crate::save::definitions(&bytes) {
        Ok(crate::save::Sii::Text(text)) => text,
        Ok(crate::save::Sii::Units(read)) => {
            units.extend(read);
            return Ok(());
        }
        Err(error) => {
            files.note_unread(path, &error);
            return Ok(());
        }
    };
    let text = String::from_utf8_lossy(&text);
    let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    parse_into(&text, units, current, &mut |include, units, current| {
        let target = if include.starts_with('/') { include.to_owned() } else { format!("{dir}/{include}") };
        read_into(files, &target, units, current, depth + 1)
    })
}

/// The units of SII text with no includes (a save game written as text).
pub fn parse(text: &str) -> Vec<Unit> {
    let (mut units, mut current) = (Vec::new(), None);
    let _ = parse_into(text, &mut units, &mut current, &mut |_, _, _| Ok(()));
    units
}

type Include<'a> = dyn FnMut(&str, &mut Vec<Unit>, &mut Option<Unit>) -> Result<(), FileError> + 'a;

fn parse_into(
    text: &str,
    units: &mut Vec<Unit>,
    current: &mut Option<Unit>,
    include: &mut Include,
) -> Result<(), FileError> {
    let mut in_comment = false;
    for raw in text.lines() {
        let line = strip_comments(raw, &mut in_comment);
        let line = line.trim();
        if line.is_empty() || line == "SiiNunit" {
            continue;
        }
        if let Some(target) = line.strip_prefix("@include") {
            include(&unquote(target.trim()), units, current)?;
            continue;
        }
        if line == "{" {
            continue;
        }
        if line == "}" {
            if let Some(unit) = current.take() {
                units.push(unit);
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match current {
            Some(unit) => {
                // An array written by index (`lanes: 2`, then `lanes[0]: …`, `lanes[1]: …`, as the game's own
                // definitions write them): its count is no entry of it. It was kept as one, a road look's count a lane
                // more (the review of 2026-11-11, minor 18).
                if let Some(name) = key.strip_suffix("[0]")
                    && let Some(count) =
                        unit.attributes.iter().rposition(|(k, v)| k == name && v.parse::<usize>().is_ok())
                {
                    unit.attributes.remove(count);
                }
                let key = key.strip_suffix("[]").or_else(|| key.split_once('[').map(|(k, _)| k)).unwrap_or(key);
                unit.attributes.push((key.to_owned(), unquote(value)));
            }
            None => {
                // `class : name`, maybe with the opening brace on the same line.
                let name = value.trim_end_matches('{').trim();
                *current = Some(Unit { class: key.to_owned(), name: name.to_owned(), attributes: Vec::new() });
                if value.ends_with('}') {
                    units.extend(current.take());
                }
            }
        }
    }
    Ok(())
}

/// Removes `#` and `//` line comments and `/* */` block comments (which may span lines), outside strings.
fn strip_comments(line: &str, in_comment: &mut bool) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if *in_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                *in_comment = false;
            }
            continue;
        }
        match c {
            // An escaped character inside a string (a quote, say) is kept as it is.
            '\\' if in_string => {
                out.push(c);
                out.extend(chars.next());
                continue;
            }
            '"' => in_string = !in_string,
            '#' if !in_string => break,
            '/' if !in_string && chars.peek() == Some(&'/') => break,
            '/' if !in_string && chars.peek() == Some(&'*') => {
                chars.next();
                *in_comment = true;
                continue;
            }
            _ => {}
        }
        out.push(c);
    }
    out
}

/// A value as meant: a quoted string without its quotes and escapes, anything else as written.
fn unquote(value: &str) -> String {
    let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) else { return value.to_owned() };
    if !inner.contains('\\') {
        return inner.to_owned();
    }
    // Escapes may spell out the bytes of UTF-8 (`\xc3\xa9`), so the text is rebuilt as bytes.
    let mut bytes = Vec::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            bytes.extend(c.encode_utf8(&mut [0; 4]).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                match u8::from_str_radix(&hex, 16) {
                    Ok(byte) => bytes.push(byte),
                    Err(_) => bytes.extend(format!("\\x{hex}").as_bytes()),
                }
            }
            // `\"`, `\\`, and anything else escaped: the character itself.
            Some(other) => bytes.extend(other.encode_utf8(&mut [0; 4]).as_bytes()),
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::files::Mount;
    use crate::test_dir::TempDir;

    fn files_with(contents: &[(&str, &str)]) -> (GameFiles, TempDir) {
        let dir = TempDir::new();
        for (path, text) in contents {
            let file = dir.path.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        }
        (GameFiles::new(vec![Mount::Folder(dir.path.clone())]), dir)
    }

    /// A family's files written otherwise than as text are read as the game reads them (a scrambled one here); one
    /// that can't be read is noted, its units left out, the rest read.
    #[test]
    fn reads_a_family_however_its_written_and_notes_what_it_cant() {
        let (files, dir) = files_with(&[("def/world/road_look.sii", "SiiNunit\n{\nroad_look : road.a\n{\n}\n}\n")]);
        let text = b"SiiNunit\n{\nroad_look : road.b\n{\n}\n}\n";
        let mut scrambled = b"3nK\x01\x00\x50".to_vec();
        scrambled.extend(text.iter().enumerate().map(|(place, &byte)| {
            let x = 0x50u8.wrapping_add(place as u8);
            byte ^ ((((x << 2) ^ !x) << 3) ^ x)
        }));
        std::fs::write(dir.path.join("def/world/road_look.b.sii"), scrambled).unwrap();
        std::fs::write(dir.path.join("def/world/road_look.c.sii"), b"ScsC broken").unwrap();
        let units = read_family_or_note(&files, "/def/world", "road_look");
        let names: Vec<&str> = units.iter().map(|unit| unit.name.as_str()).collect();
        assert_eq!(names, ["road.a", "road.b"]);
        let unread = files.unread();
        assert_eq!(unread.count, 1);
        assert!(unread.first[0].starts_with("/def/world/road_look.c.sii in "), "{}", unread.first[0]);
    }

    /// An array written by index has its entries alone, not its count; one written `key[]` has its entries; a plain
    /// value stays.
    #[test]
    fn an_indexed_arrays_count_is_no_entry_of_it() {
        let units = parse(
            "SiiNunit
{
road_look : road.a
{
 lanes_left: 2
 lanes_left[0]: traffic_lane.road.local
              lanes_left[1]: traffic_lane.road.local
 lanes_right[]: traffic_lane.road.motorway
 offset: 3
              shoulders: 1
}
}
",
        );
        assert_eq!(units[0].all("lanes_left").collect::<Vec<_>>(), ["traffic_lane.road.local"; 2]);
        assert_eq!(units[0].all("lanes_right").collect::<Vec<_>>(), ["traffic_lane.road.motorway"]);
        assert_eq!((units[0].get("offset"), units[0].get("shoulders")), (Some("3"), Some("1")));
    }

    #[test]
    fn reads_units_arrays_and_includes() {
        let (files, _dir) = files_with(&[
            (
                "def/city.sii",
                "SiiNunit\n{\n# comment: not a unit\n@include \"city/calais.sui\"\n@include \"/def/more.sui\"\n}\n",
            ),
            ("def/city/calais.sui", "city_data : city.calais\n{\n\tcity_name: \"Calais: port\" // trailing\n}\n"),
            ("def/more.sui", "road_look : road.look1 {\n lanes_left[]: a\n lanes_left[]: b /* x */\n}\n"),
        ]);
        let units = read_units(&files, "/def/city.sii").unwrap();
        assert_eq!(units.len(), 2);
        assert_eq!((units[0].class.as_str(), units[0].name.as_str()), ("city_data", "city.calais"));
        assert_eq!(units[0].get("city_name"), Some("Calais: port"));
        assert_eq!(units[1].all("lanes_left").collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn unescapes_strings() {
        let text = r#"SiiNunit
{
u : n.a
{
k: "say \"hi\" // not a comment"
m: "two\nlines \xc3\xa9"
}
}
"#;
        let (files, _dir) = files_with(&[("a.sii", text)]);
        let units = read_units(&files, "a.sii").unwrap();
        assert_eq!(units[0].get("k"), Some(r#"say "hi" // not a comment"#));
        assert_eq!(units[0].get("m"), Some("two\nlines é"));
    }

    #[test]
    fn ignores_block_comments_across_lines() {
        let (files, _dir) = files_with(&[("a.sii", "SiiNunit\n{\n/* x : y\n{\n}*/\nu : n.a\n{\nk: 1.5\n}\n}\n")]);
        let units = read_units(&files, "a.sii").unwrap();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].number("k"), Some(1.5));
    }
}
