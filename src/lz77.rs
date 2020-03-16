//! The lz77 module contains an implementation of the LZ77 compression/decompression algorithm.

use fallible_iterator::FallibleIterator;
use thiserror::Error as ThisErr;
use std::collections::{VecDeque, HashMap};
use crate::matchmap::MatchMap;


/// Represents an LZ77 token.
#[derive(Clone, Debug)]
pub enum Token {
    /// This is literal characters; i.e. characters that aren't yet part of a match.
    Literal(u8),
    /// A match represents the distance to go back in the output stream (offset)
    /// and how long to copy.
    Match {
        /// The distance to go back in the output stream
        offset: usize,
        /// How much data to copy from the point pointed to by the offset
        length: usize,
    },
}

impl Token {
    /// Helper function for creating a new match without needing to write enum boilerplate
    pub fn new_match(offset: usize, length: usize) -> Self {
        Token::Match { offset, length }
    }

    /// Converts a string (or string-like `[u8]`) into Literal tokens
    pub fn literals(s: impl Into<String>) -> Vec<Token> {
        s.into().into_bytes().into_iter().map(Token::Literal).collect()
    }

    /// Returns the length of a token; literals are 1, matches are their length
    pub fn token_len(&self) -> usize {
        match self {
            Token::Literal(_) => 1,
            Token::Match { length, .. } => *length,
        }
    }
}

/// The error types for LZ77
#[derive(ThisErr, Debug, Clone)]
pub enum Error {
    /// The offset was too large, either for the current buffer or based on settings.
    #[error("Invalid offset at {idx} bytes: {offset}")]
    InvalidOffset {
        /// Where in the output buffer the offset was encountered
        idx: usize,
        /// The offset
        offset: usize,
    },
    /// Either the input or output was too large.
    #[error("The output exceeded a maximum size specification.")]
    MaximumSizeExceeded,
}

/// Represents settings for the LZ77 algorithm, compression direction.
pub struct CompressionSettings {
    max_match_offset: usize,
    min_match_len: usize,
    end_literals: usize,
}

impl Default for CompressionSettings {
    fn default() -> Self {
        CompressionSettings {
            max_match_offset: 64 * 1024,
            min_match_len: 4,
            end_literals: 12,
        }
    }
}

impl CompressionSettings {
    /// Default compression settings for the LZ4 algorithm
    /// Minimum match of 4, 12 literals at the end, max match offset of 64KiB
    pub fn lz4_default() -> Self {
        Self::default()
    }

    #[doc(hidden)]
    pub(crate) fn test_default() -> Self {
        let mut out = Self::default();
        out.end_literals = 0;
        out
    }
}

/// Represents settings for the LZ77 algorithm, decompression direction.
pub struct DecompressionSettings {
    max_output_len: usize
}

pub const DEFAULT_MAX_OUTPUT: usize = (4 * 1024 * 1024);

impl Default for DecompressionSettings {
    fn default() -> Self {
        DecompressionSettings {
            max_output_len: DEFAULT_MAX_OUTPUT
        }
    }
}

pub struct Compressor<'a> {
    data: &'a [u8],
    idx: usize,
    matches: MatchMap<'a>,
    settings: CompressionSettings,
}

pub struct Decompressor<T: Iterator<Item=Token>> {
    source: T,
    output_buf: Vec<u8>,
    settings: DecompressionSettings,
}

impl Compressor<'_> {
    pub fn new(data: &[u8], settings: CompressionSettings) -> Compressor<'_> {
        Compressor { data, idx: 0, matches: MatchMap::new(settings.max_match_offset), settings }
    }

    pub fn reset(&mut self) {
        self.idx = 0
    }

    fn get_match(&self) -> Option<Token> {
        let dist_from_end = self.data.len() - self.idx;
        if dist_from_end > self.settings.min_match_len && dist_from_end > self.settings.end_literals {
            let match_idx = self.matches.get_match(&self.data[self.idx..(self.idx + self.settings.min_match_len)]);
            match_idx.map(|i| Token::new_match(self.idx - i, self.determine_match_length(i)))
        } else {
            None
        }
    }

    fn determine_match_length(&self, match_idx: usize) -> usize {
        self.data[self.idx..].iter()
            .zip(self.data[match_idx..].iter())
            .take_while(|(&a, &b)| a == b).count()
    }

    fn advance(&mut self, amt: usize) {
        self.idx += amt;
        self.matches.advance(self.idx);
    }
}

