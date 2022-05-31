// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
use std::fmt;

/// Represents a clickable link containing text and url
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ClickableLink<'a> {
    text: &'a str,
    url: &'a str,
}

impl<'a> ClickableLink<'a> {
    /// Create a new link with a name and target URL, helpful to print clickable links in the terminal.
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
