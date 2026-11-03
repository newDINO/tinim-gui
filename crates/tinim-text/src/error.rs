//! Errors from loading fonts.

use std::fmt;

/// Something went wrong while reading a font file.
#[derive(Debug)]
pub enum FontError {
    /// The bytes are not a valid OpenType/TrueType font.
    InvalidFont,
    /// A font collection was given, but the requested face does not exist.
    InvalidIndex(u32),
    /// The font file could not be read from disk.
    Io(std::io::Error),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFont => f.write_str("invalid font data"),
            Self::InvalidIndex(index) => write!(f, "font collection has no face at index {index}"),
            Self::Io(err) => write!(f, "failed to read font: {err}"),
        }
    }
}

impl std::error::Error for FontError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for FontError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}