impl Iterator for Compressor<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let out = if self.idx >= self.data.len() {
            None
        } else if let Some(m) = self.get_match() {
            Some(m)
        } else {
            let o = self.data[self.idx];
            Some(Token::Literal(o))
        };

        if self.data.len().saturating_sub(self.idx) > self.settings.min_match_len {
            self.matches.add_prefix(&self.data[self.idx..(self.idx + self.settings.min_match_len)], self.idx);
        };
        out.map(|i| {
            self.advance(i.token_len());
            i
        })
    }
}

impl<T: Iterator<Item=Token>> Decompressor<T> {
    pub fn new(source: T, settings: DecompressionSettings) -> Self {
        Decompressor { source, settings, output_buf: Vec::new() }
    }

    pub fn decompress(mut self) -> Result<Vec<u8>, Error> {
        for token in self.source {
            match token {
                Token::Literal(l) => {
                    if self.output_buf.len() + 1 > self.settings.max_output_len {
                        return Err(Error::MaximumSizeExceeded);
                    } else {
                        self.output_buf.push(l)
                    }
                }
                Token::Match { offset, length } => {
                    if offset > self.output_buf.len() {
                        return Err(Error::InvalidOffset { idx: self.output_buf.len(), offset });
                    }

                    if self.output_buf.len() + length > self.settings.max_output_len {
                        return Err(Error::MaximumSizeExceeded);
                    }

                    let copy_start = self.output_buf.len() - offset;
                    let copy_end = copy_start + length;

                    for idx in copy_start..copy_end {
                        self.output_buf.push(self.output_buf[idx])
                    }
                }
            }
        };

        Ok(self.output_buf)
    }
}

pub fn compress(data: impl AsRef<[u8]>, settings: CompressionSettings) -> Vec<Token> {
    let mut cmp = Compressor::new(data.as_ref(), settings);
    cmp.collect()
}

pub fn decompress(tokens: impl Iterator<Item=Token>, settings: DecompressionSettings) -> Result<Vec<u8>, Error> {
    let dcmp = Decompressor { source: tokens, output_buf: Vec::new(), settings };
    dcmp.decompress()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_decompression() {
        let inp: Vec<_> = vec![
            Token::literals("abcdef"),
            vec![Token::new_match(4, 8)],
            Token::literals("ABCD"),
            vec![Token::new_match(1, 4)]
        ].into_iter().flatten().collect();

        let res = decompress(inp.into_iter(), DecompressionSettings::default());

        let decompressed = String::from_utf8(res.unwrap()).unwrap();

        assert_eq!(decompressed, "abcdefcdefcdefABCDDDDD");
    }

    #[test]
    fn decompression_failure_offset() {
        let inp: Vec<_> = vec![
            Token::literals("abcdef"),
            vec![Token::new_match(4, 8)],
            Token::literals("ABCD"),
            vec![Token::new_match(15000, 4)]
        ].into_iter().flatten().collect();

        let res = decompress(inp.into_iter(), DecompressionSettings::default());

        let err = res.unwrap_err();

        assert!(matches!(err, Error::InvalidOffset {..}))
    }

    #[test]
    fn decompression_failure_too_big() {
        let inp: Vec<_> = vec![
            Token::literals("abcdef"),
            vec![Token::new_match(4, 8)],
            Token::literals("ABCD"),
            vec![Token::new_match(1, 4)]
        ].into_iter().flatten().collect();

        let res = decompress(inp.into_iter(), DecompressionSettings { max_output_len: 10 });

        let err = res.unwrap_err();

        assert!(matches!(err, Error::MaximumSizeExceeded))
    }

    #[test]
    fn compression_test() {
        let inp = "Abcdefgefgefg";
        let res = compress(inp, CompressionSettings::test_default());
        println!("{:?}", &res);
        let out = decompress(res.into_iter(), DecompressionSettings::default())
            .unwrap();

        assert_eq!(String::from_utf8(out).unwrap(), inp);
    }

    #[test]
    fn large_compression_test() {
        let inp = std::fs::read_to_string("resources/asyoulik.txt").unwrap();
        let res = compress(&inp, CompressionSettings::test_default());
        println!("{:?}", &res);
        let out = decompress(res.into_iter(), DecompressionSettings::default())
            .unwrap();

        assert_eq!(String::from_utf8(out).unwrap(), inp);
    }
}