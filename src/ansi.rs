use std::fmt::Write;

use crate::{CellSize, Grid};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationCell {
    pub glyphs: String,
    pub width: u8,
    pub fg: u8,
    pub bg: u8,
    pub flash: bool,
    pub conceal: bool,
}

pub type PresentationGrid = Vec<Vec<PresentationCell>>;

/// Render semantic teletext cells without terminal escape sequences. In wide
/// mode every source cell contributes exactly two terminal cells.
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
pub enum SeparatedStyle {
    #[default]
    Braille,
    Contiguous,
    Unicode16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnsiOptions {
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
        SeparatedStyle::Unicode16 => unicode16(mask).unwrap_or_else(|| braille(mask)),
    }
}

fn char_mask(ch: char) -> Option<u8> {
    (0..64).find(|&mask| crate::decode::sextant(mask) == ch)
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
        output.push(if separated {
            match style {
                SeparatedStyle::Braille => braille(mask),
                SeparatedStyle::Contiguous => crate::decode::sextant(mask),
                SeparatedStyle::Unicode16 => unicode16(mask).unwrap_or_else(|| braille(mask)),
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

fn unicode16(mask: u8) -> Option<char> {
    // Unicode 16 Symbols for Legacy Computing Supplement. The assigned
    // sequence U+1CE51..U+1CE8F follows the sextant mask numerically.
    (mask != 0).then(|| char::from_u32(0x1ce50 + mask as u32))?
}
