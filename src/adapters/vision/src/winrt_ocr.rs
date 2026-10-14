//! [`WinRtTextRecognizer`]: OCR with the `Windows.Media.Ocr` engine built into Windows
//! (feature `winrt-ocr`, Windows only). It uses the safe WinRT projection of the `windows`
//! crate: this module contains no `unsafe` code.
//!
//! - The engine is created for exactly the configured language tag. If that language is
//!   not installed, construction fails: it never falls back to another language (a wrong
//!   language silently degrades every label).
//! - Each region is cropped (rounded outwards, clipped to the frame), split into tiles no
//!   larger than `OcrEngine::MaxImageDimension`, and recognised tile by tile; a word cut by
//!   a tile seam is read as two words, which only happens for crops above that limit.
//! - Word rectangles are taken back to frame coordinates (tile origin added), clipped to
//!   the region, and the words are returned sorted by box then text.
//! - The engine reports no per-word confidence, so none is invented.
//! - Recognised text is never logged.

use ommatix_application::{PortError, TextRecognizer};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Word;
use ommatix_domain::incremental::BgraFrame;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrResult};
use windows::Storage::Streams::DataWriter;
use windows::core::HSTRING;

use crate::crop::{BPP, PixelRect, crop_bgra};
use crate::ocr::{sort_words, word_box};

const NAME: &str = "winrt-ocr";

/// The language used when the configuration does not name one.
pub const DEFAULT_LANGUAGE: &str = "es-ES";

fn port(message: impl Into<String>) -> PortError {
    PortError::new(NAME, message)
}

fn winrt(what: &'static str) -> impl Fn(windows::core::Error) -> PortError {
    move |e| port(what).with_source(e)
}

/// Windows OCR for one installed language.
pub struct WinRtTextRecognizer {
    engine: OcrEngine,
    language: String,
    max_side: u32,
}

impl std::fmt::Debug for WinRtTextRecognizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinRtTextRecognizer")
            .field("language", &self.language)
            .field("max_side", &self.max_side)
            .finish_non_exhaustive()
    }
}

