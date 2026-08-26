//! Level 1 broadcast teletext decoding with a presentation-independent grid.

mod ansi;
mod compile;
mod decode;
mod formats;
mod saa5050;
mod sn8k5050;

pub use ansi::{AnsiOptions, PresentationCell, PresentationGrid, SeparatedStyle, present, to_ansi};
pub use compile::{CompileWarning, CompiledRow, VisualCell, compile_visual_row};
pub use decode::{Cell, CellSize, DecodeOptions, Grid, decode, mosaic_code, mosaic_mask};
pub use formats::{FastTextLinks, Page, Service, TtiRecord};

#[cfg(test)]
mod tests;

use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidRawLength(usize),
    InvalidTti(String),
    NoPages,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRawLength(len) => {
                write!(f, "raw page must be 960 or 1000 bytes, got {len}")
            }
            Self::InvalidTti(message) => write!(f, "invalid TTI: {message}"),
            Self::NoPages => write!(f, "input contains no teletext pages"),
        }
    }
}

impl std::error::Error for Error {}
