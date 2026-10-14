//! Newline-delimited JSON-RPC over a byte stream, with a hard per-message limit.
//!
//! `rmcp`'s `AsyncRwTransport` reads lines with an unbounded `read_until`, so the limit is
//! enforced here instead: a line is buffered only up to the limit; past it the rest of the
//! line is consumed and discarded chunk by chunk (never stored) and reported as
//! [`Frame::Oversized`]. Memory per connection stays below the limit plus one read chunk.

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

/// Reads [`Frame`]s from a buffered stream.
#[derive(Debug)]
pub(crate) struct LineReader<R> {
    inner: R,
    limit: usize,
    buf: Vec<u8>,
}

impl<R: AsyncBufRead + Unpin> LineReader<R> {
    pub(crate) fn new(inner: R, limit: usize) -> Self {
        Self {
            inner,
            limit,
            buf: Vec::new(),
        }
    }

    pub(crate) async fn next_frame(&mut self) -> io::Result<Frame> {
        self.buf.clear();
        // One extra byte so a message of exactly `limit` bytes followed by `\r` fits.
        let cap = self.limit.saturating_add(1);
        let mut discarding = false;
        loop {
            let available = self.inner.fill_buf().await?;
            if available.is_empty() {
                return Ok(if discarding {
                    Frame::Oversized
                } else if self.buf.is_empty() {
                    Frame::Eof
                } else {
                    self.finish()
                });
            }
            let newline = available.iter().position(|&b| b == b'\n');
            let take = newline.unwrap_or(available.len());
            if !discarding {
                if self.buf.len().saturating_add(take) > cap {
                    discarding = true;
                    self.buf.clear();
                } else {
                    self.buf.extend_from_slice(&available[..take]);
                }
            }
            let consumed = newline.map_or(take, |i| i + 1);
            self.inner.consume(consumed);
            if newline.is_some() {
                return Ok(if discarding {
                    Frame::Oversized
                } else {
                    self.finish()
                });
            }
        }
    }

    fn finish(&mut self) -> Frame {
        if self.buf.last() == Some(&b'\r') {
            self.buf.pop();
        }
        if self.buf.len() > self.limit {
            self.buf.clear();
            Frame::Oversized
        } else {
            Frame::Message(std::mem::take(&mut self.buf))
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::BufReader;

    use super::*;

    fn frames(input: &[u8], limit: usize, chunk: usize) -> Vec<Frame> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut reader = LineReader::new(BufReader::with_capacity(chunk, input), limit);
            let mut out = Vec::new();
            loop {
                let frame = reader.next_frame().await.unwrap();
                let eof = frame == Frame::Eof;
                out.push(frame);
                if eof {
                    return out;
                }
            }
        })
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
            assert!(reader.buf.capacity() <= 64 + 4096);
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
}
