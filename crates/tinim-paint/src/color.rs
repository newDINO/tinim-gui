//! Colors, stored as sRGB bytes with straight (non-premultiplied) alpha.

/// A color in sRGB space with straight alpha.
///
/// This is the format widgets and shapes are authored in. Conversion to linear space happens in
/// the shader, right before blending, so that alpha compositing is physically correct.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Color32(pub [u8; 4]);

impl Color32 {
    pub const TRANSPARENT: Self = Self([0, 0, 0, 0]);
    pub const BLACK: Self = Self([0, 0, 0, 255]);
    pub const WHITE: Self = Self([255, 255, 255, 255]);

    #[inline]
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b, 255])
    }

    #[inline]
    pub const fn from_rgba_unmultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self([r, g, b, a])
    }

    #[inline]
    pub const fn from_gray(gray: u8) -> Self {
        Self([gray, gray, gray, 255])
    }

    #[inline]
    pub const fn to_array(self) -> [u8; 4] {
        self.0
    }

    #[inline]
    pub const fn is_opaque(self) -> bool {
        self.0[3] == 255
    }

    #[inline]
    pub const fn is_invisible(self) -> bool {
        self.0[3] == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_transparent() {
        assert_eq!(Color32::default(), Color32::TRANSPARENT);
        assert!(Color32::default().is_invisible());
        assert!(Color32::WHITE.is_opaque());
    }
}
