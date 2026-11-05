//! A small content-addressed cache for shaped text.
//!
//! The cache is keyed by the text plus the normalized layout inputs (layout style and wrap
//! width), not by a widget id. Paint-only inputs such as color are excluded, so identical text
//! can share one layout across different places and colors.

use std::collections::HashMap;

use tinim_text::{LayoutKey, TextLayout, TextStyle, TextSystem};

const DEFAULT_CAPACITY: usize = 256;

/// Caches shaped [`TextLayout`]s.
pub(crate) struct TextCache {
    entries: HashMap<Box<str>, Vec<Entry>>,
    len: usize,
    capacity: usize,
    clock: u64,
}

struct Entry {
    key: LayoutKey,
    layout: TextLayout,
    last_used: u64,
}

impl Default for TextCache {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl TextCache {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            len: 0,
            capacity: capacity.max(1),
            clock: 0,
        }
    }

    /// Shape `text`, reusing a cached layout when the inputs are unchanged.
    pub(crate) fn layout(
        &mut self,
        text_system: &TextSystem,
        text: &str,
        style: &TextStyle,
        max_width: Option<f32>,
    ) -> &TextLayout {
        self.clock = self.clock.wrapping_add(1);

        let key = LayoutKey::new(style, max_width);

        let existing = self
            .entries
            .get(text)
            .and_then(|entries| entries.iter().position(|entry| entry.key == key));

        if let Some(index) = existing {
            self.entries
                .get_mut(text)
                .expect("entry exists")
                .get_mut(index)
                .expect("layout index is valid")
                .last_used = self.clock;
        } else {
            let entry = Entry {
                key,
                layout: text_system.layout(text, style, max_width),
                last_used: self.clock,
            };
            self.entries.entry(text.into()).or_default().push(entry);
            self.len += 1;
            self.trim();
        }

        // The new entry has the newest clock, so trimming cannot evict it. Looking the layout up
        // once at the end yields a reference that borrows the cache.
        self.entries
            .get(text)
            .and_then(|entries| entries.iter().find(|entry| entry.key == key))
            .map(|entry| &entry.layout)
            .expect("requested layout is present after insert or lookup")
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.len = 0;
    }

    fn trim(&mut self) {
        while self.len > self.capacity {
            let Some((key, index)) = self.oldest() else {
                break;
            };

            let mut remove_key = false;
            if let Some(entries) = self.entries.get_mut(key.as_ref()) {
                entries.remove(index);
                remove_key = entries.is_empty();
            }
            if remove_key {
                self.entries.remove(key.as_ref());
            }
            self.len -= 1;
        }
    }

    fn oldest(&self) -> Option<(Box<str>, usize)> {
        let mut oldest: Option<(Box<str>, usize, u64)> = None;
        for (key, entries) in &self.entries {
            for (index, entry) in entries.iter().enumerate() {
                if oldest
                    .as_ref()
                    .is_none_or(|(_, _, last_used)| entry.last_used < *last_used)
                {
                    oldest = Some((key.clone(), index, entry.last_used));
                }
            }
        }
        oldest.map(|(key, index, _)| (key, index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinim_paint::{Color32, TextureId};
    use tinim_text::TextStyle;

    fn cache_with_font(capacity: usize) -> (TextCache, TextSystem) {
        let mut text = TextSystem::new(TextureId(1));
        text.add_font(font_test_data::AHEM.to_vec()).unwrap();
        (TextCache::new(capacity), text)
    }

    #[test]
    fn identical_requests_reuse_the_layout() {
        let (mut cache, text) = cache_with_font(8);
        let style = TextStyle::default();
        let a = cache.layout(&text, "hello", &style, Some(100.0)) as *const TextLayout;
        let b = cache.layout(&text, "hello", &style, Some(100.0)) as *const TextLayout;
        assert_eq!(a, b);
    }

    #[test]
    fn different_widths_are_different_entries() {
        let (mut cache, text) = cache_with_font(8);
        let style = TextStyle::default();
        // Insert both first so later lookups cannot move either entry.
        let _ = cache.layout(&text, "hello", &style, Some(100.0));
        let _ = cache.layout(&text, "hello", &style, Some(200.0));
        let a = cache.layout(&text, "hello", &style, Some(100.0)) as *const TextLayout;
        let b = cache.layout(&text, "hello", &style, Some(200.0)) as *const TextLayout;
        assert_ne!(a, b);
        assert_eq!(cache.len, 2);
    }

    #[test]
    fn capacity_evicts_old_entries() {
        let (mut cache, text) = cache_with_font(1);
        let style = TextStyle::default();
        let _ = cache.layout(&text, "one", &style, None);
        let _ = cache.layout(&text, "two", &style, None);
        assert_eq!(cache.len, 1);
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn color_only_change_reuses_the_layout() {
        let (mut cache, text) = cache_with_font(8);
        let mut style = TextStyle {
            color: Color32::BLACK,
            ..Default::default()
        };
        let a = cache.layout(&text, "hello", &style, Some(100.0)) as *const TextLayout;

        style.color = Color32::WHITE;
        let b = cache.layout(&text, "hello", &style, Some(100.0)) as *const TextLayout;

        assert_eq!(a, b);
        assert_eq!(cache.len, 1);
    }

    #[test]
    fn sanitized_equivalent_styles_reuse_the_layout() {
        let (mut cache, text) = cache_with_font(8);
        let sanitized = TextStyle {
            font_size: 12.0,
            line_height: None,
            letter_spacing: 0.0,
            ..Default::default()
        };
        let invalid = TextStyle {
            font_size: 0.0,
            line_height: Some(f32::NAN),
            letter_spacing: f32::NAN,
            ..Default::default()
        };

        let a = cache.layout(&text, "hello", &sanitized, Some(100.0)) as *const TextLayout;
        let b = cache.layout(&text, "hello", &invalid, Some(100.0)) as *const TextLayout;

        assert_eq!(a, b);
        assert_eq!(cache.len, 1);
    }

    #[test]
    fn equivalent_widths_reuse_the_layout() {
        let (mut cache, text) = cache_with_font(8);
        let style = TextStyle::default();

        let unlimited = cache.layout(&text, "hello", &style, None) as *const TextLayout;
        let infinity =
            cache.layout(&text, "hello", &style, Some(f32::INFINITY)) as *const TextLayout;
        assert_eq!(unlimited, infinity);

        let zero = cache.layout(&text, "hello", &style, Some(0.0)) as *const TextLayout;
        let negative = cache.layout(&text, "hello", &style, Some(-10.0)) as *const TextLayout;
        let nan = cache.layout(&text, "hello", &style, Some(f32::NAN)) as *const TextLayout;

        assert_eq!(zero, negative);
        assert_eq!(zero, nan);

        // Look the unlimited entry up again: inserting the zero-width entry may have moved it.
        let unlimited = cache.layout(&text, "hello", &style, None) as *const TextLayout;
        assert_ne!(unlimited, zero);
        assert_eq!(cache.len, 2);
    }
}
