use std::path::{Path, PathBuf};
use std::sync::Arc;

use ommatix_application::{CapturedFrame, Clock, FrameCapturer, PortError};

use crate::stamp::Stamper;

const ADAPTER: &str = "image-sequence";

/// Replays every `*.png` in a directory, in lexicographic file-name order, as BGRA
/// frames. The geometry epoch grows whenever the image size changes. Capturing past the
/// last file is an error.
#[derive(Debug, Clone)]
pub struct ImageSequenceSource {
    files: Vec<PathBuf>,
    cursor: usize,
    stamper: Stamper,
}

impl ImageSequenceSource {
    /// `clock` stamps each capture.
    pub fn from_dir(dir: impl AsRef<Path>, clock: Arc<dyn Clock>) -> Result<Self, PortError> {
        let dir = dir.as_ref();
        let entries = std::fs::read_dir(dir).map_err(|e| {
            PortError::new(ADAPTER, format!("cannot read {}", dir.display())).with_source(e)
        })?;
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
            })
            .collect();
        files.sort();
        if files.is_empty() {
            return Err(PortError::new(
                ADAPTER,
                format!("no .png files in {}", dir.display()),
            ));
        }
        Ok(Self {
            files,
            cursor: 0,
            stamper: Stamper::new(clock),
        })
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl FrameCapturer for ImageSequenceSource {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn capture(&mut self) -> Result<CapturedFrame, PortError> {
        let Some(path) = self.files.get(self.cursor) else {
            return Err(PortError::new(ADAPTER, "the image sequence has ended"));
        };
        self.cursor += 1;
        let captured_at = self.stamper.now();
        let img = image::open(path)
            .map_err(|e| {
                PortError::new(ADAPTER, format!("cannot decode {}", path.display())).with_source(e)
            })?
            .into_rgba8();
        let (w, h) = img.dimensions();
        self.stamper
            .stamp_rgba(ADAPTER, w, h, img.into_raw(), captured_at)
    }
}

#[cfg(test)]
mod tests {
    use ommatix_application::ManualClock;

    use super::*;

    #[test]
    fn replays_pngs_in_name_order() {
        let dir = std::env::temp_dir().join(format!("ommatix-seq-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, w) in [("b.png", 3), ("a.png", 2)] {
            image::RgbaImage::from_pixel(w, 2, image::Rgba([1, 2, 3, 255]))
                .save(dir.join(name))
                .unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "ignored").unwrap();

        let mut src = ImageSequenceSource::from_dir(&dir, Arc::new(ManualClock::new())).unwrap();
        assert_eq!(src.len(), 2);
        let a = src.capture().unwrap();
        assert_eq!(
            (a.frame.width(), a.frame_seq, a.frame.geometry_epoch()),
            (2, 1, 1)
        );
        assert_eq!(&a.frame.pixels()[..4], &[3, 2, 1, 255], "BGRA");
        let b = src.capture().unwrap();
        assert_eq!(
            (b.frame.width(), b.frame_seq, b.frame.geometry_epoch()),
            (3, 2, 2)
        );
        assert!(src.capture().is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_dir_is_an_error() {
        let dir = std::env::temp_dir().join(format!("ommatix-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(ImageSequenceSource::from_dir(&dir, Arc::new(ManualClock::new())).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
