use ratatui::style::Color;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    Auto,
    Light,
    Dark,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub panel: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub warning: Color,
    pub border: Color,
    pub selected: Color,
    pub error: Color,
}
impl Theme {
    pub fn palette(self) -> Palette {
        match self {
            Self::Dark => Palette {
                bg: Color::Rgb(18, 22, 30),
                panel: Color::Rgb(24, 29, 39),
                text: Color::Rgb(214, 222, 235),
                muted: Color::Rgb(151, 165, 187),
                accent: Color::Rgb(130, 218, 185),
                warning: Color::Rgb(239, 182, 137),
                border: Color::Rgb(53, 65, 82),
                selected: Color::Rgb(40, 59, 68),
                error: Color::Rgb(255, 133, 133),
            },
            Self::Light => Palette {
                bg: Color::Rgb(244, 247, 250),
                panel: Color::Rgb(255, 255, 255),
                text: Color::Rgb(30, 41, 56),
                muted: Color::Rgb(85, 101, 121),
                accent: Color::Rgb(20, 108, 81),
                warning: Color::Rgb(145, 74, 22),
                border: Color::Rgb(157, 172, 190),
                selected: Color::Rgb(222, 239, 232),
                error: Color::Rgb(180, 35, 45),
            },
        }
    }
}
impl ThemeChoice {
    /// Probe before starting the input reader: only one consumer can own the TTY.
    pub fn resolve(self) -> Theme {
        match self {
            Self::Light => Theme::Light,
            Self::Dark => Theme::Dark,
            Self::Auto => {
                let mut options = terminal_colorsaurus::QueryOptions::default();
                options.timeout = std::time::Duration::from_millis(250);
                let detected =
                    terminal_colorsaurus::theme_mode(options)
                        .ok()
                        .map(|mode| match mode {
                            terminal_colorsaurus::ThemeMode::Light => Theme::Light,
                            terminal_colorsaurus::ThemeMode::Dark => Theme::Dark,
                        });
                detected
                    .or_else(|| colorfgbg_theme(std::env::var("COLORFGBG").ok().as_deref()))
                    .unwrap_or(Theme::Dark)
            }
        }
    }
}
/// COLORFGBG is only a fallback; terminal responses take precedence over stale env.
pub fn colorfgbg_theme(value: Option<&str>) -> Option<Theme> {
    let index: u8 = value?.rsplit(';').next()?.parse().ok()?;
    let (r, g, b) = match index {
        0..=15 => {
            let colors = [
                (0, 0, 0),
                (128, 0, 0),
                (0, 128, 0),
                (128, 128, 0),
                (0, 0, 128),
                (128, 0, 128),
                (0, 128, 128),
                (192, 192, 192),
                (128, 128, 128),
                (255, 0, 0),
                (0, 255, 0),
                (255, 255, 0),
                (0, 0, 255),
                (255, 0, 255),
                (0, 255, 255),
                (255, 255, 255),
            ];
            colors[index as usize]
        }
        16..=231 => {
            let value = index - 16;
            let levels = [0, 95, 135, 175, 215, 255];
            (
                levels[(value / 36) as usize],
                levels[(value / 6 % 6) as usize],
                levels[(value % 6) as usize],
            )
        }
        232..=255 => {
            let level = 8 + (index - 232) as u32 * 10;
            (level, level, level)
        }
    };
    Some(if 299 * r + 587 * g + 114 * b >= 128000 {
        Theme::Light
    } else {
        Theme::Dark
    })
}
