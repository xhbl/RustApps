use ratatui::style::Color;
use term_color_support::ColorSupport;

/// A trait to extend Ratatui's Color with cross-platform consistency methods.
pub trait WTMatch {
    /// Adjusts the color to match the Windows Terminal (Campbell) visual style 
    /// based on the current terminal's color capabilities.
    fn wtmatch(self) -> Color;
}

impl WTMatch for Color {
    fn wtmatch(self) -> Color {
        // Detect terminal color support (TrueColor, 256, or Basic)
        let support = ColorSupport::stdout();

        // Mapping table based on Windows Terminal "Campbell" RGB values.
        // Format: Some(((R, G, B), ANSI_256_Index))
        let mapping = match self {
            Color::Black =>         Some(((12, 12, 12),   232)),
            Color::Red =>           Some(((197, 15, 31),  160)),
            Color::Green =>         Some(((19, 161, 14),  28)),
            Color::Yellow =>        Some(((193, 156, 0),  178)),
            Color::Blue =>          Some(((0, 55, 218),   20)),
            Color::Magenta =>       Some(((136, 23, 152), 90)),
            Color::Cyan =>          Some(((58, 150, 221), 38)),
            Color::Gray =>          Some(((204, 204, 204), 250)),
            Color::DarkGray =>      Some(((118, 118, 118), 243)),
            Color::LightRed =>      Some(((231, 72, 86),  203)),
            Color::LightGreen =>    Some(((22, 198, 12),  46)),
            Color::LightYellow =>   Some(((249, 241, 165), 229)),
            Color::LightBlue =>     Some(((59, 120, 255), 63)),
            Color::LightMagenta =>  Some(((180, 0, 158),  163)),
            Color::LightCyan =>     Some(((97, 214, 214), 116)),
            Color::White =>         Some(((242, 242, 242), 255)),
            _ => None, // Custom RGB or Indexed colors are returned as-is
        };

        match mapping {
            Some((rgb, index256)) => {
                if support.has_16m {
                    // 1. TrueColor support: Return the exact sampled RGB value
                    Color::Rgb(rgb.0, rgb.1, rgb.2)
                } else if support.has_256 {
                    // 2. 256-color support (e.g., macOS Terminal): Return a stable 16-255 index
                    Color::Indexed(index256)
                } else {
                    // 3. Basic 16-color support: Return the original ANSI variant
                    self
                }
            }
            None => self, // Return original if not a standard ANSI 16 color
        }
    }
}
