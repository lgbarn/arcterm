//! Built-in colour palettes and user-override merging for Arcterm.
//!
//! A [`ColorPalette`] holds the 16 ANSI colours plus the three terminal
//! special colours (foreground, background, cursor).  Eight named palettes
//! are provided via [`ColorPalette::by_name`]; unknown names return `None`.
//! User overrides from [`crate::config::ColorOverrides`] are applied with
//! [`ColorPalette::with_overrides`].

use crate::config::ColorOverrides;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A complete terminal colour palette.
///
/// All colours are stored as `(r, g, b)` tuples of `u8` values (0–255).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorPalette {
    /// Default foreground colour.
    pub foreground: (u8, u8, u8),
    /// Default background colour.
    pub background: (u8, u8, u8),
    /// Cursor block colour.
    pub cursor: (u8, u8, u8),
    /// ANSI colours 0–15 (normal 0–7, bright 8–15).
    pub ansi: [(u8, u8, u8); 16],
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self::cool_night()
    }
}

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

impl ColorPalette {
    /// Return the named palette, or `None` if the name is unrecognised.
    ///
    /// Supported names (case-sensitive):
    /// `cool-night` (default), `catppuccin-mocha`, `dracula`, `solarized-dark`, `solarized-light`,
    /// `nord`, `tokyo-night`, `gruvbox-dark`, `one-dark`.
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "cool-night" => Some(Self::cool_night()),
            "catppuccin-mocha" => Some(Self::catppuccin_mocha()),
            "dracula" => Some(Self::dracula()),
            "solarized-dark" => Some(Self::solarized_dark()),
            "solarized-light" => Some(Self::solarized_light()),
            "nord" => Some(Self::nord()),
            "tokyo-night" => Some(Self::tokyo_night()),
            "gruvbox-dark" => Some(Self::gruvbox_dark()),
            "one-dark" => Some(Self::one_dark()),
            _ => None,
        }
    }

    /// Apply user overrides from a [`ColorOverrides`] struct.
    ///
    /// Each `Some(hex)` field is parsed as `"#rrggbb"` and written into the
    /// corresponding slot.  Invalid hex strings are silently ignored.
    pub fn with_overrides(mut self, overrides: &ColorOverrides) -> Self {
        macro_rules! apply {
            ($field:expr, $slot:expr) => {
                if let Some(ref hex) = $field {
                    if let Some(rgb) = parse_hex(hex) {
                        $slot = rgb;
                    }
                }
            };
        }

        apply!(overrides.black, self.ansi[0]);
        apply!(overrides.red, self.ansi[1]);
        apply!(overrides.green, self.ansi[2]);
        apply!(overrides.yellow, self.ansi[3]);
        apply!(overrides.blue, self.ansi[4]);
        apply!(overrides.magenta, self.ansi[5]);
        apply!(overrides.cyan, self.ansi[6]);
        apply!(overrides.white, self.ansi[7]);
        apply!(overrides.bright_black, self.ansi[8]);
        apply!(overrides.bright_red, self.ansi[9]);
        apply!(overrides.bright_green, self.ansi[10]);
        apply!(overrides.bright_yellow, self.ansi[11]);
        apply!(overrides.bright_blue, self.ansi[12]);
        apply!(overrides.bright_magenta, self.ansi[13]);
        apply!(overrides.bright_cyan, self.ansi[14]);
        apply!(overrides.bright_white, self.ansi[15]);
        apply!(overrides.foreground, self.foreground);
        apply!(overrides.background, self.background);
        apply!(overrides.cursor, self.cursor);

        self
    }
}

// ---------------------------------------------------------------------------
// Hex parsing
// ---------------------------------------------------------------------------

/// Parse `"#rrggbb"` → `Some((r, g, b))`.  Returns `None` for any other form.
fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

// ---------------------------------------------------------------------------
// Built-in palettes
// ---------------------------------------------------------------------------

