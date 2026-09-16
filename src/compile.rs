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
/// otherwise the transition cell is sacrificed and reported. Changes to the
/// background of a reused blank are also reported.
pub fn compile_visual_row(cells: &[VisualCell]) -> CompiledRow {
    let mut bytes = [b' '; COLS];
    let mut warnings = Vec::new();
    let mut state = State::default();
    for column in 0..COLS.min(cells.len()) {
        let target = cells[column];
        let mut controls = transition_controls(state, target);
        if controls.is_empty() {
            bytes[column] = target.ch & 0x7f;
            continue;
        }
        if target.ch == b' '
            && let Some(index) = controls
                .iter()
                .position(|&code| matches!(code, 0x1c | 0x1d))
            && !controls[index + 1..]
                .iter()
                .any(|&code| matches!(code, 0x00..=0x07 | 0x10..=0x17))
        {
            // Background can follow independent flash/size controls, but it
            // must precede a colour restore because it latches foreground.
            let background = controls.remove(index);
            controls.push(background);
        }
        let current_slot = target.ch == b' '
            && matches!(
                controls.last(),
                Some(0x09 | 0x0c | 0x18 | 0x19 | 0x1a | 0x1c | 0x1d)
            );
        // Level 1 can express only one control per transmitted cell. Walk
        // backwards over blank cells, preserving the requested text whenever
        // sufficient room exists.
        let mut start = column;
        while start > 0
            && column - start < controls.len() - usize::from(current_slot)
            && bytes[start - 1] == b' '
            && cells[start - 1].ch == b' '
        {
            start -= 1;
        }
        let reused = column - start;
        for (slot, &control) in (start..column).zip(&controls) {
            bytes[slot] = control;
            apply_control(&mut state, control);
            warn_blank_background(&mut warnings, slot, cells[slot], state);
        }
        if reused == controls.len() {
            bytes[column] = target.ch & 0x7f;
        } else {
            // Use every available preceding blank before sacrificing this
            // cell. Stop at existing text or controls to preserve their state.
            bytes[column] = controls[reused];
            apply_control(&mut state, controls[reused]);
            warn_blank_background(&mut warnings, column, target, state);
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

fn warn_blank_background(
    warnings: &mut Vec<CompileWarning>,
    column: usize,
    target: VisualCell,
    state: State,
) {
    // Background controls are set-at: the new background is already visible
    // in the cell occupied by the control, even when its glyph is a space.
    if target.ch == b' ' && state.bg != target.bg.min(7) {
        warnings.push(CompileWarning {
            column,
            message: "attribute transition changes this blank cell's background".into(),
        });
    }
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