/// The OCR languages installed on this machine, as BCP-47 tags, sorted.
pub fn installed_languages() -> Result<Vec<String>, PortError> {
    let languages = OcrEngine::AvailableRecognizerLanguages()
        .map_err(winrt("cannot list the OCR languages"))?;
    let count = languages
        .Size()
        .map_err(winrt("cannot list the OCR languages"))?;
    let mut tags = (0..count)
        .map(|i| {
            languages
                .GetAt(i)
                .and_then(|l| l.LanguageTag())
                .map(|t| t.to_string_lossy())
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(winrt("cannot list the OCR languages"))?;
    tags.sort();
    Ok(tags)
}

impl WinRtTextRecognizer {
    /// An engine for exactly `language` (a BCP-47 tag such as `es-ES`). Fails if that
    /// language is not installed; never falls back to another one.
    pub fn new(language: &str) -> Result<Self, PortError> {
        let unavailable = || {
            let installed =
                installed_languages().map_or_else(|_| "unknown".to_owned(), |t| t.join(", "));
            port(format!(
                "OCR language {language:?} is not installed (installed: {installed}); \
                 install it in Windows Settings or configure another ocr_language"
            ))
        };
        let lang = Language::CreateLanguage(&HSTRING::from(language))
            .map_err(winrt("invalid OCR language tag"))?;
        if !OcrEngine::IsLanguageSupported(&lang).map_err(winrt("cannot query OCR languages"))? {
            return Err(unavailable());
        }
        let engine = OcrEngine::TryCreateFromLanguage(&lang).map_err(|_| unavailable())?;
        let actual = engine
            .RecognizerLanguage()
            .and_then(|l| l.LanguageTag())
            .map_err(winrt("cannot read the OCR engine's language"))?
            .to_string_lossy();
        if !actual.eq_ignore_ascii_case(language) {
            return Err(port(format!(
                "the OCR engine resolved {language:?} to {actual:?}; configure ocr_language = \
                 {actual:?} explicitly"
            )));
        }
        let max_side =
            OcrEngine::MaxImageDimension().map_err(winrt("cannot query the OCR engine"))?;
        Ok(Self {
            engine,
            language: actual,
            max_side: max_side.max(1),
        })
    }

    /// The BCP-47 tag of the engine's language.
    pub fn language(&self) -> &str {
        &self.language
    }

    fn recognize_tile(
        &self,
        frame: &BgraFrame,
        tile: PixelRect,
        clip: &BBox,
    ) -> Result<Vec<Word>, PortError> {
        let mut pixels = crop_bgra(frame, tile);
        // Captures may carry any alpha; OCR reads the colour only.
        for px in pixels.chunks_exact_mut(BPP) {
            px[3] = 0xFF;
        }
        let writer = DataWriter::new().map_err(winrt("cannot create the OCR buffer"))?;
        writer
            .WriteBytes(&pixels)
            .map_err(winrt("cannot fill the OCR buffer"))?;
        let buffer = writer
            .DetachBuffer()
            .map_err(winrt("cannot fill the OCR buffer"))?;
        let side = |v: u32| i32::try_from(v).map_err(|_| port("tile too large"));
        let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
            &buffer,
            BitmapPixelFormat::Bgra8,
            side(tile.width)?,
            side(tile.height)?,
            BitmapAlphaMode::Ignore,
        )
        .map_err(winrt("cannot create the OCR bitmap"))?;
        let result: OcrResult = self
            .engine
            .RecognizeAsync(&bitmap)
            .and_then(|op| op.join())
            .map_err(winrt("OCR failed"))?;
        let read = winrt("cannot read the OCR result");
        let lines = result.Lines().map_err(&read)?;
        let mut words = Vec::new();
        for i in 0..lines.Size().map_err(&read)? {
            let line_words = lines.GetAt(i).and_then(|l| l.Words()).map_err(&read)?;
            for j in 0..line_words.Size().map_err(&read)? {
                let word = line_words.GetAt(j).map_err(&read)?;
                let text = word.Text().map_err(&read)?.to_string_lossy();
                let r = word.BoundingRect().map_err(&read)?;
                let rect = [r.X, r.Y, r.Width, r.Height].map(f64::from);
                if text.trim().is_empty() {
                    continue;
                }
                if let Some(bbox) = word_box(rect, tile, clip) {
                    words.push(Word::new(text, bbox));
                }
            }
        }
        Ok(words)
    }
}

impl TextRecognizer for WinRtTextRecognizer {
    fn name(&self) -> &'static str {
        NAME
    }

    fn recognize(&mut self, frame: &BgraFrame, regions: &[BBox]) -> Result<Vec<Word>, PortError> {
        let mut words = Vec::new();
        for region in regions {
            let (Some(rect), Some(clip)) = (
                PixelRect::cover(region, frame.width(), frame.height()),
                region.clip(&frame.bounds()),
            ) else {
                continue;
            };
            for tile in rect.tiles(self.max_side) {
                words.extend(self.recognize_tile(frame, tile, &clip)?);
            }
        }
        sort_words(&mut words);
        Ok(words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_language_that_is_not_installed_is_refused_without_fallback() {
        // Klingon is never an installed OCR language.
        let err = WinRtTextRecognizer::new("tlh").unwrap_err();
        let message = ommatix_application::error_chain(&err);
        assert!(
            message.contains("not installed") || message.contains("invalid"),
            "{message}"
        );
    }

    /// Manual: needs the configured OCR language installed (`es-ES` by default). Runs the
    /// engine on synthetic frames only.
    #[test]
    #[ignore = "needs a Windows OCR language pack; run with --ignored"]
    fn a_blank_frame_has_no_words() {
        let mut ocr = WinRtTextRecognizer::new(DEFAULT_LANGUAGE).expect("es-ES OCR is installed");
        assert!(ocr.language().eq_ignore_ascii_case(DEFAULT_LANGUAGE));
        let (w, h) = (320, 120);
        let frame =
            BgraFrame::new(w, h, 4 * w as usize, 0, vec![0xFF; 4 * (w * h) as usize]).unwrap();
        let regions = [frame.bounds(), BBox::new(10.0, 10.0, 200.0, 90.0).unwrap()];
        assert!(ocr.recognize(&frame, &regions).unwrap().is_empty());
        assert!(ocr.recognize(&frame, &[]).unwrap().is_empty());
    }

    /// 5x7 block glyphs, enough for a few capital words.
    fn glyph(c: char) -> [&'static str; 7] {
        match c {
            'H' => [
                "X...X", "X...X", "X...X", "XXXXX", "X...X", "X...X", "X...X",
            ],
            'O' => [
                ".XXX.", "X...X", "X...X", "X...X", "X...X", "X...X", ".XXX.",
            ],
            'L' => [
                "X....", "X....", "X....", "X....", "X....", "X....", "XXXXX",
            ],
            'A' => [
                ".XXX.", "X...X", "X...X", "XXXXX", "X...X", "X...X", "X...X",
            ],
            _ => ["....."; 7],
        }
    }

    /// A white frame with `text` drawn in black block letters (`cell` pixels per glyph
    /// cell) at `(ox, oy)`; returns the frame and the drawn text's box.
    fn draw(w: u32, h: u32, text: &str, cell: u32, ox: u32, oy: u32) -> (BgraFrame, BBox) {
        let mut pixels = vec![0xFFu8; 4 * (w * h) as usize];
        for (g, ch) in text.chars().enumerate() {
            for (r, row) in glyph(ch).iter().enumerate() {
                for (c, bit) in row.chars().enumerate() {
                    if bit != 'X' {
                        continue;
                    }
                    let x0 = ox + (g as u32 * 6 + c as u32) * cell;
                    let y0 = oy + r as u32 * cell;
                    for y in y0..y0 + cell {
                        for x in x0..x0 + cell {
                            let i = 4 * (y * w + x) as usize;
                            pixels[i..i + 3].copy_from_slice(&[0, 0, 0]);
                        }
                    }
                }
            }
        }
        let drawn = BBox::new(
            f64::from(ox),
            f64::from(oy),
            f64::from(ox + (text.chars().count() as u32 * 6 - 1) * cell),
            f64::from(oy + 7 * cell),
        )
        .unwrap();
        (
            BgraFrame::new(w, h, 4 * w as usize, 0, pixels).unwrap(),
            drawn,
        )
    }

    /// Manual: block letters drawn with filled rectangles (no font, no new dependency) are
    /// read inside the region they were drawn in, at frame coordinates.
    #[test]
    #[ignore = "needs a Windows OCR language pack; run with --ignored"]
    fn words_on_a_synthetic_frame_land_at_frame_coordinates() {
        let mut ocr = WinRtTextRecognizer::new(DEFAULT_LANGUAGE).expect("es-ES OCR is installed");
        // 21 px tall letters: the engine reads them (block glyphs are not a real font, so
        // the text itself is not asserted, only where it lands).
        let (frame, drawn) = draw(480, 200, "HOLA", 3, 210, 90);
        let region = BBox::new(180.5, 60.0, 420.0, 170.0).unwrap();
        let words = ocr.recognize(&frame, &[region]).unwrap();
        assert_eq!(words.len(), 1, "one word is read");
        // At frame coordinates (the region's origin is added), over the drawn text.
        assert!(
            words[0].bbox.iou(&drawn) > 0.5,
            "{:?} vs {drawn:?}",
            words[0].bbox
        );
        assert!(region.contains(&words[0].bbox));
        // A region that does not cover the text reads nothing; the same input reads the
        // same words.
        let elsewhere = BBox::new(0.0, 0.0, 150.0, 200.0).unwrap();
        assert!(ocr.recognize(&frame, &[elsewhere]).unwrap().is_empty());
        assert_eq!(ocr.recognize(&frame, &[region]).unwrap(), words);
    }
}
