use std::fmt::Write;

use crate::{CellSize, Grid};

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
            if options.wide
                && matches!(cell.size, CellSize::DoubleTop | CellSize::DoubleBottom)
                && let Some(glyph) = crate::saa5050::glyph(cell.ch)
            {
                let half = usize::from(cell.size == CellSize::DoubleBottom);
                output.extend(glyph[half]);
                continue;
            }
            let ch = if cell.size == CellSize::DoubleBottom {
                ' '
            } else if cell.separated {
                separated_char(cell.ch, options.separated)
            } else {
                cell.ch
            };
            let mosaic = cell.separated || char_mask(cell.ch).is_some_and(|mask| mask != 0);
            if options.wide && mosaic {
                push_wide_mosaic(
                    &mut output,
                    char_mask(cell.ch).unwrap_or(0),
                    cell.separated,
                    options.separated,
                );
            } else {
                push_cell(&mut output, ch, options.wide);
            }
        }
        output.push_str("\x1b[0m\n");
    }
    output
}

pub(crate) fn separated_char(ch: char, style: SeparatedStyle) -> char {
    let mask = char_mask(ch).unwrap_or(0);
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
