//! Steam's KeyValues text (`libraryfolders.vdf`, `appmanifest_*.acf`): quoted keys, each followed by a quoted
//! value or a `{ … }` block of more of them. `//` comments are skipped; escapes (`\\`, `\"`) are read.

/// A value: text, or a block of keyed values in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Block(Vec<(String, Value)>),
}

impl Value {
    /// The value under `key` in a block (Steam's keys aren't case-sensitive), if there is one.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Block(entries) => entries.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Value::Text(_) => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Value::Text(text) => Some(text),
            Value::Block(_) => None,
        }
    }

    /// A block's entries (none for text).
    pub fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Block(entries) => entries,
            Value::Text(_) => &[],
        }
    }
}

/// The whole file, as a block of its top-level entries; `None` if it isn't KeyValues text.
pub fn parse(text: &str) -> Option<Value> {
    let mut tokens = tokens(text)?.into_iter().peekable();
    let entries = block(&mut tokens, false)?;
    Some(Value::Block(entries))
}

#[derive(Debug, PartialEq)]
enum Token {
    Text(String),
    Open,
    Close,
}

fn tokens(text: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '"' => {
                let mut text = String::new();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            'n' => text.push('\n'),
                            't' => text.push('\t'),
                            other => text.push(other),
                        },
                        other => text.push(other),
                    }
                }
                tokens.push(Token::Text(text));
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            // An unquoted word (Steam writes none, but the format allows them).
            c => {
                let mut text = c.to_string();
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() || next == '{' || next == '}' || next == '"' {
                        break;
                    }
                    text.push(next);
                    chars.next();
                }
                tokens.push(Token::Text(text));
            }
        }
    }
    Some(tokens)
}

fn block(tokens: &mut std::iter::Peekable<std::vec::IntoIter<Token>>, nested: bool) -> Option<Vec<(String, Value)>> {
    let mut entries = Vec::new();
    loop {
        match tokens.next() {
            None => return (!nested).then_some(entries),
            Some(Token::Close) => return nested.then_some(entries),
            Some(Token::Open) => return None,
            Some(Token::Text(key)) => {
                let value = match tokens.next()? {
                    Token::Text(text) => Value::Text(text),
                    Token::Open => Value::Block(block(tokens, true)?),
                    Token::Close => return None,
                };
                entries.push((key, value));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_steams_library_folders() {
        let text = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"730"		"71589381210"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps"
		{
			"227300"		"40863153459"
		}
	}
}"#;
        let file = parse(text).unwrap();
        let folders = file.get("libraryfolders").unwrap();
        let second = folders.get("1").unwrap();
        assert_eq!(second.get("path").unwrap().text(), Some(r"D:\SteamLibrary"));
        assert!(second.get("apps").unwrap().get("227300").is_some());
        assert_eq!(folders.entries().len(), 2);
    }

    #[test]
    fn skips_comments_and_refuses_what_isnt_keyvalues() {
        let file = parse("// a comment\n\"AppState\" { \"installdir\" \"Euro Truck Simulator 2\" }").unwrap();
        assert_eq!(file.get("appstate").unwrap().get("InstallDir").unwrap().text(), Some("Euro Truck Simulator 2"));
        assert_eq!(parse("\"a\" { \"b\" "), None);
        assert_eq!(parse("\"a\" }"), None);
    }
}
