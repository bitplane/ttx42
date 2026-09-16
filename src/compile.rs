use crate::formats::COLS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VisualCell {
    pub ch: u8,
    pub fg: u8,
    pub bg: u8,
    pub flash: bool,
    pub conceal: bool,
    pub mosaic: bool,
    pub separated: bool,
    pub double_height: bool,
}

impl Default for VisualCell {
    fn default() -> Self {
        Self {
            ch: b' ',
            fg: 7,
            bg: 0,
            flash: false,
            conceal: false,
            mosaic: false,
            separated: false,
            double_height: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileWarning {
    pub column: usize,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledRow {
    pub bytes: [u8; COLS],
    pub warnings: Vec<CompileWarning>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct State {
    fg: u8,
    bg: u8,
    flash: bool,
    conceal: bool,
    mosaic: bool,
    separated: bool,
    double_height: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            fg: 7,
            bg: 0,
            flash: false,
            conceal: false,
            mosaic: false,
            separated: false,
            double_height: false,
        }
    }
}

/// Compile a WYSIWYG row to a real Level 1 control-code row. Attribute
/// transitions consume cells; a preceding blank is used where possible,
/// otherwise the transition cell is sacrificed and reported.
pub fn compile_visual_row(cells: &[VisualCell]) -> CompiledRow {
    let mut bytes = [b' '; COLS];
    let mut warnings = Vec::new();
    let mut state = State::default();
    for column in 0..COLS.min(cells.len()) {
        let target = cells[column];
        let controls = transition_controls(state, target);
        if controls.is_empty() {
            bytes[column] = target.ch & 0x7f;
            continue;
        }
        // Level 1 can express only one control per transmitted cell. Walk
        // backwards over blank cells, preserving the requested text whenever
        // sufficient room exists.
        let start = column.saturating_sub(controls.len());
        let slots: Vec<_> = (start..column)
            .filter(|&slot| {
                bytes[slot] == b' ' && cells.get(slot).is_none_or(|cell| cell.ch == b' ')
            })
            .collect();
        let enough = slots.len() >= controls.len();
        if enough {
            let chosen = &slots[slots.len() - controls.len()..];
            for (&slot, &control) in chosen.iter().zip(&controls) {
                bytes[slot] = control;
                apply_control(&mut state, control);
            }
            bytes[column] = target.ch & 0x7f;
        } else {
            // Never rewrite an earlier transmitted control. If there is not
            // enough blank space before this cell, emit the next transition
            // here and let following cells complete the state change.
            bytes[column] = controls[0];
            apply_control(&mut state, controls[0]);
            if target.ch != b' ' {
                warnings.push(CompileWarning {
                    column,
                    message: "attribute transition consumes this display cell".into(),
                });
            }
        }
    }
    CompiledRow { bytes, warnings }
}

fn apply_control(state: &mut State, control: u8) {
    match control {
        0x00..=0x07 => {
            state.mosaic = false;
            state.fg = control;
            state.conceal = false;
        }
        0x08 => state.flash = true,
        0x09 => state.flash = false,
        0x0c => state.double_height = false,
        0x0d => state.double_height = true,
        0x10..=0x17 => {
            state.mosaic = true;
            state.fg = control - 0x10;
            state.conceal = false;
        }
        0x18 => state.conceal = true,
        0x19 => state.separated = false,
        0x1a => state.separated = true,
        0x1c => state.bg = 0,
        0x1d => state.bg = state.fg,
        _ => {}
    }
}

fn state_for(cell: VisualCell) -> State {
    State {
        fg: cell.fg.min(7),
        bg: cell.bg.min(7),
        flash: cell.flash,
        conceal: cell.conceal,
        mosaic: cell.mosaic,
        separated: cell.separated,
        double_height: cell.double_height,
    }
}

fn transition_controls(mut state: State, target: VisualCell) -> Vec<u8> {
    let target = state_for(target);
    let mut out = Vec::new();
    // Background is latched independently of subsequent foreground changes.
    // Establish it first so partial transitions also make forward progress.
    if state.bg != target.bg {
        if target.bg != 0 && state.fg != target.bg {
            let colour = if target.mosaic { 0x10 } else { 0 } + target.bg;
            out.push(colour);
            apply_control(&mut state, colour);
        }
        let background = if target.bg == 0 { 0x1c } else { 0x1d };
        out.push(background);
        apply_control(&mut state, background);
    }
    if state.mosaic != target.mosaic || state.fg != target.fg || (state.conceal && !target.conceal)
    {
        out.push(if target.mosaic {
            0x10 + target.fg
        } else {
            target.fg
        });
        state.mosaic = target.mosaic;
        state.fg = target.fg;
        state.conceal = false;
    }
    if state.flash != target.flash {
        out.push(if target.flash { 0x08 } else { 0x09 });
    }
    if state.double_height != target.double_height {
        out.push(if target.double_height { 0x0d } else { 0x0c });
    }
    if state.separated != target.separated {
        out.push(if target.separated { 0x1a } else { 0x19 });
    }
    if !state.conceal && target.conceal {
        out.push(0x18);
    }
    out
}
