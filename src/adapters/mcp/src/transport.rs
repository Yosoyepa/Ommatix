//! Newline-delimited JSON-RPC over a byte stream, with a hard per-message limit.
//!
//! `rmcp`'s `AsyncRwTransport` reads lines with an unbounded `read_until`, so the limit is
//! enforced here instead: a line is buffered only up to the limit; past it the rest of the
//! line is consumed and discarded chunk by chunk (never stored) and reported as
//! [`Frame::Oversized`]. Memory per connection stays below the limit plus one read chunk.
//!
//! The framing rules live in one place, `Framer`, which is handed the stream one buffered
//! chunk at a time and never reads by itself. [`LineReader`] drives it over an asynchronous
//! reader ([`LineReader::next_frame`]) or a blocking one ([`LineReader::next_frame_blocking`],
//! for a transport that gives each connection a thread, ADR-0043): both only fetch the next
//! chunk and hand it over, so the two cannot frame a stream differently.

use std::io;

use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// One unit read from the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Frame {
    /// A complete message without its `\n` (or `\r\n`) terminator.
    Message(Vec<u8>),
    /// A line longer than the limit; its bytes were discarded.
    Oversized,
    /// End of input.
    Eof,
}

/// The framing rules of one stream, fed one buffered chunk at a time: a line ends at `\n`, a
/// `\r` just before it is dropped, a line over the limit is discarded without being stored, and
/// at the end of input a final line without its terminator is still a line (an oversized one
/// is reported once). The bytes of a line it discards are overwritten before they are dropped.
///
/// With `cut_oversized` set (the handshake, before a client is admitted), a line over the limit
/// is reported as [`Frame::Oversized`] as soon as its bytes pass the limit, without reading on to
/// its end: nothing after that is read, because the connection ends.
#[derive(Debug)]
struct Framer {
    limit: usize,
    buf: Vec<u8>,
    discarding: bool,
    terminated: bool,
    cut_oversized: bool,
}

impl Framer {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            buf: Vec::new(),
            discarding: false,
            terminated: false,
            cut_oversized: false,
        }
    }

    /// Drops what the line holds so far, overwriting it first.
    fn discard(&mut self) {
        self.buf.fill(0);
        self.buf.clear();
    }

    /// Starts a new frame.
    fn begin(&mut self) {
        self.buf.clear();
        self.discarding = false;
        self.terminated = false;
    }

    /// Takes in one chunk of the stream; `available` is empty at the end of input. Returns how
    /// many of its bytes were consumed and the frame, once one is complete.
    fn feed(&mut self, available: &[u8]) -> (usize, Option<Frame>) {
        if available.is_empty() {
            let frame = if self.discarding {
                Frame::Oversized
            } else if self.buf.is_empty() {
                Frame::Eof
            } else {
                self.finish()
            };
            return (0, Some(frame));
        }
        // One extra byte so a message of exactly `limit` bytes followed by `\r` fits.
        let cap = self.limit.saturating_add(1);
        let newline = available.iter().position(|&b| b == b'\n');
        let take = newline.unwrap_or(available.len());
        if !self.discarding {
            if self.buf.len().saturating_add(take) > cap {
                self.discard();
                if self.cut_oversized {
                    return (take, Some(Frame::Oversized));
                }
                self.discarding = true;
            } else {
                self.buf.extend_from_slice(&available[..take]);
            }
        }
        let consumed = newline.map_or(take, |i| i + 1);
        let frame = newline.map(|_| {
            self.terminated = true;
            if self.discarding {
                Frame::Oversized
            } else {
                self.finish()
            }
        });
        (consumed, frame)
    }

    fn finish(&mut self) -> Frame {
        if self.buf.last() == Some(&b'\r') {
            self.buf.pop();
        }
        if self.buf.len() > self.limit {
            self.discard();
            Frame::Oversized
        } else {
            Frame::Message(std::mem::take(&mut self.buf))
        }
    }
}

/// Reads [`Frame`]s from a buffered stream, asynchronous or blocking.
#[derive(Debug)]
pub(crate) struct LineReader<R> {
    inner: R,
    framer: Framer,
}

impl<R> LineReader<R> {
    pub(crate) fn new(inner: R, limit: usize) -> Self {
        Self {
            inner,
            framer: Framer::new(limit),
        }
    }

    /// Whether the last frame read ended at its `\n`, rather than at the end of input.
    pub(crate) fn terminated(&self) -> bool {
        self.framer.terminated
    }

    /// Whether a line over the limit ends the read at once ([`Frame::Oversized`] as soon as its
    /// bytes pass the limit) instead of being read on to its end. Set for a handshake, so that the
    /// bytes a peer can make the reader take before it is admitted are bounded by the limit, plus
    /// one byte, plus one buffer of the inner reader; cleared after it, so that an established
    /// connection survives an oversized message.
    pub(crate) fn cut_oversized(&mut self, cut: bool) {
        self.framer.cut_oversized = cut;
    }
}

impl<R: AsyncBufRead + Unpin> LineReader<R> {
    pub(crate) async fn next_frame(&mut self) -> io::Result<Frame> {
        self.framer.begin();
        loop {
            let available = self.inner.fill_buf().await?;
            let (consumed, frame) = self.framer.feed(available);
            self.inner.consume(consumed);
            if let Some(frame) = frame {
                return Ok(frame);
            }
        }
    }
}

