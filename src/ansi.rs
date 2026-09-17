use std::fmt::Write;

use crate::{CellSize, Grid};

#[derive(Clone, Debug, Eq, PartialEq)]
/// Glyphs and attributes for one source cell, without ANSI escapes.
pub struct PresentationCell {
    /// Printable glyphs for the selected width and mosaic style.
    pub glyphs: String,
    /// Intended terminal-column width: one in narrow mode, two in wide mode.
    pub width: u8,
    /// Foreground colour index, 0 to 7.
    pub fg: u8,
    /// Background colour index, 0 to 7.
    pub bg: u8,
    /// Flash attribute for consumers that provide animation.
    pub flash: bool,
    /// Conceal attribute; reveal is controlled when creating the decoded grid.
    pub conceal: bool,
}

/// Presentation cells arranged as 25 rows of 40 source cells.
pub type PresentationGrid = Vec<Vec<PresentationCell>>;

/// Render semantic teletext cells without terminal escape sequences. In wide
/// mode every source cell contributes two terminal cells, assuming the
/// terminal displays ambiguous-width Unicode characters in a single column.
pub fn present(grid: &Grid, options: &AnsiOptions) -> PresentationGrid {
    grid.rows()
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| PresentationCell {
                    glyphs: cell_glyphs(cell, options),
                    width: if options.wide { 2 } else { 1 },
                    fg: cell.fg,
                    bg: cell.bg,
                    flash: cell.flash,
                    conceal: cell.conceal,
                })
                .collect()
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// Glyph family used for separated mosaic graphics.
pub enum SeparatedStyle {
    /// Braille dots, with the bottom dot row repeated for aspect ratio.
    #[default]
    Braille,
    /// Contiguous sextants, discarding separation gaps.
    Contiguous,
    /// Separated sextants from Unicode 16; requires a supporting font.
    Unicode16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Shared options for [`present`] and [`to_ansi`]. Defaults to wide braille.
pub struct AnsiOptions {
    /// Glyph family for separated mosaics; contiguous mosaics are unaffected.
    pub separated: SeparatedStyle,
    /// Render every teletext cell as exactly two terminal columns.
    pub wide: bool,
}

impl Default for AnsiOptions {
    fn default() -> Self {
        Self {
            separated: SeparatedStyle::default(),
            wide: true,
        }
    }
}

/// Render 25 newline-terminated rows using basic ANSI foreground/background
/// colours, resetting attributes at the end of each row. Flash is not emitted
/// as ANSI blink; conceal has already been applied by [`crate::decode`].
pub fn to_ansi(grid: &Grid, options: &AnsiOptions) -> String {
    let mut output = String::new();
    for row in grid.rows() {
        let mut colours = None;
        for cell in row {
            let next = (cell.fg, cell.bg);
            if colours != Some(next) {
                let _ = write!(output, "\x1b[{};{}m", 30 + cell.fg, 40 + cell.bg);
                colours = Some(next);
            }
            output.push_str(&cell_glyphs(cell, options));
        }
        output.push_str("\x1b[0m\n");
    }
    output
}

fn cell_glyphs(cell: &crate::Cell, options: &AnsiOptions) -> String {
    if options.wide
        && let Some(mask) = char_mask(cell.ch).filter(|&mask| cell.separated || mask != 0)
    {
        // Double each of the three mosaic rows across two terminal rows:
        // the top half contains rows A, A, B; the bottom contains B, C, C.
        let mask = match cell.size {
            CellSize::Normal => mask,
            CellSize::DoubleTop => (mask & 3) | ((mask & 3) << 2) | ((mask & 12) << 2),
            CellSize::DoubleBottom => ((mask & 12) >> 2) | ((mask & 48) >> 2) | (mask & 48),
        };
        let mut output = String::new();
        push_wide_mosaic(&mut output, mask, cell.separated, options.separated);
        return output;
    }
    if options.wide
        && matches!(cell.size, CellSize::DoubleTop | CellSize::DoubleBottom)
        && let Some(glyph) = crate::saa5050::glyph(cell.ch)
    {
        return glyph[usize::from(cell.size == CellSize::DoubleBottom)]
            .iter()
            .collect();
    }
    let ch = if cell.size == CellSize::DoubleBottom {
        ' '
    } else if cell.separated {
        separated_char(cell.ch, options.separated)
    } else {
        cell.ch
    };
    let mut output = String::new();
    push_cell(&mut output, ch, options.wide);
    output
}

pub(crate) fn separated_char(ch: char, style: SeparatedStyle) -> char {
    let Some(mask) = char_mask(ch) else {
        return ch;
    };
    if mask == 0 {
        return ' ';
    }
    match style {
        SeparatedStyle::Contiguous => ch,
        SeparatedStyle::Braille => braille(mask),
        SeparatedStyle::Unicode16 => unicode16(mask),
    }
}

fn char_mask(ch: char) -> Option<u8> {
    match ch {
        ' ' => Some(0),
        '▌' => Some(21),
        '▐' => Some(42),
        '█' => Some(63),
        '\u{1fb00}'..='\u{1fb13}' => Some((ch as u32 - 0x1fb00 + 1) as u8),
        '\u{1fb14}'..='\u{1fb27}' => Some((ch as u32 - 0x1fb00 + 2) as u8),
        '\u{1fb28}'..='\u{1fb3b}' => Some((ch as u32 - 0x1fb00 + 3) as u8),
        _ => None,
    }
}

fn push_cell(output: &mut String, ch: char, wide: bool) {
    if !wide {
        output.push(ch);
    } else if ch == ' ' {
        output.push_str("  ");
    } else if ch == '£' {
        output.push('￡');
    } else if ('!'..='~').contains(&ch) {
        output.push(char::from_u32(ch as u32 + 0xfee0).unwrap());
    } else {
        output.extend([ch, ' ']);
    }
}

fn push_wide_mosaic(output: &mut String, mask: u8, separated: bool, style: SeparatedStyle) {
    let (left, right) = stretch_mask(mask);
    for mask in [left, right] {
        output.push(if mask == 0 {
            ' '
        } else if separated {
            match style {
                SeparatedStyle::Braille => braille(mask),
                SeparatedStyle::Contiguous => crate::decode::sextant(mask),
                SeparatedStyle::Unicode16 => unicode16(mask),
            }
        } else {
            crate::decode::sextant(mask)
        });
    }
}

pub(crate) fn stretch_mask(mask: u8) -> (u8, u8) {
    let left = (((mask & 1 != 0) as u8) * 3)
        | (((mask & 4 != 0) as u8) * 12)
        | (((mask & 16 != 0) as u8) * 48);
    let right = (((mask & 2 != 0) as u8) * 3)
        | (((mask & 8 != 0) as u8) * 12)
        | (((mask & 32 != 0) as u8) * 48);
    (left, right)
}

pub(crate) fn braille(mask: u8) -> char {
    let bits = (mask & 1)
        | ((mask & 4) >> 1)
        | ((mask & 16) >> 2)
        | ((mask & 2) << 2)
        | ((mask & 8) << 1)
        | (mask & 32)
        | ((mask & 16) << 2)
        | ((mask & 32) << 2);
    char::from_u32(0x2800 + bits as u32).unwrap()
}

fn unicode16(mask: u8) -> char {
    // Unicode 16 Symbols for Legacy Computing Supplement. The assigned
    // sequence U+1CE51..U+1CE8F follows the sextant mask numerically.
    debug_assert!((1..=63).contains(&mask));
    char::from_u32(0x1ce50 + mask as u32).unwrap()
}

#[cfg(test)]
mod tests {
    #[test]
    fn sextant_lookup_inverts_all_masks_and_rejects_text() {
        for mask in 0..64 {
            assert_eq!(super::char_mask(crate::decode::sextant(mask)), Some(mask));
        }
        for ch in ['A', '£', '■', '—', '\u{1faff}', '\u{1fb3c}'] {
            assert_eq!(super::char_mask(ch), None);
        }
    }
}