impl ColorPalette {
    /// Catppuccin Mocha — dark, pastel-heavy.
    fn catppuccin_mocha() -> Self {
        Self {
            foreground: (0xcd, 0xd6, 0xf4), // text
            background: (0x1e, 0x1e, 0x2e), // base
            cursor: (0xf5, 0xe0, 0xdc),     // rosewater
            ansi: [
                (0x45, 0x47, 0x5a), // 0  black      (surface1)
                (0xf3, 0x8b, 0xa8), // 1  red
                (0xa6, 0xe3, 0xa1), // 2  green
                (0xf9, 0xe2, 0xaf), // 3  yellow
                (0x89, 0xb4, 0xfa), // 4  blue
                (0xcb, 0xa6, 0xf7), // 5  magenta    (mauve)
                (0x89, 0xdc, 0xeb), // 6  cyan       (sky)
                (0xba, 0xc2, 0xde), // 7  white      (subtext1)
                (0x58, 0x5b, 0x70), // 8  bright black (surface2)
                (0xf3, 0x8b, 0xa8), // 9  bright red
                (0xa6, 0xe3, 0xa1), // 10 bright green
                (0xf9, 0xe2, 0xaf), // 11 bright yellow
                (0x89, 0xb4, 0xfa), // 12 bright blue
                (0xcb, 0xa6, 0xf7), // 13 bright magenta
                (0x89, 0xdc, 0xeb), // 14 bright cyan
                (0xcd, 0xd6, 0xf4), // 15 bright white (text)
            ],
        }
    }

    /// Dracula — dark, vivid purple-accented.
    fn dracula() -> Self {
        Self {
            foreground: (0xf8, 0xf8, 0xf2),
            background: (0x28, 0x2a, 0x36),
            cursor: (0xf8, 0xf8, 0xf2),
            ansi: [
                (0x21, 0x22, 0x2c), // 0  black
                (0xff, 0x55, 0x55), // 1  red
                (0x50, 0xfa, 0x7b), // 2  green
                (0xf1, 0xfa, 0x8c), // 3  yellow
                (0xbd, 0x93, 0xf9), // 4  blue
                (0xff, 0x79, 0xc6), // 5  magenta
                (0x8b, 0xe9, 0xfd), // 6  cyan
                (0xf8, 0xf8, 0xf2), // 7  white
                (0x62, 0x72, 0xa4), // 8  bright black
                (0xff, 0x6e, 0x6e), // 9  bright red
                (0x69, 0xff, 0x94), // 10 bright green
                (0xff, 0xff, 0xa5), // 11 bright yellow
                (0xd6, 0xac, 0xff), // 12 bright blue
                (0xff, 0x92, 0xdf), // 13 bright magenta
                (0xa4, 0xff, 0xff), // 14 bright cyan
                (0xff, 0xff, 0xff), // 15 bright white
            ],
        }
    }

    /// Solarized Dark — warm, low-contrast dark.
    fn solarized_dark() -> Self {
        Self {
            foreground: (0x83, 0x94, 0x96),
            background: (0x00, 0x2b, 0x36),
            cursor: (0x83, 0x94, 0x96),
            ansi: [
                (0x07, 0x36, 0x42), // 0  black
                (0xdc, 0x32, 0x2f), // 1  red
                (0x85, 0x99, 0x00), // 2  green
                (0xb5, 0x89, 0x00), // 3  yellow
                (0x26, 0x8b, 0xd2), // 4  blue
                (0xd3, 0x36, 0x82), // 5  magenta
                (0x2a, 0xa1, 0x98), // 6  cyan
                (0xee, 0xe8, 0xd5), // 7  white
                (0x00, 0x2b, 0x36), // 8  bright black
                (0xcb, 0x4b, 0x16), // 9  bright red
                (0x58, 0x6e, 0x75), // 10 bright green
                (0x65, 0x7b, 0x83), // 11 bright yellow
                (0x83, 0x94, 0x96), // 12 bright blue
                (0x6c, 0x71, 0xc4), // 13 bright magenta
                (0x93, 0xa1, 0xa1), // 14 bright cyan
                (0xfd, 0xf6, 0xe3), // 15 bright white
            ],
        }
    }

