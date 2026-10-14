/// Colour scheme of the simulated desktop. Themes exist to prove the detector assumes no
/// background colour (ADR-0010).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Theme {
    /// Light window on a light wallpaper of nearly the same colour.
    #[default]
    Light,
    /// Light text on dark surfaces.
    Dark,
    /// The light theme on a wallpaper that contrasts with the window, so the window
    /// itself is a visible element.
    ContrastingWallpaper,
}

/// RGBA colour of every part of the simulated desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub desktop: [u8; 4],
    pub window: [u8; 4],
    pub title: [u8; 4],
    pub primary: [u8; 4],
    pub secondary: [u8; 4],
    pub ink: [u8; 4],
    pub border: [u8; 4],
    pub focus: [u8; 4],
}

const LIGHT: Palette = Palette {
    desktop: [236, 239, 244, 255],
    window: [255, 255, 255, 255],
    title: [46, 52, 64, 255],
    primary: [94, 129, 172, 255],
    secondary: [216, 222, 233, 255],
    ink: [20, 20, 20, 255],
    border: [150, 150, 150, 255],
    focus: [94, 129, 172, 255],
};

const DARK: Palette = Palette {
    desktop: [24, 26, 31, 255],
    window: [40, 42, 48, 255],
    title: [64, 70, 84, 255],
    primary: [94, 129, 172, 255],
    secondary: [72, 76, 88, 255],
    ink: [230, 230, 230, 255],
    border: [110, 110, 110, 255],
    focus: [136, 192, 208, 255],
};

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dark, Theme::ContrastingWallpaper];

    pub const fn palette(self) -> Palette {
        match self {
            Theme::Light => LIGHT,
            Theme::Dark => DARK,
            Theme::ContrastingWallpaper => Palette {
                desktop: [120, 140, 170, 255],
                ..LIGHT
            },
        }
    }
}
