//! The game's text in the language it's set to: `/locale/<language>/local.sii` and its extensions (in
//! `locale.scs`, and from DLCs and mods), a `localization_db` unit pairing keys with strings. Definitions
//! refer to its strings as `@@key@@` (a city's `city_name_localized: "@@varna@@"`).

use std::collections::HashMap;

use crate::files::{FileError, GameFiles};
use crate::sii;

/// Keys to the strings of one language.
#[derive(Debug, Default)]
pub struct Strings {
    strings: HashMap<String, String>,
}

impl Strings {
    /// The strings of `language` (`tr_tr`, `en_us`…); none if the game doesn't have it.
    pub fn load(files: &GameFiles, language: &str) -> Result<Self, FileError> {
        let mut strings = HashMap::new();
        for unit in sii::read_family(files, &format!("/locale/{language}"), "local")? {
            // Later files (overrides, DLCs, mods) replace earlier strings.
            strings.extend(unit.all("key").zip(unit.all("val")).map(|(key, value)| (key.to_owned(), value.to_owned())));
        }
        Ok(Self { strings })
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }

    /// Text as shown: a reference (`@@key@@`) looked up, `None` if this language hasn't got it; anything
    /// else as written.
    pub fn resolve<'a>(&'a self, text: &'a str) -> Option<&'a str> {
        match text.strip_prefix("@@").and_then(|rest| rest.strip_suffix("@@")) {
            Some(key) => self.strings.get(key).map(String::as_str),
            None => Some(text),
        }
    }
}

impl FromIterator<(String, String)> for Strings {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(pairs: I) -> Self {
        Self { strings: pairs.into_iter().collect() }
    }
}

/// The game's language, from its `config.cfg` (in the player's documents, `Euro Truck Simulator 2/`):
/// `uset g_lang_init "tr_tr"`.
pub fn game_language(config: &str) -> Option<String> {
    config.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next() == Some("uset") && words.next() == Some("g_lang_init"))
            .then(|| words.next())
            .flatten()
            .map(|value| value.trim_matches('"').to_owned())
            .filter(|language| !language.is_empty())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Mount;
    use crate::test_dir::TempDir;

    #[test]
    fn looks_names_up_in_the_language_and_its_extensions() {
        let dir = TempDir::new();
        let locale = dir.path.join("locale/tr_tr");
        std::fs::create_dir_all(&locale).unwrap();
        std::fs::write(
            locale.join("local.sii"),
            "\u{feff}SiiNunit\n{\nlocalization_db : .localization\n{\n@include \"localization.sui\"\n}\n}\n",
        )
        .unwrap();
        std::fs::write(
            locale.join("localization.sui"),
            "\tkey[]: \"athens\"\n\tval[]: \"Atina\"\n\tkey[]: \"wien\"\n\tval[]: \"Wien\"\n",
        )
        .unwrap();
        std::fs::write(
            locale.join("local.override.sii"),
            "SiiNunit\n{\nlocalization_db : .localization\n{\n\tkey[]: \"wien\"\n\tval[]: \"Viyana\"\n}\n}\n",
        )
        .unwrap();
        let files = GameFiles::new(vec![Mount::Folder(dir.path.clone())]);

        let strings = Strings::load(&files, "tr_tr").unwrap();
        assert_eq!(strings.resolve("@@athens@@"), Some("Atina"));
        assert_eq!(strings.resolve("@@wien@@"), Some("Viyana"));
        assert_eq!(strings.resolve("@@calais@@"), None);
        assert_eq!(strings.resolve("Calais"), Some("Calais"));
        assert!(Strings::load(&files, "xx_xx").unwrap().is_empty());
    }

    #[test]
    fn reads_the_language_from_the_config() {
        let config = "uset g_fps_limit \"60\"\nuset g_lang_init \"tr_tr\"\nuset g_other \"1\"\n";
        assert_eq!(game_language(config), Some("tr_tr".to_owned()));
        assert_eq!(game_language("uset g_lang_init \"\"\n"), None);
        assert_eq!(game_language(""), None);
    }
}
