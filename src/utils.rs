use std::fmt;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ClickableLink<'a> {
    text: &'a str,
    url: &'a str,
}

impl<'a> ClickableLink<'a> {
    /// Create a new link with a name and target url.
    pub fn new(text: &'a str, url: &'a str) -> Self {
        Self { text, url }
    }
}

impl fmt::Display for ClickableLink<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\u{1b}]8;;{}\u{1b}\\{}\u{1b}]8;;\u{1b}\\",
            self.url, self.text
        )
    }
}