    /// Solarized Light — warm, low-contrast light background.
    fn solarized_light() -> Self {
        Self {
            foreground: (0x65, 0x7b, 0x83),
            background: (0xfd, 0xf6, 0xe3),
            cursor: (0x58, 0x6e, 0x75),
            ansi: [
                (0x07, 0x36, 0x42), // 0  black
                (0xdc, 0x32, 0x2f), // 1  red
                (0x85, 0x99, 0x00), // 2  green
                (0xb5, 0x89, 0x00), // 3  yellow
                (0x26, 0x8b, 0xd2), // 4  blue
                (0xd3, 0x36, 0x82), // 5  magenta
                (0x2a, 0xa1, 0x98), // 6  cyan
                (0xee, 0xe8, 0xd5), // 7  white
                (0x00, 0x2b, 0x36), // 8  bright black
                (0xcb, 0x4b, 0x16), // 9  bright red
                (0x58, 0x6e, 0x75), // 10 bright green
                (0x65, 0x7b, 0x83), // 11 bright yellow
                (0x83, 0x94, 0x96), // 12 bright blue
                (0x6c, 0x71, 0xc4), // 13 bright magenta
                (0x93, 0xa1, 0xa1), // 14 bright cyan
                (0xfd, 0xf6, 0xe3), // 15 bright white
            ],
        }
    }

    /// Nord — dark, arctic blue-toned.
    fn nord() -> Self {
        Self {
            foreground: (0xd8, 0xde, 0xe9),
            background: (0x2e, 0x34, 0x40),
            cursor: (0xd8, 0xde, 0xe9),
            ansi: [
                (0x3b, 0x42, 0x52), // 0  black
                (0xbf, 0x61, 0x6a), // 1  red
                (0xa3, 0xbe, 0x8c), // 2  green
                (0xeb, 0xcb, 0x8b), // 3  yellow
                (0x81, 0xa1, 0xc1), // 4  blue
                (0xb4, 0x8e, 0xad), // 5  magenta
                (0x88, 0xc0, 0xd0), // 6  cyan
                (0xe5, 0xe9, 0xf0), // 7  white
                (0x4c, 0x56, 0x6a), // 8  bright black
                (0xbf, 0x61, 0x6a), // 9  bright red
                (0xa3, 0xbe, 0x8c), // 10 bright green
                (0xeb, 0xcb, 0x8b), // 11 bright yellow
                (0x81, 0xa1, 0xc1), // 12 bright blue
                (0xb4, 0x8e, 0xad), // 13 bright magenta
                (0x8f, 0xbc, 0xbb), // 14 bright cyan
                (0xec, 0xef, 0xf4), // 15 bright white
            ],
        }
    }

    /// Tokyo Night — dark, neon city aesthetic.
    fn tokyo_night() -> Self {
        Self {
            foreground: (0xc0, 0xca, 0xf5),
            background: (0x1a, 0x1b, 0x26),
            cursor: (0xc0, 0xca, 0xf5),
            ansi: [
                (0x15, 0x17, 0x1b), // 0  black
                (0xf7, 0x76, 0x8e), // 1  red
                (0x9e, 0xce, 0x6a), // 2  green
                (0xe0, 0xaf, 0x68), // 3  yellow
                (0x7a, 0xa2, 0xf7), // 4  blue
                (0xbb, 0x9a, 0xf7), // 5  magenta
                (0x7d, 0xcf, 0xff), // 6  cyan
                (0xa9, 0xb1, 0xd6), // 7  white
                (0x41, 0x44, 0x68), // 8  bright black
                (0xf7, 0x76, 0x8e), // 9  bright red
                (0x9e, 0xce, 0x6a), // 10 bright green
                (0xe0, 0xaf, 0x68), // 11 bright yellow
                (0x7a, 0xa2, 0xf7), // 12 bright blue
                (0xbb, 0x9a, 0xf7), // 13 bright magenta
                (0x7d, 0xcf, 0xff), // 14 bright cyan
                (0xc0, 0xca, 0xf5), // 15 bright white
            ],
        }
    }

