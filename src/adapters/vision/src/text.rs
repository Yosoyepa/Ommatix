use ommatix_application::{PortError, TextRecognizer};
use ommatix_domain::bbox::BBox;
use ommatix_domain::fusion::Word;
use ommatix_domain::incremental::BgraFrame;

/// The "none" recogniser: reads no text. Elements then carry empty labels with
/// `label_source: unknown`, which the contract allows. Real OCR arrives in S3.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullTextRecognizer;

impl TextRecognizer for NullTextRecognizer {
    fn name(&self) -> &'static str {
        "none"
    }

    fn recognize(&mut self, _: &BgraFrame, _: &[BBox]) -> Result<Vec<Word>, PortError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nothing() {
        let frame = BgraFrame::new(1, 1, 4, 1, vec![0; 4]).unwrap();
        let mut ocr = NullTextRecognizer;
        assert_eq!(ocr.name(), "none");
        assert_eq!(ocr.recognize(&frame, &[frame.bounds()]).unwrap().len(), 0);
    }
}
