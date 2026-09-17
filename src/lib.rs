//! Level 1 broadcast teletext decoding with a presentation-independent grid.
//!
//! Parse raw pages, TTI files, or T42 packet captures into [`Page`]s, then
//! [`decode`] a page into a 25-row, 40-column [`Grid`]. The decoder uses the
//! UK G0 character set. [`present`] retains display metadata for graphical
//! consumers; [`to_ansi`] produces static terminal output.
//!
//! Colour indices throughout this crate are 0 black, 1 red, 2 green, 3 yellow,
//! 4 blue, 5 magenta, 6 cyan, and 7 white. Coordinates are zero-based.
//!
//! # Decode and render
//!
//! ```
//! use ttx42::{Page, DecodeOptions, AnsiOptions, decode, to_ansi};
//! let mut bytes = [b' '; 1000];
//! bytes[40..45].copy_from_slice(b"HELLO");
//! let page = Page::from_raw(&bytes)?;
//! let grid = decode(&page, &DecodeOptions::default());
//! assert_eq!(grid.cell(1, 0).unwrap().ch, 'H');
//! let terminal_output = to_ansi(&grid, &AnsiOptions::default());
//! assert_eq!(terminal_output.lines().count(), 25);
//! # Ok::<(), ttx42::Error>(())
//! ```
//!
//! # Edit a service
//!
//! ```
//! use ttx42::Service;
//! let mut service = Service::parse_tti("PN,10001\r\nOL,1,HELLO\r\n")?;
//! service.pages_mut()[0].raw_mut()[1][0] = b'J';
//! let saved = service.to_tti();
//! assert_eq!(Service::parse_tti(&saved)?, service);
//! # Ok::<(), ttx42::Error>(())
//! ```

#![warn(missing_docs)]

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
/// An invalid input or an input containing no recoverable pages.
pub enum Error {
    /// A raw buffer had a length other than 960 or 1000 bytes.
    InvalidRawLength(usize),
    /// A TTI page number or output-row record was malformed; contains details.
    InvalidTti(String),
    /// Parsing found no TTI pages or no decodable T42 page headers.
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
