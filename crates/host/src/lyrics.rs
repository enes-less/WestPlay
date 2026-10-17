//! A song's words, as Apple Music and Spotify answer them (apple_music.js, spotify.js) and the apps show them in
//! Now Playing: line by line, each with when it's sung (synced) or not, where the words are from (Spotify's
//! provider, Apple Music's songwriters), and Spotify's own colours for them. Only these fields pass on, checked:
//! a sensible number of lines of a sensible length, times that are times, colours of one shape.

/// A line: when it's sung (seconds; `end` 0: until the next), its words, and whether it starts a stanza.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Line {
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub end: f64,
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stanza: bool,
}

/// Spotify's colours for a song's words: behind them, the words to come, and the words sung (`#rrggbb`).
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Colors {
    pub background: String,
    pub text: String,
    pub highlight: String,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Lyrics {
    /// Each line with when it's sung (else only the words, in order).
    #[serde(default)]
    pub synced: bool,
    #[serde(default)]
    pub lines: Vec<Line>,
    /// Where the words are from ("Musixmatch").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider: String,
    /// Who wrote them (Apple Music says).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<Colors>,
    /// Written right to left (Arabic, Hebrew).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rtl: bool,
}

/// The most lines a song's words have here (a long song has a few hundred).
const MOST_LINES: usize = 600;
const MOST_TEXT: usize = 300;

fn is_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn cut(text: &str, most: usize) -> String {
    text.chars().take(most).collect()
}

impl Lyrics {
    /// Only what's sound: so many lines, so long; times that are times (else not synced); colours of one shape.
    pub fn checked(mut self) -> Self {
        self.lines.truncate(MOST_LINES);
        for line in &mut self.lines {
            line.text = cut(&line.text, MOST_TEXT);
        }
        let timed = |time: f64| time.is_finite() && (0.0..=36_000.0).contains(&time);
        if !self.lines.iter().all(|line| timed(line.start) && timed(line.end)) {
            self.synced = false;
            for line in &mut self.lines {
                line.start = 0.0;
                line.end = 0.0;
            }
        }
        self.provider = cut(&self.provider, 60);
        self.writers.truncate(20);
        self.writers = self.writers.iter().map(|writer| cut(writer, 100)).collect();
        self.colors = self
            .colors
            .filter(|colors| is_color(&colors.background) && is_color(&colors.text) && is_color(&colors.highlight));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_only_whats_sound() {
        let lyrics: Lyrics = serde_json::from_str(
            r##"{"synced":true,"lines":[{"start":22.87,"end":0,"text":"one","stanza":true},{"start":25.1,"text":"two"}],
            "provider":"Musixmatch","colors":{"background":"#10c2a0","text":"#000000","highlight":"#ffffff"},"extra":"x"}"##,
        )
        .unwrap();
        let checked = lyrics.clone().checked();
        assert_eq!(checked, lyrics);
        assert!(!serde_json::to_string(&checked).unwrap().contains("extra"));
        // A colour of another shape: none; a time that isn't one: not synced.
        let odd: Lyrics = serde_json::from_str(
            r##"{"synced":true,"lines":[{"start":-1,"text":"x"}],"colors":{"background":"red","text":"#000000","highlight":"#ffffff"}}"##,
        )
        .unwrap();
        let odd = odd.checked();
        assert!(!odd.synced && odd.lines[0].start == 0.0 && odd.colors.is_none());
        // So many lines, so long.
        let long = Lyrics {
            lines: vec![Line { start: 0.0, end: 0.0, text: "a".repeat(1000), stanza: false }; 900],
            ..Default::default()
        }
        .checked();
        assert_eq!(long.lines.len(), MOST_LINES);
        assert_eq!(long.lines[0].text.chars().count(), MOST_TEXT);
    }
}
