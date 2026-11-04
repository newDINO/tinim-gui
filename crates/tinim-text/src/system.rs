//! Optional font discovery. Only compiled with the `system-fonts` feature.

use std::path::{Path, PathBuf};

/// Memory-maps a font file.
pub(crate) fn map_font_file(file: &std::fs::File) -> std::io::Result<memmap2::Mmap> {
    // SAFETY: the file is opened read-only and the mapping is read-only. As with any
    // memory map, replacing or truncating the file while it is mapped could fault, but
    // system font files are not modified while text is being rendered.
    unsafe { memmap2::MmapOptions::new().map(file) }
}

/// Returns every font file in the usual system font directories.
pub(crate) fn font_files() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    #[cfg(target_os = "linux")]
    {
        directories.push(PathBuf::from("/usr/share/fonts"));
        directories.push(PathBuf::from("/usr/local/share/fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            directories.push(home.join(".fonts"));
            directories.push(home.join(".local/share/fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        directories.push(PathBuf::from("/System/Library/Fonts"));
        directories.push(PathBuf::from("/Library/Fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            directories.push(PathBuf::from(home).join("Library/Fonts"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(windows) = std::env::var_os("WINDIR") {
            directories.push(PathBuf::from(windows).join("Fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            directories.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
    }

    let mut files = Vec::new();
    for directory in directories {
        collect(&directory, 0, &mut files);
    }
    files
}

fn collect(directory: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, depth + 1, files);
        } else if is_font_file(&path) {
            files.push(path);
        }
    }
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("ttf")
                || extension.eq_ignore_ascii_case("otf")
                || extension.eq_ignore_ascii_case("ttc")
                || extension.eq_ignore_ascii_case("otc")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_a_font_file() {
        // A regular file cannot take the safe path, so this also covers the unsafe fallback.
        let path = std::env::temp_dir().join(format!("tinim-map-{}.ttf", std::process::id()));
        std::fs::write(&path, font_test_data::AHEM).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let mapped = map_font_file(&file).unwrap();
        assert_eq!(&mapped[..], font_test_data::AHEM);

        drop(mapped);
        drop(file);
        std::fs::remove_file(&path).unwrap();
    }
}
