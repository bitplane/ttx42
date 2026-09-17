use crate::formats::COLS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// One requested authoring cell, expressed as a seven-bit teletext code and
/// attributes. Defaults to a normal white-on-black alpha space. On spaces only
/// background and double height are requirements. The compiler ignores the
/// supplied remaining attributes and may set them to prepare later text.
pub struct VisualCell {
    /// Seven-bit display code. The parity bit is ignored; control codes are
    /// replaced by spaces with a warning rather than interpreted as attributes.
    pub ch: u8,
    /// Foreground colour index; values above 7 are clamped to 7.
    pub fg: u8,
    /// Background colour index; values above 7 are clamped to 7.
    pub bg: u8,
    /// Request flashing text or graphics.
    pub flash: bool,
    /// Request text or graphics hidden until revealed.
    pub conceal: bool,
    /// Interpret the display code in G1 mosaic mode, including alpha capitals.
    pub mosaic: bool,
    /// Request gaps between mosaic blocks. Ignored when mosaic mode is off.
    pub separated: bool,
    /// Request the upper half of a double-height character or blank.
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
/// A display compromise or invalid glyph encountered while compiling a row.
pub struct CompileWarning {
    /// Zero-based source column affected by the warning.
    pub column: usize,
    /// Human-readable explanation; wording is not a stable machine interface.
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A transmitted 40-byte row and its compilation diagnostics.
pub struct CompiledRow {
    /// Seven-bit display codes and spacing controls, padded with spaces.
    pub bytes: [u8; COLS],
    /// Reported glyph substitutions, consumed text cells, and changed blank backgrounds or heights.
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
/// background or height of a blank are also reported.
///
/// Only the first 40 input cells are used; shorter inputs are space-padded.
/// Padding inherits the final transmitted attributes, including background
/// and double height. Supply explicit blank cells when the remaining columns
/// need a different background or height.
///
/// Control codes supplied as glyphs become spaces with warnings. Blank cells
/// constrain only background and double height. The compiler ignores the
/// supplied remaining attributes and may set them to prepare later text.
/// The compiler is greedy and does not use hold mosaics or guarantee the
/// smallest possible number of controls.
///
/// ```
/// use ttx42::{VisualCell, compile_visual_row};
/// let row = [
///     VisualCell::default(),
///     VisualCell { ch: b'R', fg: 1, ..VisualCell::default() },
/// ];
/// let compiled = compile_visual_row(&row);
/// assert_eq!(&compiled.bytes[..2], &[0x01, b'R']);
/// assert!(compiled.warnings.is_empty());
/// ```
pub fn compile_visual_row(cells: &[VisualCell]) -> CompiledRow {
    let mut bytes = [b' '; COLS];
    let mut warnings = Vec::new();
    let cells: Vec<_> = cells
        .iter()
        .take(COLS)
        .enumerate()
        .map(|(column, cell)| {
            let mut cell = *cell;
            cell.ch &= 0x7f;
            if cell.ch < 0x20 {
                cell.ch = b' ';
                warnings.push(CompileWarning {
                    column,
                    message: "control code in display character replaced with a space".into(),
                });
            }
            cell
        })
        .collect();
    let mut state = State::default();
    for column in 0..COLS.min(cells.len()) {
        let target = cells[column];
        // None means the foreground, mode and effects are unconstrained.
        // In particular, latching a background must not restore an old colour
        // just to draw a blank. A later glyph can supply preparation hints.
        let appearance = if target.ch == b' ' {
            cells[column + 1..]
                .iter()
                .find(|cell| cell.ch != b' ')
                .filter(|cell| cell.bg.min(7) == target.bg.min(7))
                .copied()
                .map(state_for)
        } else {
            Some(state_for(target))
        };
        let mut controls = transition_controls(state, target, appearance);
        if target.ch == b' '
            && cells.get(column + 1).is_some_and(|cell| cell.ch == b' ')
            && let Some(index) = controls
                .iter()
                .position(|&code| matches!(code, 0x1c | 0x1d))
            && !controls[index + 1..]
                .iter()
                .any(|&code| matches!(code, 0x0c | 0x0d))
        {
            // Finish the observable background here. Colour restores and
            // effects can use the following blanks instead of pulling this
            // set-at control into an earlier cell.
            controls.truncate(index + 1);
        }
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
    // Validate the final row: later transitions can reuse an earlier blank.
    // Double height is set-after, whereas normal height is set-at.
    let mut double_height = false;
    for (column, target) in cells.iter().enumerate() {
        if bytes[column] == 0x0c {
            double_height = false;
        }
        if target.ch == b' ' && double_height != target.double_height {
            warnings.push(CompileWarning {
                column,
                message: "attribute transition changes this blank cell's height".into(),
            });
        }
        if bytes[column] == 0x0d {
            double_height = true;
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

fn transition_controls(mut state: State, cell: VisualCell, appearance: Option<State>) -> Vec<u8> {
    let bg = cell.bg.min(7);
    let mut out = Vec::new();
    // Background is latched independently of subsequent foreground changes.
    // Establish it first so partial transitions also make forward progress.
    if state.bg != bg {
        if bg != 0 && state.fg != bg {
            let mosaic = appearance.map_or(state.mosaic, |target| target.mosaic);
            let colour = if mosaic { 0x10 } else { 0 } + bg;
            out.push(colour);
            apply_control(&mut state, colour);
        }
        let background = if bg == 0 { 0x1c } else { 0x1d };
        out.push(background);
        apply_control(&mut state, background);
    }
    let target = State {
        bg,
        double_height: cell.double_height,
        ..appearance.unwrap_or(state)
    };
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
    if target.mosaic && state.separated != target.separated {
        out.push(if target.separated { 0x1a } else { 0x19 });
    }
    if !state.conceal && target.conceal {
        out.push(0x18);
    }
    out
}
