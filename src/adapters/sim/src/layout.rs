//! Fixed geometry of the simulated desktop (640×400), shared by the renderer, the input
//! handler and tests.

use ommatix_domain::Rect;

pub const SCREEN: Rect = Rect::new(0, 0, 640, 400);
pub const WINDOW: Rect = Rect::new(40, 40, 560, 320);
pub const TITLE_BAR: Rect = Rect::new(40, 40, 560, 28);
pub const TEXT_FIELD: Rect = Rect::new(80, 120, 300, 30);
pub const CANCEL_BUTTON: Rect = Rect::new(400, 300, 90, 32);
pub const SAVE_BUTTON: Rect = Rect::new(500, 300, 80, 32);
pub const DIALOG: Rect = Rect::new(200, 170, 240, 110);
pub const OK_BUTTON: Rect = Rect::new(360, 240, 64, 28);

/// Typed characters are drawn as solid glyph blocks.
pub const GLYPH_W: u32 = 6;
pub const GLYPH_H: u32 = 12;
pub const GLYPH_ADVANCE: u32 = 9;
pub const TEXT_ORIGIN_X: u32 = TEXT_FIELD.x + 10;
pub const TEXT_ORIGIN_Y: u32 = TEXT_FIELD.y + 8;
/// Characters that fit in the field; extra input is ignored like a `maxlength`.
pub const MAX_CHARS: usize = ((TEXT_FIELD.w - 20) / GLYPH_ADVANCE) as usize;

pub mod colors {
    pub const DESKTOP: [u8; 4] = [236, 239, 244, 255];
    pub const WINDOW: [u8; 4] = [255, 255, 255, 255];
    pub const TITLE: [u8; 4] = [46, 52, 64, 255];
    pub const PRIMARY: [u8; 4] = [94, 129, 172, 255];
    pub const SECONDARY: [u8; 4] = [216, 222, 233, 255];
    pub const INK: [u8; 4] = [20, 20, 20, 255];
    pub const BORDER: [u8; 4] = [150, 150, 150, 255];
    pub const FOCUS: [u8; 4] = [94, 129, 172, 255];
}
