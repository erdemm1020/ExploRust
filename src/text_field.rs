use gpui::Keystroke;

/// What a key press did to a field.
#[derive(Debug, PartialEq, Eq)]
pub enum KeyResult {
    Enter,
    Escape,
    /// Text or caret changed.
    Edited,
    /// Not a text-editing key: the caller may use it.
    Ignored,
}

/// Minimal single-line text editing state: text, caret and selection.
/// Offsets are byte indexes, always on char boundaries.
pub struct TextField {
    text: String,
    anchor: usize,
    cursor: usize,
}

impl TextField {
    /// Starts with the file name selected, without its extension (like Finder).
    pub fn for_file_name(name: &str) -> Self {
        let stem_end = match name.rfind('.') {
            Some(i) if i > 0 => i,
            _ => name.len(),
        };
        Self {
            text: name.to_string(),
            anchor: 0,
            cursor: stem_end,
        }
    }

    pub fn empty() -> Self {
        Self {
            text: String::new(),
            anchor: 0,
            cursor: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn clear(&mut self) {
        *self = Self::empty();
    }

    /// Apply a key press. `paste` is only called for ⌘V.
    pub fn handle_key(
        &mut self,
        k: &Keystroke,
        paste: impl FnOnce() -> Option<String>,
    ) -> KeyResult {
        let (cmd, shift) = (k.modifiers.platform, k.modifiers.shift);
        match k.key.as_str() {
            "enter" => return KeyResult::Enter,
            "escape" => return KeyResult::Escape,
            "backspace" => self.backspace(),
            "delete" => self.delete(),
            "left" if cmd => self.home(shift),
            "right" if cmd => self.end(shift),
            "left" => self.left(shift),
            "right" => self.right(shift),
            "a" if cmd => self.select_all(),
            "v" if cmd => match paste() {
                Some(text) => self.insert(&text.replace('\n', " ")),
                None => return KeyResult::Ignored,
            },
            _ if !cmd && !k.modifiers.control => match k.key_char.as_deref() {
                Some(ch) if !ch.chars().any(char::is_control) => self.insert(ch),
                _ => return KeyResult::Ignored,
            },
            _ => return KeyResult::Ignored,
        }
        KeyResult::Edited
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Selected byte range, in order.
    pub fn selection(&self) -> (usize, usize) {
        (self.anchor.min(self.cursor), self.anchor.max(self.cursor))
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }

    /// Replace the selection (or insert at the caret).
    pub fn insert(&mut self, s: &str) {
        let (start, end) = self.selection();
        self.text.replace_range(start..end, s);
        self.cursor = start + s.len();
        self.anchor = self.cursor;
    }

    pub fn backspace(&mut self) {
        if self.anchor == self.cursor {
            self.anchor = self.prev(self.cursor);
        }
        self.insert("");
    }

    pub fn delete(&mut self) {
        if self.anchor == self.cursor {
            self.anchor = self.next(self.cursor);
            // `insert` works from the ordered selection, so the caret may sit on either end.
        }
        self.insert("");
    }

    pub fn left(&mut self, extend: bool) {
        let to = if !extend && self.anchor != self.cursor {
            self.selection().0
        } else {
            self.prev(self.cursor)
        };
        self.move_to(to, extend);
    }

    pub fn right(&mut self, extend: bool) {
        let to = if !extend && self.anchor != self.cursor {
            self.selection().1
        } else {
            self.next(self.cursor)
        };
        self.move_to(to, extend);
    }

    pub fn home(&mut self, extend: bool) {
        self.move_to(0, extend);
    }

    pub fn end(&mut self, extend: bool) {
        self.move_to(self.text.len(), extend);
    }

    fn move_to(&mut self, to: usize, extend: bool) {
        self.cursor = to;
        if !extend {
            self.anchor = to;
        }
    }

    fn prev(&self, i: usize) -> usize {
        self.text[..i]
            .char_indices()
            .next_back()
            .map_or(0, |(j, _)| j)
    }

    fn next(&self, i: usize) -> usize {
        self.text[i..]
            .chars()
            .next()
            .map_or(i, |c| i + c.len_utf8())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_stem_only() {
        let f = TextField::for_file_name("report.final.pdf");
        assert_eq!(f.selection(), (0, 12));
        let f = TextField::for_file_name(".gitignore");
        assert_eq!(f.selection(), (0, 10));
    }

    #[test]
    fn typing_replaces_selection() {
        let mut f = TextField::for_file_name("a.txt");
        f.insert("notes");
        assert_eq!(f.text(), "notes.txt");
        assert_eq!(f.cursor(), 5);
    }

    #[test]
    fn backspace_and_delete_handle_utf8() {
        let mut f = TextField::for_file_name("é");
        f.end(false);
        f.backspace();
        assert_eq!(f.text(), "");
        let mut f = TextField::for_file_name("éa");
        f.home(false);
        f.delete();
        assert_eq!(f.text(), "a");
    }

    fn key(spec: &str) -> Keystroke {
        Keystroke::parse(spec).unwrap()
    }

    #[test]
    fn handles_typing_editing_and_control_keys() {
        let mut f = TextField::empty();
        assert!(f.is_empty());
        for c in ["h", "i"] {
            let k = Keystroke {
                key_char: Some(c.into()),
                ..key(c)
            };
            assert_eq!(f.handle_key(&k, || None), KeyResult::Edited);
        }
        assert_eq!(f.text(), "hi");
        assert_eq!(f.handle_key(&key("backspace"), || None), KeyResult::Edited);
        assert_eq!(f.text(), "h");
        assert_eq!(
            f.handle_key(&key("cmd-v"), || Some("a\nb".into())),
            KeyResult::Edited
        );
        assert_eq!(f.text(), "ha b");
        assert_eq!(f.handle_key(&key("enter"), || None), KeyResult::Enter);
        assert_eq!(f.handle_key(&key("escape"), || None), KeyResult::Escape);
        assert_eq!(f.handle_key(&key("up"), || None), KeyResult::Ignored);
        assert_eq!(f.handle_key(&key("cmd-c"), || None), KeyResult::Ignored);
        f.clear();
        assert!(f.is_empty());
    }

    #[test]
    fn arrows_collapse_selection() {
        let mut f = TextField::for_file_name("abc.txt");
        f.left(false);
        assert_eq!(f.selection(), (0, 0));
        let mut f = TextField::for_file_name("abc.txt");
        f.right(false);
        assert_eq!(f.selection(), (3, 3));
    }
}
