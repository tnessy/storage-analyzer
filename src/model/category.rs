//! Extension to category table. One data table, easy to edit.

use super::{Category, ExtKey};

const TABLE: &[(Category, &[&str])] = &[
    (
        Category::Video,
        &["mp4", "mkv", "webm", "avi", "mov", "m4v"],
    ),
    (
        Category::Image,
        &[
            "jpg", "jpeg", "png", "gif", "webp", "heic", "svg", "raw", "cr2", "nef",
        ],
    ),
    (
        Category::Archive,
        &[
            "zip", "tar", "gz", "xz", "zst", "bz2", "7z", "rar", "iso", "pkg", "deb", "rpm",
        ],
    ),
    (
        Category::CodeText,
        &[
            "rs", "c", "h", "cpp", "py", "js", "ts", "go", "java", "md", "txt", "json", "toml",
            "yaml", "yml", "xml", "html", "css", "sh", "log", "csv",
        ],
    ),
    (
        Category::Binary,
        &["so", "a", "o", "bin", "exe", "dll", "elf", "class", "wasm"],
    ),
    (
        Category::Documents,
        &[
            "pdf", "doc", "docx", "odt", "xls", "xlsx", "ods", "ppt", "pptx", "epub",
        ],
    ),
    (
        Category::AudioOther,
        &["mp3", "flac", "ogg", "opus", "wav", "m4a"],
    ),
];

/// Files without an extension are mostly executables; everything unlisted is `AudioOther`.
pub fn category_of(ext: &ExtKey) -> Category {
    match ext {
        ExtKey::NoExt => Category::Binary,
        ExtKey::Other => Category::AudioOther,
        ExtKey::Ext(bytes) => TABLE
            .iter()
            .find(|(_, exts)| exts.iter().any(|e| e.as_bytes() == &**bytes))
            .map_or(Category::AudioOther, |(category, _)| *category),
    }
}