    /// Gruvbox Dark — warm, retro amber tones.
    fn gruvbox_dark() -> Self {
        Self {
            foreground: (0xeb, 0xdb, 0xb2),
            background: (0x28, 0x28, 0x28),
            cursor: (0xeb, 0xdb, 0xb2),
            ansi: [
                (0x28, 0x28, 0x28), // 0  black
                (0xcc, 0x24, 0x1d), // 1  red
                (0x98, 0x97, 0x1a), // 2  green
                (0xd7, 0x99, 0x21), // 3  yellow
                (0x45, 0x85, 0x88), // 4  blue
                (0xb1, 0x62, 0x86), // 5  magenta
                (0x68, 0x9d, 0x6a), // 6  cyan
                (0xa8, 0x99, 0x84), // 7  white
                (0x92, 0x83, 0x74), // 8  bright black
                (0xfb, 0x49, 0x34), // 9  bright red
                (0xb8, 0xbb, 0x26), // 10 bright green
                (0xfa, 0xbd, 0x2f), // 11 bright yellow
                (0x83, 0xa5, 0x98), // 12 bright blue
                (0xd3, 0x86, 0x9b), // 13 bright magenta
                (0x8e, 0xc0, 0x7c), // 14 bright cyan
                (0xeb, 0xdb, 0xb2), // 15 bright white
            ],
        }
    }

    /// One Dark — Atom-inspired dark scheme.
    /// Cool-Night — deep blue-black, vibrant accents (from Ghostty/WezTerm).
    fn cool_night() -> Self {
        Self {
            foreground: (0xcb, 0xe0, 0xf0),
            background: (0x01, 0x14, 0x23),
            cursor: (0x47, 0xff, 0x9c),
            ansi: [
                (0x0b, 0x25, 0x3a), // 0  black
                (0xe8, 0x52, 0x52), // 1  red
                (0x44, 0xff, 0xb1), // 2  green
                (0xff, 0xd2, 0x7a), // 3  yellow
                (0x1f, 0xa8, 0xff), // 4  blue
                (0xa2, 0x77, 0xff), // 5  magenta
                (0x24, 0xea, 0xf7), // 6  cyan
                (0xbe, 0xe5, 0xff), // 7  white
                (0x4b, 0x6a, 0x87), // 8  bright black
                (0xff, 0x6b, 0x6b), // 9  bright red
                (0x67, 0xff, 0xc3), // 10 bright green
                (0xff, 0xe8, 0x9a), // 11 bright yellow
                (0x4d, 0xb5, 0xff), // 12 bright blue
                (0xc0, 0x9b, 0xff), // 13 bright magenta
                (0x5a, 0xed, 0xff), // 14 bright cyan
                (0xff, 0xff, 0xff), // 15 bright white
            ],
        }
    }

