use ratatui::style::{Color, Style};
use serde::Deserialize;

use self::de::deserialize_option_color_hex_string;
use crate::THEME;

#[inline]
pub fn style() -> Style {
    Style::default().bg(THEME.background())
}

macro_rules! def_theme_struct_with_defaults {
    ($($name:ident => $color:expr),+) => {
        #[derive(Debug, Clone, Copy, Deserialize)]
        pub struct Theme {
            $(
                #[serde(deserialize_with = "deserialize_option_color_hex_string")]
                #[serde(default)]
                $name: Option<Color>,
            )+
        }
        impl Theme {
            $(
                #[inline]
                pub fn $name(self) -> Color {
                    self.$name.unwrap_or($color)
                }
            )+
        }
        impl Default for Theme {
            fn default() -> Theme {
                Self {
                    $( $name: Some($color), )+
                }
            }
        }
    };
}

def_theme_struct_with_defaults!(
    background => Color::Reset,
    gray => Color::DarkGray,
    profit => Color::Green,
    loss => Color::Red,
    text_normal => Color::Reset,
    text_primary => Color::Yellow,
    text_secondary => Color::Cyan,
    border_primary => Color::Blue,
    border_secondary => Color::Reset,
    border_axis => Color::Blue,
    highlight_focused => Color::LightBlue,
    highlight_unfocused => Color::DarkGray
);

fn hex_to_color(hex: &str) -> Option<Color> {
    let hex = hex.strip_prefix('#')?;

    // Checking for hex digits up front also guarantees the slicing below
    // happens on character boundaries
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }

    let channel = |idx: usize| u8::from_str_radix(&hex[idx..idx + 2], 16).ok();

    Some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?))
}

mod de {
    use std::fmt;

    use serde::de::{self, Error, Unexpected, Visitor};

    use super::{hex_to_color, Color};

    pub(crate) fn deserialize_option_color_hex_string<'de, D>(
        deserializer: D,
    ) -> Result<Option<Color>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct ColorVisitor;

        impl Visitor<'_> for ColorVisitor {
            type Value = Option<Color>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a hex string in the format of '#09ACDF'")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                if let Some(color) = hex_to_color(s) {
                    return Ok(Some(color));
                }

                Err(de::Error::invalid_value(Unexpected::Str(s), &self))
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::hex_to_color;

    #[test]
    fn hex_to_color_parses_valid_colors() {
        assert_eq!(hex_to_color("#403E41"), Some(Color::Rgb(0x40, 0x3E, 0x41)));
        assert_eq!(hex_to_color("#fcfcfa"), Some(Color::Rgb(0xFC, 0xFC, 0xFA)));
    }

    #[test]
    fn hex_to_color_rejects_invalid_colors() {
        for hex in [
            "403E41", "#403E4", "#403E411", "#40 E41", "#+1+2+3", "#GGGGGG",
        ] {
            assert_eq!(hex_to_color(hex), None, "{}", hex);
        }
    }

    #[test]
    fn hex_to_color_rejects_multibyte_characters() {
        // 7 bytes long, previously panicked slicing inside the 'é'
        assert_eq!(hex_to_color("é12345"), None);
        assert_eq!(hex_to_color("#é1234"), None);
    }
}
