//! Terminal octant encoding for the editable `sn8k5050` font.

pub(crate) fn glyph(ch: char) -> Option<[[char; 2]; 2]> {
    let pixels = pixels(ch)?;
    let mut output = [[' '; 2]; 2];
    for (cell_row, output_row) in output.iter_mut().enumerate() {
        for (cell_column, output_cell) in output_row.iter_mut().enumerate() {
            let mut mask = 0;
            for y in 0..4 {
                for x in 0..2 {
                    if pixels[cell_row * 4 + y] & (1 << (3 - (cell_column * 2 + x))) != 0 {
                        mask |= 1 << (y * 2 + x);
                    }
                }
            }
            *output_cell = octant(mask);
        }
    }
    Some(output)
}

pub(crate) fn pixels(ch: char) -> Option<[u8; 8]> {
    crate::sn8k5050::font_pixels(ch)
}

fn octant(mask: u8) -> char {
    let special = match mask {
        0x00 => Some(0x0020),
        0x01 => Some(0x1cea8),
        0x02 => Some(0x1ceab),
        0x03 => Some(0x1fb82),
        0x05 => Some(0x2598),
        0x0a => Some(0x259d),
        0x0f => Some(0x2580),
        0x14 => Some(0x1fbe6),
        0x28 => Some(0x1fbe7),
        0x3f => Some(0x1fb85),
        0x40 => Some(0x1cea3),
        0x50 => Some(0x2596),
        0x55 => Some(0x258c),
        0x5a => Some(0x259e),
        0x5f => Some(0x259b),
        0x80 => Some(0x1cea0),
        0xa0 => Some(0x2597),
        0xa5 => Some(0x259a),
        0xaa => Some(0x2590),
        0xaf => Some(0x259c),
        0xc0 => Some(0x2582),
        0xf0 => Some(0x2584),
        0xf5 => Some(0x2599),
        0xfa => Some(0x259f),
        0xfc => Some(0x2586),
        0xff => Some(0x2588),
        _ => None,
    };
    let codepoint = special.unwrap_or_else(|| {
        const SKIPPED: [u8; 26] = [
            0x00, 0x01, 0x02, 0x03, 0x05, 0x0a, 0x0f, 0x14, 0x28, 0x3f, 0x40, 0x50, 0x55, 0x5a,
            0x5f, 0x80, 0xa0, 0xa5, 0xaa, 0xaf, 0xc0, 0xf0, 0xf5, 0xfa, 0xfc, 0xff,
        ];
        0x1cd00 + u32::from(mask) - SKIPPED.iter().filter(|&&value| value < mask).count() as u32
    });
    char::from_u32(codepoint).unwrap()
}