impl<R: io::BufRead> LineReader<R> {
    /// [`LineReader::next_frame`] over a blocking reader, with the same framing. A read
    /// interrupted by a signal is retried, as the standard library's own line readers do.
    pub(crate) fn next_frame_blocking(&mut self) -> io::Result<Frame> {
        self.framer.begin();
        loop {
            let available = match self.inner.fill_buf() {
                Ok(available) => available,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            let (consumed, frame) = self.framer.feed(available);
            self.inner.consume(consumed);
            if let Some(frame) = frame {
                return Ok(frame);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::BufReader;

    use super::*;

    /// Every frame of `input` and whether each ended at its `\n`, read asynchronously.
    fn frames_terminated(input: &[u8], limit: usize, chunk: usize) -> Vec<(Frame, bool)> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut reader = LineReader::new(BufReader::with_capacity(chunk, input), limit);
            let mut out = Vec::new();
            loop {
                let frame = reader.next_frame().await.unwrap();
                let eof = frame == Frame::Eof;
                out.push((frame, reader.terminated()));
                if eof {
                    return out;
                }
            }
        })
    }

    /// The same, read by the blocking reader.
    fn frames_terminated_blocking(input: &[u8], limit: usize, chunk: usize) -> Vec<(Frame, bool)> {
        let mut reader = LineReader::new(io::BufReader::with_capacity(chunk, input), limit);
        let mut out = Vec::new();
        loop {
            let frame = reader.next_frame_blocking().unwrap();
            let eof = frame == Frame::Eof;
            out.push((frame, reader.terminated()));
            if eof {
                return out;
            }
        }
    }

    fn frames(input: &[u8], limit: usize, chunk: usize) -> Vec<Frame> {
        frames_terminated(input, limit, chunk)
            .into_iter()
            .map(|(frame, _)| frame)
            .collect()
    }

    #[test]
    fn splits_lines_and_strips_terminators() {
        assert_eq!(
            frames(b"ab\r\ncd\nef", 8, 3),
            vec![
                Frame::Message(b"ab".to_vec()),
                Frame::Message(b"cd".to_vec()),
                Frame::Message(b"ef".to_vec()),
                Frame::Eof
            ]
        );
    }

    #[test]
    fn a_line_of_exactly_the_limit_passes_and_one_more_byte_does_not() {
        assert_eq!(
            frames(b"abcd\nabcde\nok\n", 4, 2),
            vec![
                Frame::Message(b"abcd".to_vec()),
                Frame::Oversized,
                Frame::Message(b"ok".to_vec()),
                Frame::Eof
            ]
        );
        assert_eq!(
            frames(b"abcd\r\n", 4, 16),
            vec![Frame::Message(b"abcd".to_vec()), Frame::Eof]
        );
    }

    #[test]
    fn an_oversized_line_is_discarded_without_buffering_it() {
        let mut input = vec![b'x'; 1 << 20];
        input.extend_from_slice(b"\nok\n");
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut reader = LineReader::new(BufReader::with_capacity(4096, &input[..]), 64);
            assert_eq!(reader.next_frame().await.unwrap(), Frame::Oversized);
            assert!(reader.framer.buf.capacity() <= 64 + 4096);
            assert_eq!(
                reader.next_frame().await.unwrap(),
                Frame::Message(b"ok".to_vec())
            );
        });
    }

    #[test]
    fn an_oversized_tail_without_newline_is_reported_once() {
        assert_eq!(
            frames(b"abcdefgh", 4, 3),
            vec![Frame::Oversized, Frame::Eof]
        );
    }

    #[test]
    fn the_blocking_reader_discards_an_oversized_line_without_buffering_it() {
        let mut input = vec![b'x'; 1 << 20];
        input.extend_from_slice(b"\nok\n");
        let mut reader = LineReader::new(io::BufReader::with_capacity(4096, &input[..]), 64);
        assert_eq!(reader.next_frame_blocking().unwrap(), Frame::Oversized);
        assert!(reader.framer.buf.capacity() <= 64 + 4096);
        assert_eq!(
            reader.next_frame_blocking().unwrap(),
            Frame::Message(b"ok".to_vec())
        );
    }

    #[test]
    fn only_a_line_that_ends_at_its_newline_is_terminated() {
        assert_eq!(
            frames_terminated(b"ab\r\ncd", 8, 3),
            vec![
                (Frame::Message(b"ab".to_vec()), true),
                (Frame::Message(b"cd".to_vec()), false),
                (Frame::Eof, false)
            ]
        );
        assert_eq!(
            frames_terminated(b"abcdefgh\nabcdefgh", 4, 3),
            vec![
                (Frame::Oversized, true),
                (Frame::Oversized, false),
                (Frame::Eof, false)
            ]
        );
    }

    /// Fixed-seed generator (splitmix64), so the generated streams are the same on every run.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            (z ^ (z >> 31)) % n
        }
    }

    #[test]
    fn the_blocking_and_asynchronous_readers_frame_every_generated_stream_alike() {
        let mut rng = Rng(0x7EA5_0001);
        let alphabet = b"ab\r\n\n\n x{}\xff";
        for _ in 0..2_000 {
            let len = usize::try_from(rng.below(64)).unwrap();
            let input: Vec<u8> = (0..len)
                .map(|_| alphabet[usize::try_from(rng.below(alphabet.len() as u64)).unwrap()])
                .collect();
            let limit = 1 + usize::try_from(rng.below(12)).unwrap();
            let chunk = 1 + usize::try_from(rng.below(9)).unwrap();
            assert_eq!(
                frames_terminated_blocking(&input, limit, chunk),
                frames_terminated(&input, limit, chunk),
                "{input:?} limit {limit} chunk {chunk}"
            );
        }
    }
}
