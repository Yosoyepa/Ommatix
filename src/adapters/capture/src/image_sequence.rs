use std::path::{Path, PathBuf};

use ommatix_application::{FrameSource, PortError};
use ommatix_domain::Frame;

const ADAPTER: &str = "image-sequence";

/// Replays every `*.png` in a directory, in lexicographic file-name order.
#[derive(Debug, Clone)]
pub struct ImageSequenceSource {
    files: Vec<PathBuf>,
    cursor: usize,
}

impl ImageSequenceSource {
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, PortError> {
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
        Ok(Self { files, cursor: 0 })
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

impl FrameSource for ImageSequenceSource {
    fn name(&self) -> &'static str {
        ADAPTER
    }

    fn next_frame(&mut self) -> Result<Option<Frame>, PortError> {
        let Some(path) = self.files.get(self.cursor) else {
            return Ok(None);
        };
        self.cursor += 1;
        let img = image::open(path)
            .map_err(|e| {
                PortError::new(ADAPTER, format!("cannot decode {}", path.display())).with_source(e)
            })?
            .into_rgba8();
        let (w, h) = img.dimensions();
        Frame::from_rgba(w, h, img.into_raw())
            .map(Some)
            .map_err(|e| PortError::new(ADAPTER, e.to_string()))
    }
}

#[cfg(test)]
mod tests {
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

        let mut src = ImageSequenceSource::from_dir(&dir).unwrap();
        assert_eq!(src.len(), 2);
        assert_eq!(src.next_frame().unwrap().unwrap().width(), 2);
        assert_eq!(src.next_frame().unwrap().unwrap().width(), 3);
        assert!(src.next_frame().unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_dir_is_an_error() {
        let dir = std::env::temp_dir().join(format!("ommatix-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(ImageSequenceSource::from_dir(&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
