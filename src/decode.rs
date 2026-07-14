use crate::formats::{COLS, Page, ROWS};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CellSize {
    #[default]
    Normal,
    DoubleTop,
    DoubleBottom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: u8,
    pub bg: u8,
    pub flash: bool,
    pub conceal: bool,
    pub size: CellSize,
    pub separated: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: 7,
            bg: 0,
            flash: false,
            conceal: false,
            size: CellSize::Normal,
            separated: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Grid {
    cells: [[Cell; COLS]; ROWS],
}

impl Grid {
    pub fn rows(&self) -> &[[Cell; COLS]; ROWS] {
        &self.cells
    }
    pub fn cell(&self, row: usize, column: usize) -> Option<&Cell> {
        self.cells.get(row)?.get(column)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DecodeOptions {
    pub reveal: bool,
}

#[derive(Clone, Copy)]
struct Held {
    ch: char,
    separated: bool,
}

#[derive(Clone, Copy)]
struct State {
    mosaic: bool,
    fg: u8,
    bg: u8,
    flash: bool,
    conceal: bool,
    separated: bool,
    double: bool,
    hold: bool,
    held: Option<Held>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            mosaic: false,
            fg: 7,
            bg: 0,
            flash: false,
            conceal: false,
            separated: false,
            double: false,
            hold: false,
            held: None,
        }
    }
}

pub fn decode(page: &Page, options: &DecodeOptions) -> Grid {
    let mut cells = [[Cell::default(); COLS]; ROWS];
    let mut suppressed = [false; ROWS];
    for row in 0..ROWS {
        if suppressed[row] {
            continue;
        }
        let decoded = decode_row(&page.bytes[row], options);
        let has_double = decoded.iter().any(|cell| cell.size == CellSize::DoubleTop);
        cells[row] = decoded;
        if has_double && row + 1 < ROWS {
            suppressed[row + 1] = true;
            let (above, below) = cells.split_at_mut(row + 1);
            for (top, bottom) in above[row].iter().copied().zip(below[0].iter_mut()) {
                *bottom = Cell {
                    ch: if top.size == CellSize::DoubleTop {
                        top.ch
                    } else {
                        ' '
                    },
                    size: if top.size == CellSize::DoubleTop {
                        CellSize::DoubleBottom
                    } else {
                        CellSize::Normal
                    },
                    ..top
                };
            }
        }
    }
    Grid { cells }
}

fn decode_row(bytes: &[u8; COLS], options: &DecodeOptions) -> [Cell; COLS] {
    let mut output = [Cell::default(); COLS];
    let mut state = State::default();
    for (column, raw) in bytes.iter().enumerate() {
        let code = raw & 0x7f;
        let set_at = matches!(code, 0x09 | 0x0c | 0x18 | 0x19 | 0x1a | 0x1c | 0x1d | 0x1e);
        if code < 0x20 && set_at {
            apply(code, &mut state);
        }
        let mut cell = current_cell(&state);
        if code < 0x20 {
            if state.mosaic
                && state.hold
                && let Some(held) = state.held
            {
                cell.ch = held.ch;
                cell.separated = held.separated;
            }
        } else if state.mosaic && !(0x40..=0x5f).contains(&code) {
            let mask = mosaic_mask(code);
            cell.ch = sextant(mask);
            state.held = (code & 0x20 != 0).then_some(Held {
                ch: cell.ch,
                separated: state.separated,
            });
        } else {
            cell.ch = g0(code);
        }
        if state.conceal && !options.reveal {
            cell.ch = ' ';
        }
        output[column] = cell;
        if code < 0x20 && !set_at {
            apply(code, &mut state);
        }
    }
    output
}

fn current_cell(state: &State) -> Cell {
    Cell {
        ch: ' ',
        fg: state.fg,
        bg: state.bg,
        flash: state.flash,
        conceal: state.conceal,
        size: if state.double {
            CellSize::DoubleTop
        } else {
            CellSize::Normal
        },
        separated: state.separated,
    }
}

fn apply(code: u8, state: &mut State) {
    match code {
        0x00..=0x07 => set_mode_colour(state, false, code),
        0x08 => state.flash = true,
        0x09 => state.flash = false,
        0x0a | 0x0b => {}
        0x0c => set_size(state, false),
        0x0d => set_size(state, true),
        0x0e | 0x0f => {}
        0x10..=0x17 => set_mode_colour(state, true, code - 0x10),
        0x18 => state.conceal = true,
        0x19 => state.separated = false,
        0x1a => state.separated = true,
        0x1b => {}
        0x1c => state.bg = 0,
        0x1d => state.bg = state.fg,
        0x1e => state.hold = true,
        0x1f => state.hold = false,
        _ => unreachable!(),
    }
}

fn set_mode_colour(state: &mut State, mosaic: bool, colour: u8) {
    if state.mosaic != mosaic {
        state.held = None;
    }
    state.mosaic = mosaic;
    state.fg = colour;
    state.conceal = false;
}

fn set_size(state: &mut State, double: bool) {
    if state.double != double {
        state.held = None;
    }
    state.double = double;
}

pub(crate) fn mosaic_mask(code: u8) -> u8 {
    match code {
        0x20..=0x3f => code - 0x20,
        0x60..=0x7f => code - 0x60 + 32,
        _ => 0,
    }
}

pub(crate) fn sextant(mask: u8) -> char {
    match mask {
        0 => ' ',
        21 => '▌',
        42 => '▐',
        63 => '█',
        1..=20 => char::from_u32(0x1fb00 + mask as u32 - 1).unwrap(),
        22..=41 => char::from_u32(0x1fb00 + mask as u32 - 2).unwrap(),
        43..=62 => char::from_u32(0x1fb00 + mask as u32 - 3).unwrap(),
        _ => ' ',
    }
}

fn g0(code: u8) -> char {
    match code {
        0x23 => '£',
        0x5b => '←',
        0x5c => '½',
        0x5d => '→',
        0x5e => '↑',
        0x5f => '#',
        0x60 => '—',
        0x7b => '¼',
        0x7c => '‖',
        0x7d => '¾',
        0x7e => '÷',
        0x7f => '■',
        0x20..=0x7e => code as char,
        _ => ' ',
    }
}
