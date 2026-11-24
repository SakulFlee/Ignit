//! Single-line/multi-line text editing model.

use ignit_ui::{UiKey, UiModifiers};

use crate::shape::ShapedText;

/// Clipboard abstraction so tests and headless builds can run without a
/// display server.
pub trait UiClipboard {
    fn get_text(&mut self) -> Option<String>;
    fn set_text(&mut self, text: String);
}

/// `arboard`-backed clipboard. Construction may fail without a display; the
/// editor still works with copy/cut/paste as no-ops.
pub struct SystemClipboard {
    inner: Option<arboard::Clipboard>,
}

impl SystemClipboard {
    pub fn new() -> Self {
        Self {
            inner: arboard::Clipboard::new().ok(),
        }
    }
}

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl UiClipboard for SystemClipboard {
    fn get_text(&mut self) -> Option<String> {
        self.inner.as_mut()?.get_text().ok()
    }

    fn set_text(&mut self, text: String) {
        if let Some(clipboard) = self.inner.as_mut() {
            let _ = clipboard.set_text(text);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TextEditState {
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
    pub scroll_x: f32,
    pub multiline: bool,
    pub max_width: f32,
}

impl TextEditState {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            anchor: cursor,
            scroll_x: 0.0,
            multiline: false,
            max_width: 0.0,
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.anchor = self.cursor;
        self.scroll_x = 0.0;
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        (self.cursor != self.anchor)
            .then_some((self.cursor.min(self.anchor), self.cursor.max(self.anchor)))
    }

    pub fn selected_text(&self) -> &str {
        self.selection_range()
            .map(|(start, end)| &self.text[start..end])
            .unwrap_or("")
    }

    pub fn insert_text(&mut self, value: &str) {
        self.delete_selection();
        self.text.insert_str(self.cursor, value);
        self.cursor += value.len();
        self.anchor = self.cursor;
    }

    pub fn apply_text_input(&mut self, value: &str) {
        if !value.is_empty() {
            self.insert_text(value);
        }
    }

    pub fn backspace(&mut self) {
        if self.selection_range().is_some() {
            self.delete_selection();
            return;
        }
        if self.cursor == 0 {
            return;
        }
        let start = self.prev_boundary(self.cursor);
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
        self.anchor = start;
    }

    pub fn delete(&mut self) {
        if self.selection_range().is_some() {
            self.delete_selection();
            return;
        }
        if self.cursor >= self.text.len() {
            return;
        }
        let end = self.next_boundary(self.cursor);
        self.text.replace_range(self.cursor..end, "");
        self.anchor = self.cursor;
    }

    pub fn delete_selection(&mut self) {
        if let Some((start, end)) = self.selection_range() {
            self.text.replace_range(start..end, "");
            self.cursor = start;
            self.anchor = start;
        }
    }

    pub fn move_left(&mut self, select: bool) {
        let next = self.prev_boundary(self.cursor);
        self.move_to(next, select);
    }

    pub fn move_right(&mut self, select: bool) {
        let next = self.next_boundary(self.cursor);
        self.move_to(next, select);
    }

    pub fn move_word_left(&mut self, select: bool) {
        let mut index = self.cursor;
        while index > 0 {
            let prev = self.prev_boundary(index);
            if self.text[prev..index].chars().all(char::is_whitespace) {
                index = prev;
            } else {
                break;
            }
        }
        while index > 0 {
            let prev = self.prev_boundary(index);
            if self.text[prev..index].chars().all(char::is_whitespace) {
                break;
            }
            index = prev;
        }
        self.move_to(index, select);
    }

    pub fn move_word_right(&mut self, select: bool) {
        let mut index = self.cursor;
        while index < self.text.len() {
            let next = self.next_boundary(index);
            if self.text[index..next].chars().all(char::is_whitespace) {
                index = next;
            } else {
                break;
            }
        }
        while index < self.text.len() {
            let next = self.next_boundary(index);
            if self.text[index..next].chars().all(char::is_whitespace) {
                break;
            }
            index = next;
        }
        self.move_to(index, select);
    }

    pub fn home(&mut self, select: bool) {
        let start = self.text[..self.cursor]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        self.move_to(start, select);
    }

    pub fn end(&mut self, select: bool) {
        let end = self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |index| self.cursor + index);
        self.move_to(end, select);
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }

    pub fn copy(&mut self, clipboard: &mut dyn UiClipboard) {
        if let Some((start, end)) = self.selection_range() {
            clipboard.set_text(self.text[start..end].to_string());
        }
    }

    pub fn cut(&mut self, clipboard: &mut dyn UiClipboard) {
        if let Some((start, end)) = self.selection_range() {
            clipboard.set_text(self.text[start..end].to_string());
            self.delete_selection();
        }
    }

    pub fn paste(&mut self, clipboard: &mut dyn UiClipboard) {
        if let Some(text) = clipboard.get_text() {
            self.insert_text(&text);
        }
    }

    /// Returns `true` when Enter should submit the field.
    pub fn handle_key(&mut self, key: UiKey, modifiers: UiModifiers) -> bool {
        let select = modifiers.shift;
        match key {
            UiKey::Character(character) if !modifiers.ctrl && !modifiers.super_key => {
                let mut buffer = [0u8; 4];
                self.apply_text_input(character.encode_utf8(&mut buffer));
            }
            UiKey::Space if !modifiers.ctrl && !modifiers.super_key => {
                self.apply_text_input(" ");
            }
            UiKey::Backspace => self.backspace(),
            UiKey::Delete => self.delete(),
            UiKey::ArrowLeft if modifiers.ctrl => self.move_word_left(select),
            UiKey::ArrowRight if modifiers.ctrl => self.move_word_right(select),
            UiKey::ArrowLeft => self.move_left(select),
            UiKey::ArrowRight => self.move_right(select),
            UiKey::Home => self.home(select),
            UiKey::End => self.end(select),
            UiKey::Enter => return true,
            UiKey::Escape | UiKey::Tab => {}
            UiKey::ArrowUp | UiKey::ArrowDown => {}
            UiKey::Character(_) | UiKey::Space => {}
        }
        false
    }

    pub fn set_cursor_from_hit(
        &mut self,
        shaped: &ShapedText,
        engine: &mut crate::TextEngine,
        x: f32,
        y: f32,
    ) {
        self.cursor = engine.hit_to_cursor(shaped, x, y);
        self.anchor = self.cursor;
    }

    pub fn cursor_rect(
        &self,
        shaped: &ShapedText,
        engine: &mut crate::TextEngine,
    ) -> Option<[f32; 4]> {
        let (x, y) = engine.cursor_position(shaped, self.cursor)?;
        let height = shaped
            .lines
            .iter()
            .find(|line| self.cursor >= line.start && self.cursor <= line.end)
            .map_or(16.0, |line| line.height);
        Some([x, y, if self.multiline { 2.0 } else { 1.5 }, height])
    }

    fn move_to(&mut self, index: usize, select: bool) {
        self.cursor = index;
        if !select {
            self.anchor = index;
        }
    }

    fn prev_boundary(&self, index: usize) -> usize {
        self.text[..index]
            .char_indices()
            .next_back()
            .map_or(0, |(offset, _)| offset)
    }

    fn next_boundary(&self, index: usize) -> usize {
        self.text[index..]
            .char_indices()
            .nth(1)
            .map_or(self.text.len(), |(offset, _)| index + offset)
    }
}