    fn one_dark() -> Self {
        Self {
            foreground: (0xab, 0xb2, 0xbf),
            background: (0x28, 0x2c, 0x34),
            cursor: (0x52, 0x8b, 0xff),
            ansi: [
                (0x28, 0x2c, 0x34), // 0  black
                (0xe0, 0x6c, 0x75), // 1  red
                (0x98, 0xc3, 0x79), // 2  green
                (0xe5, 0xc0, 0x7b), // 3  yellow
                (0x61, 0xaf, 0xef), // 4  blue
                (0xc6, 0x78, 0xdd), // 5  magenta
                (0x56, 0xb6, 0xc2), // 6  cyan
                (0xab, 0xb2, 0xbf), // 7  white
                (0x5c, 0x63, 0x70), // 8  bright black
                (0xe0, 0x6c, 0x75), // 9  bright red
                (0x98, 0xc3, 0x79), // 10 bright green
                (0xe5, 0xc0, 0x7b), // 11 bright yellow
                (0x61, 0xaf, 0xef), // 12 bright blue
                (0xc6, 0x78, 0xdd), // 13 bright magenta
                (0x56, 0xb6, 0xc2), // 14 bright cyan
                (0xab, 0xb2, 0xbf), // 15 bright white
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── by_name ──────────────────────────────────────────────────────────────

    #[test]
    fn by_name_known_schemes_return_some() {
        let names = [
            "cool-night",
            "catppuccin-mocha",
            "dracula",
            "solarized-dark",
            "solarized-light",
            "nord",
            "tokyo-night",
            "gruvbox-dark",
            "one-dark",
        ];
        for name in &names {
            assert!(
                ColorPalette::by_name(name).is_some(),
                "by_name({name:?}) should return Some"
            );
        }
    }

    #[test]
    fn by_name_unknown_returns_none() {
        assert!(ColorPalette::by_name("").is_none());
        assert!(ColorPalette::by_name("unknown-scheme").is_none());
        assert!(
            ColorPalette::by_name("Catppuccin-Mocha").is_none(),
            "case-sensitive"
        );
    }

    // ── default ──────────────────────────────────────────────────────────────

    #[test]
    fn default_is_cool_night() {
        let def = ColorPalette::default();
        let cool = ColorPalette::by_name("cool-night").unwrap();
        assert_eq!(def, cool, "Default must equal cool-night");
    }

    // ── all 8 palettes are distinct ───────────────────────────────────────────

    #[test]
    fn all_nine_palettes_are_distinct() {
        let names = [
            "cool-night",
            "catppuccin-mocha",
            "dracula",
            "solarized-dark",
            "solarized-light",
            "nord",
            "tokyo-night",
            "gruvbox-dark",
            "one-dark",
        ];
        let palettes: Vec<ColorPalette> = names
            .iter()
            .map(|n| ColorPalette::by_name(n).unwrap())
            .collect();

        for i in 0..palettes.len() {
            for j in (i + 1)..palettes.len() {
                assert_ne!(
                    palettes[i], palettes[j],
                    "palette {i} ({}) and {j} ({}) must differ",
                    names[i], names[j]
                );
            }
        }
    }

    // ── with_overrides ────────────────────────────────────────────────────────

    #[test]
    fn overrides_apply_foreground_background_cursor() {
        let overrides = ColorOverrides {
            foreground: Some("#ff0000".to_string()),
            background: Some("#00ff00".to_string()),
            cursor: Some("#0000ff".to_string()),
            ..Default::default()
        };
        let palette = ColorPalette::default().with_overrides(&overrides);
        assert_eq!(
            palette.foreground,
            (0xff, 0x00, 0x00),
            "foreground override"
        );
        assert_eq!(
            palette.background,
            (0x00, 0xff, 0x00),
            "background override"
        );
        assert_eq!(palette.cursor, (0x00, 0x00, 0xff), "cursor override");
    }

    #[test]
    fn overrides_apply_ansi_slots() {
        let overrides = ColorOverrides {
            red: Some("#ff5555".to_string()),
            bright_white: Some("#eeeeee".to_string()),
            ..Default::default()
        };
        let palette = ColorPalette::default().with_overrides(&overrides);
        assert_eq!(palette.ansi[1], (0xff, 0x55, 0x55), "ansi[1] red override");
        assert_eq!(
            palette.ansi[15],
            (0xee, 0xee, 0xee),
            "ansi[15] bright_white override"
        );
    }

    #[test]
    fn invalid_hex_override_is_ignored() {
        let original = ColorPalette::default();
        let overrides = ColorOverrides {
            foreground: Some("not-a-color".to_string()),
            red: Some("#zzzzzz".to_string()),
            ..Default::default()
        };
        let palette = original.clone().with_overrides(&overrides);
        // Invalid entries must not change the palette.
        assert_eq!(
            palette.foreground, original.foreground,
            "bad hex leaves foreground unchanged"
        );
        assert_eq!(
            palette.ansi[1], original.ansi[1],
            "bad hex leaves ansi[1] unchanged"
        );
    }

    #[test]
    fn no_overrides_leaves_palette_unchanged() {
        let original = ColorPalette::default();
        let overrides = ColorOverrides::default();
        let palette = original.clone().with_overrides(&overrides);
        assert_eq!(palette, original, "empty overrides must not modify palette");
    }

    // ── parse_hex corner cases ────────────────────────────────────────────────

    #[test]
    fn parse_hex_valid() {
        assert_eq!(super::parse_hex("#1e1e2e"), Some((0x1e, 0x1e, 0x2e)));
        assert_eq!(super::parse_hex("#ffffff"), Some((0xff, 0xff, 0xff)));
        assert_eq!(super::parse_hex("#000000"), Some((0x00, 0x00, 0x00)));
    }

    #[test]
    fn parse_hex_invalid() {
        assert_eq!(super::parse_hex("1e1e2e"), None, "missing #");
        assert_eq!(super::parse_hex("#1e1e2"), None, "too short");
        assert_eq!(super::parse_hex("#1e1e2e0"), None, "too long");
        assert_eq!(super::parse_hex("#gggggg"), None, "invalid hex digits");
        assert_eq!(super::parse_hex(""), None, "empty string");
    }
}
