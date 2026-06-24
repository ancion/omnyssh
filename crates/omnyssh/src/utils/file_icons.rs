//! Nerd-font file type icons for the File Manager screen.
//!
//! Maps a file or directory entry to a single-glyph icon from a Nerd Font
//! (https://www.nerdfonts.com/). If the user does not have a Nerd Font
//! installed the glyphs render as empty boxes — the file manager still
//! works, it just looks plain. The README/install docs mention Nerd Font as
//! a recommendation for the File Manager.
//!
//! Icon set: Font Awesome extension private-use area, which is bundled in
//! every popular Nerd Font (Hack, FiraCode, Meslo, Iosevka, JetBrainsMono,
//! …).

/// Returns the Nerd Font icon glyph for a given file or directory name.
///
/// `is_dir` is honoured first so the caller can pass any entry; for `..` we
/// return the open-folder glyph so the parent entry stays visually distinct.
pub fn icon_for(name: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return if name == ".." { DIR_OPEN } else { DIR_CLOSED };
    }

    let ext_lower: String = match name.rfind('.') {
        // Hidden dotfiles like `.bashrc` have no extension — fall through
        // to the generic glyph.
        Some(0) => return GENERIC,
        Some(idx) => name[idx + 1..].to_ascii_lowercase(),
        None => return GENERIC,
    };

    icon_by_ext(&ext_lower)
}

fn icon_by_ext(ext: &str) -> &'static str {
    match ext {
        // ── text / markup / data ──────────────────────────────────────
        "txt"  | "md" | "rst" | "log" | "json" | "yaml" | "yml"
        | "toml" | "ini" | "conf" | "cfg" | "xml" | "proto" => FILE_TEXT_O,

        "csv" | "tsv" => FILE_CSV,

        // ── shell / scripts ───────────────────────────────────────────
        "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" => CONSOLE,

        // ── source code ───────────────────────────────────────────────
        "rs"        => SETI_RUST,
        "py"        => SETI_PYTHON,
        "js" | "jsx" | "mjs" | "cjs" => SETI_JS,
        "ts" | "tsx" => SETI_TS,
        "c"  | "h"  => SETI_C,
        "cpp" | "hpp" | "cc" | "hh" => SETI_CPP,
        "java" | "jar" | "war" => SETI_JAVA,
        "kt" | "kts" => SETI_KOTLIN,
        "go"        => SETI_GO,
        "rb"        => SETI_RUBY,
        "php"       => SETI_PHP,
        "pl"        => SETI_PERL,
        "lua"       => SETI_LUA,
        "swift"     => SETI_SWIFT,
        "scala"     => SETI_SCALA,
        "dart"      => SETI_DART,
        "hs"        => SETI_HASKELL,
        "ex" | "exs" => SETI_ELIXIR,
        "erl"       => SETI_ERLANG,
        "elm"       => SETI_ELM,
        "r"         => SETI_RLANG,
        "jl"        => SETI_JULIA,
        "sql"       => SETI_DB,
        "graphql"   => SETI_GRAPHQL,

        // ── secrets / keys ────────────────────────────────────────────
        "lock" => LOCK,
        "pem" | "key" | "crt" | "cer" | "pub" => KEY,

        // ── web ───────────────────────────────────────────────────────
        "html" | "htm" => SETI_HTML,
        "css"  | "scss" | "sass" | "less" => SETI_CSS,
        "vue"  => SETI_VUE,
        "svelte" => SETI_SVELTE,

        // ── images ────────────────────────────────────────────────────
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp"
        | "ico" | "tif" | "tiff" | "eps" | "fig" => FILE_IMAGE_O,

        "ai" => SETI_ILLUSTRATOR,

        // ── video ─────────────────────────────────────────────────────
        "mp4" | "mkv" | "mov" | "avi" | "webm" | "flv" | "wmv" | "m4v"
            => FILE_VIDEO_O,

        // ── audio ─────────────────────────────────────────────────────
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus"
            => FILE_AUDIO_O,

        // ── archives / packages ───────────────────────────────────────
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" | "tgz"
        | "tbz" | "txz" | "deb" | "rpm" | "apk" | "iso" | "dmg"
            => FILE_ARCHIVE_O,

        // ── documents ─────────────────────────────────────────────────
        "pdf" => FILE_PDF_O,
        "doc" | "docx" | "odt" | "rtf" => FILE_WORD_O,
        "xls" | "xlsx" | "ods" | "ppt" | "pptx" | "odp" => FILE_EXCEL_O,

        // ── executables / libraries ───────────────────────────────────
        "exe" | "msi" | "app" | "dll" | "so" | "dylib" | "bin" => ROCKET,

        // ── fonts ─────────────────────────────────────────────────────
        "ttf" | "otf" | "woff" | "woff2" | "eot" => FONT,

        // ── everything else ───────────────────────────────────────────
        _ => GENERIC,
    }
}

