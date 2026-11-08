//! A small, pure shelf packer for 2D rectangles.
//!
//! The packer has no knowledge of glyphs or textures. It only tracks shelves and returns
//! top-left positions for axis-aligned rectangles. [`crate::atlas::Atlas`] wraps it and reserves
//! the white texel at `(0, 0)`.

#[derive(Clone, Copy, Debug)]
struct Shelf {
    /// Top edge of the shelf.
    y: u32,
    /// Height of the tallest rectangle placed on the shelf.
    height: u32,
    /// X coordinate where the next rectangle may start.
    x: u32,
}

/// Packs rectangles into shelves and doubles its dimensions when it fills up.
#[derive(Clone, Debug)]
pub(crate) struct ShelfPacker {
    width: u32,
    height: u32,
    /// Largest side the packer may grow to.
    max_size: u32,
    /// Space kept to the right and below every rectangle.
    padding: u32,
    shelves: Vec<Shelf>,
    next_y: u32,
}

impl ShelfPacker {
    pub(crate) fn new(width: u32, height: u32, max_size: u32, padding: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let max_size = max_size.max(width).max(height);
        Self {
            width,
            height,
            max_size,
            padding,
            shelves: Vec::new(),
            next_y: 0,
        }
    }

    pub(crate) fn width(&self) -> u32 {
        self.width
    }

    pub(crate) fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn max_size(&self) -> u32 {
        self.max_size
    }

    pub(crate) fn padding(&self) -> u32 {
        self.padding
    }

    /// Places a rectangle at the top-left corner.
    ///
    /// Only valid on an empty packer, which is how the atlas reserves its white texel after
    /// construction, growth and reset. Returns `None` if the rectangle does not fit or the packer
    /// already contains something.
    pub(crate) fn reserve_top_left(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if !self.shelves.is_empty() || self.next_y != 0 {
            return None;
        }
        if width == 0 || height == 0 {
            return None;
        }
        if width.saturating_add(self.padding) > self.width
            || height.saturating_add(self.padding) > self.height
        {
            return None;
        }

        self.shelves.push(Shelf {
            y: 0,
            height,
            x: width.saturating_add(self.padding),
        });
        self.next_y = height.saturating_add(self.padding);
        Some((0, 0))
    }

    /// Allocates a rectangle and returns its top-left position.
    ///
    /// Returns `None` when there is no room left; the caller is expected to grow or reset the
    /// packer and try again.
    pub(crate) fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        if width == 0 || height == 0 {
            return None;
        }
        if width.saturating_add(self.padding) > self.width
            || height.saturating_add(self.padding) > self.height
        {
            return None;
        }

        for shelf in &mut self.shelves {
            if shelf.height >= height
                && shelf.x.saturating_add(width).saturating_add(self.padding) <= self.width
            {
                let position = (shelf.x, shelf.y);
                shelf.x = shelf.x.saturating_add(width).saturating_add(self.padding);
                return Some(position);
            }
        }

        let y = self.next_y;
        if y.saturating_add(height).saturating_add(self.padding) > self.height {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height,
            x: width.saturating_add(self.padding),
        });
        self.next_y = y.saturating_add(height).saturating_add(self.padding);
        Some((0, y))
    }

    /// Whether the packer can still double its dimensions.
    pub(crate) fn can_grow(&self) -> bool {
        let current = self.width.max(self.height);
        current.saturating_mul(2).min(self.max_size) > current
    }

    /// Doubles the dimensions and forgets every shelf.
    ///
    /// Returns `false` when the packer has already reached its maximum size.
    pub(crate) fn grow(&mut self) -> bool {
        let current = self.width.max(self.height);
        let next = current.saturating_mul(2).min(self.max_size);
        if next <= current {
            return false;
        }

        self.width = next;
        self.height = next;
        self.shelves.clear();
        self.next_y = 0;
        true
    }

    /// Forgets every shelf, keeping the current dimensions.
    pub(crate) fn reset(&mut self) {
        self.shelves.clear();
        self.next_y = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns `true` if the two half-open rectangles do not overlap.
    fn disjoint(a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)) -> bool {
        let (ax, ay, aw, ah) = a;
        let (bx, by, bw, bh) = b;
        ax + aw <= bx || bx + bw <= ax || ay + ah <= by || by + bh <= ay
    }

    #[test]
    fn reserve_top_left_occupies_the_corner_with_padding() {
        let mut packer = ShelfPacker::new(16, 16, 16, 1);
        assert_eq!(packer.reserve_top_left(1, 1), Some((0, 0)));

        // The white texel keeps one pixel of padding on its right and bottom, so the first
        // rectangle starts on a new shelf below it.
        assert_eq!(packer.allocate(4, 4), Some((0, 2)));
        // The next rectangle fits on the same shelf, to the right.
        assert_eq!(packer.allocate(4, 4), Some((5, 2)));
    }

    #[test]
    fn reserved_rectangles_do_not_overlap() {
        let mut packer = ShelfPacker::new(32, 32, 32, 1);
        assert_eq!(packer.reserve_top_left(1, 1), Some((0, 0)));

        let white = (0, 0, 1, 1);
        for _ in 0..8 {
            let Some((x, y)) = packer.allocate(5, 5) else {
                break;
            };
            assert!(disjoint(white, (x, y, 5, 5)));
            // Padding on the right and bottom must stay clear as well.
            assert!(x >= 2 || y >= 2);
        }
    }

    #[test]
    fn reserve_top_left_requires_an_empty_packer() {
        let mut packer = ShelfPacker::new(16, 16, 16, 1);
        assert_eq!(packer.reserve_top_left(1, 1), Some((0, 0)));
        assert_eq!(packer.reserve_top_left(1, 1), None);
    }

    #[test]
    fn grow_doubles_until_the_maximum() {
        let mut packer = ShelfPacker::new(4, 4, 8, 1);
        assert!(packer.can_grow());
        assert_eq!(packer.reserve_top_left(1, 1), Some((0, 0)));

        assert!(packer.grow());
        assert_eq!((packer.width(), packer.height()), (8, 8));
        assert!(!packer.can_grow());

        assert_eq!(packer.reserve_top_left(1, 1), Some((0, 0)));
        assert_eq!(packer.allocate(2, 2), Some((0, 2)));
        assert!(!packer.grow(), "already at the maximum size");
    }

    #[test]
    fn reset_forgets_shelves_but_keeps_the_size() {
        let mut packer = ShelfPacker::new(16, 16, 16, 1);
        assert_eq!(packer.reserve_top_left(1, 1), Some((0, 0)));
        assert_eq!(packer.allocate(4, 4), Some((0, 2)));

        packer.reset();
        assert_eq!((packer.width(), packer.height()), (16, 16));
        assert_eq!(packer.reserve_top_left(1, 1), Some((0, 0)));
        assert_eq!(packer.allocate(4, 4), Some((0, 2)));
    }

    #[test]
    fn zero_sized_rectangles_are_rejected() {
        let mut packer = ShelfPacker::new(16, 16, 16, 1);
        assert_eq!(packer.allocate(0, 4), None);
        assert_eq!(packer.allocate(4, 0), None);
        assert_eq!(packer.reserve_top_left(0, 1), None);
    }
}