// ---------------------------------------------------------------------------
// Glyphs (Font Awesome private-use area)
// ---------------------------------------------------------------------------

const GENERIC: &str = "\u{f15b}"; //  nf-fa-file-o
const FILE_TEXT_O: &str = "\u{f15c}"; //  nf-fa-file-text-o
const FILE_CSV: &str = "\u{f1c3}"; //  nf-fa-file-csv-o
const FILE_IMAGE_O: &str = "\u{f1c5}"; //  nf-fa-file-image-o
const FILE_VIDEO_O: &str = "\u{f1c8}"; //  nf-fa-file-video-o
const FILE_AUDIO_O: &str = "\u{f1c7}"; //  nf-fa-file-audio-o
const FILE_ARCHIVE_O: &str = "\u{f1c6}"; //  nf-fa-file-archive-o
const FILE_PDF_O: &str = "\u{f1c1}"; //  nf-fa-file-pdf-o
const FILE_WORD_O: &str = "\u{f1c2}"; //  nf-fa-file-word-o
const FILE_EXCEL_O: &str = "\u{f1c4}"; //  nf-fa-file-excel-o
const DIR_CLOSED: &str = "\u{f07b}"; //  nf-fa-folder
const DIR_OPEN: &str = "\u{f07c}"; //  nf-fa-folder-open
const KEY: &str = "\u{f084}"; //  nf-fa-key
const LOCK: &str = "\u{f023}"; //  nf-fa-lock
const ROCKET: &str = "\u{f135}"; //  nf-fa-rocket
const FONT: &str = "\u{f031}"; //  nf-fa-font
const CONSOLE: &str = "\u{f489}"; //  nf-md-console

// Seti-UI (also in every Nerd Font) — used for language-specific icons.
const SETI_RUST: &str = "\u{e7a8}";
const SETI_PYTHON: &str = "\u{e73c}";
const SETI_JS: &str = "\u{e74e}";
const SETI_TS: &str = "\u{e628}";
const SETI_C: &str = "\u{e61e}";
const SETI_CPP: &str = "\u{e61d}";
const SETI_JAVA: &str = "\u{e738}";
const SETI_KOTLIN: &str = "\u{e634}";
const SETI_GO: &str = "\u{e626}";
const SETI_RUBY: &str = "\u{e739}";
const SETI_PHP: &str = "\u{e73d}";
const SETI_PERL: &str = "\u{e67e}";
const SETI_LUA: &str = "\u{e620}";
const SETI_SWIFT: &str = "\u{e755}";
const SETI_SCALA: &str = "\u{e737}";
const SETI_DART: &str = "\u{e798}";
const SETI_HASKELL: &str = "\u{e61f}";
const SETI_ELIXIR: &str = "\u{e62d}";
const SETI_ERLANG: &str = "\u{e62b}";
const SETI_ELM: &str = "\u{e62c}";
const SETI_RLANG: &str = "\u{e68a}";
const SETI_JULIA: &str = "\u{e624}";
const SETI_DB: &str = "\u{e706}";
const SETI_GRAPHQL: &str = "\u{e662}";
const SETI_HTML: &str = "\u{e736}";
const SETI_CSS: &str = "\u{e749}";
const SETI_VUE: &str = "\u{e6a0}";
const SETI_SVELTE: &str = "\u{e6a1}";
const SETI_ILLUSTRATOR: &str = "\u{e7b4}";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_get_folder_glyph() {
        assert_eq!(icon_for("projects", true), DIR_CLOSED);
        assert_eq!(icon_for("..", true), DIR_OPEN);
    }

    #[test]
    fn extension_lookup_is_case_insensitive() {
        assert_eq!(icon_for("README.TXT", false), icon_for("readme.txt", false));
        assert_eq!(icon_for("PHOTO.JPG", false), icon_for("photo.jpg", false));
    }

    #[test]
    fn hidden_dotfiles_get_generic_glyph() {
        assert_eq!(icon_for(".bashrc", false), GENERIC);
        assert_eq!(icon_for(".profile", false), GENERIC);
    }

    #[test]
    fn unknown_extension_falls_back_to_generic() {
        assert_eq!(icon_for("weirdo.zzz", false), GENERIC);
    }

    #[test]
    fn common_languages_resolve() {
        assert!(!icon_for("main.rs", false).is_empty());
        assert!(!icon_for("app.py", false).is_empty());
        assert!(!icon_for("index.js", false).is_empty());
    }

    #[test]
    fn no_extension_file_is_generic() {
        assert_eq!(icon_for("Makefile", false), GENERIC);
    }
}
