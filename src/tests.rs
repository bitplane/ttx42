use crate::ansi::braille;
use crate::formats::{encode_hamming84, hamming84, parity_data};
use crate::{AnsiOptions, CellSize, DecodeOptions, Page, SeparatedStyle, decode, to_ansi};

fn page_with_row(row: usize, data: &[u8]) -> Page {
    let mut raw = vec![b' '; 1000];
    raw[row * 40..row * 40 + data.len()].copy_from_slice(data);
    Page::from_raw(&raw).unwrap()
}

#[test]
fn colours_and_background_obey_set_after_and_set_at() {
    let page = page_with_row(0, &[0x01, b'R', 0x1d, b'X']);
    let grid = decode(&page, &DecodeOptions::default());
    assert_eq!(
        (grid.cell(0, 0).unwrap().fg, grid.cell(0, 0).unwrap().bg),
        (7, 0)
    );
    assert_eq!(
        (grid.cell(0, 1).unwrap().ch, grid.cell(0, 1).unwrap().fg),
        ('R', 1)
    );
    assert_eq!(
        (grid.cell(0, 2).unwrap().fg, grid.cell(0, 2).unwrap().bg),
        (1, 1)
    );
}

#[test]
fn flash_is_set_after_and_steady_is_set_at() {
    let grid = decode(
        &page_with_row(0, &[0x08, b'F', 0x09, b'S']),
        &DecodeOptions::default(),
    );
    assert!(!grid.cell(0, 0).unwrap().flash);
    assert!(grid.cell(0, 1).unwrap().flash);
    assert!(!grid.cell(0, 2).unwrap().flash);
    assert!(!grid.cell(0, 3).unwrap().flash);
}

#[test]
fn mosaic_colour_is_set_after_and_separation_is_set_at() {
    let grid = decode(
        &page_with_row(0, &[0x11, 0x1a, 0x61]),
        &DecodeOptions::default(),
    );
    assert_eq!(grid.cell(0, 0).unwrap().fg, 7);
    assert_eq!(grid.cell(0, 1).unwrap().fg, 1);
    assert!(grid.cell(0, 1).unwrap().separated);
    assert!(grid.cell(0, 2).unwrap().separated);
}

#[test]
fn hold_is_set_at_and_release_is_set_after() {
    let grid = decode(
        &page_with_row(0, &[0x11, 0x61, 0x1e, 0x12, 0x1f, 0x13]),
        &DecodeOptions::default(),
    );
    let mosaic = grid.cell(0, 1).unwrap().ch;
    assert_eq!(grid.cell(0, 2).unwrap().ch, mosaic);
    assert_eq!(grid.cell(0, 3).unwrap().ch, mosaic);
    assert_eq!(grid.cell(0, 4).unwrap().ch, mosaic);
    assert_eq!(grid.cell(0, 5).unwrap().ch, ' ');
}

#[test]
fn size_changes_reset_held_mosaic() {
    let grid = decode(
        &page_with_row(0, &[0x11, 0x61, 0x1e, 0x0d, 0x12]),
        &DecodeOptions::default(),
    );
    assert_ne!(grid.cell(0, 2).unwrap().ch, ' ');
    assert_eq!(grid.cell(0, 4).unwrap().ch, ' ');
}

#[test]
fn hold_preserves_original_separation_and_resets_on_mode_change() {
    let page = page_with_row(0, &[0x11, 0x1a, 0x61, 0x1e, 0x19, 0x12, 0x01, 0x11, 0x1e]);
    let grid = decode(&page, &DecodeOptions::default());
    assert!(grid.cell(0, 4).unwrap().separated);
    assert_ne!(grid.cell(0, 4).unwrap().ch, ' ');
    assert_ne!(grid.cell(0, 6).unwrap().ch, ' ');
    assert_eq!(grid.cell(0, 7).unwrap().ch, ' ');
    assert_eq!(grid.cell(0, 8).unwrap().ch, ' ');
}

#[test]
fn blast_through_cap_does_not_replace_held_mosaic() {
    let page = page_with_row(0, &[0x11, 0x61, 0x1e, b'A', 0x12]);
    let grid = decode(&page, &DecodeOptions::default());
    assert_eq!(grid.cell(0, 3).unwrap().ch, 'A');
    assert_eq!(grid.cell(0, 4).unwrap().ch, grid.cell(0, 1).unwrap().ch);
}

#[test]
fn conceal_keeps_flag_and_reveal_controls_character() {
    let page = page_with_row(0, &[0x18, b'X', 0x02, b'Y']);
    let hidden = decode(&page, &DecodeOptions::default());
    assert_eq!(hidden.cell(0, 1).unwrap().ch, ' ');
    assert!(hidden.cell(0, 1).unwrap().conceal);
    let revealed = decode(&page, &DecodeOptions { reveal: true });
    assert_eq!(revealed.cell(0, 1).unwrap().ch, 'X');
    assert!(!revealed.cell(0, 3).unwrap().conceal);
}

#[test]
fn double_height_generates_bottom_and_suppresses_transmitted_row() {
    let mut raw = vec![b' '; 1000];
    raw[..4].copy_from_slice(&[b'n', 0x0d, b'D', 0x0c]);
    raw[40] = b'Z';
    let grid = decode(&Page::from_raw(&raw).unwrap(), &DecodeOptions::default());
    assert_eq!(grid.cell(0, 2).unwrap().size, CellSize::DoubleTop);
    assert_eq!(grid.cell(1, 2).unwrap().size, CellSize::DoubleBottom);
    assert_eq!(grid.cell(1, 2).unwrap().ch, 'D');
    assert_eq!(grid.cell(1, 0).unwrap().ch, ' ');
}

#[test]
fn double_height_on_last_row_has_no_bottom() {
    let page = page_with_row(24, &[0x0d, b'X']);
    assert_eq!(
        decode(&page, &DecodeOptions::default())
            .cell(24, 1)
            .unwrap()
            .size,
        CellSize::DoubleTop
    );
}

#[test]
fn saa5050_t_has_a_full_top_bar_and_centred_stem() {
    assert_eq!(
        crate::saa5050::pixels('T').unwrap(),
        [0b1111, 0b0100, 0b0100, 0b0100, 0b0100, 0b0100, 0b0100, 0]
    );
}

#[test]
fn octant_digits_zero_and_one_have_clean_shapes() {
    assert_eq!(
        crate::saa5050::pixels('0').unwrap(),
        [0b0110, 0b1001, 0b1001, 0b1001, 0b1001, 0b1001, 0b0110, 0]
    );
    assert_eq!(
        crate::saa5050::pixels('1').unwrap(),
        [0b0010, 0b0110, 0b0010, 0b0010, 0b0010, 0b0010, 0b0010, 0]
    );
}

#[test]
fn wide_ansi_renders_both_halves_of_double_height_text() {
    let grid = decode(&page_with_row(0, &[0x0d, b'T']), &DecodeOptions::default());
    let glyph = crate::saa5050::glyph('T').unwrap();
    let ansi = to_ansi(&grid, &AnsiOptions::default());
    assert!(
        ansi.lines()
            .next()
            .unwrap()
            .contains(&glyph[0].iter().collect::<String>())
    );
    assert!(
        ansi.lines()
            .nth(1)
            .unwrap()
            .contains(&glyph[1].iter().collect::<String>())
    );
}

#[test]
fn uk_g0_subset_maps_specials() {
    let page = page_with_row(0, b"#[\\]^_`{|}~");
    let chars: String = decode(&page, &DecodeOptions::default()).rows()[0][..12]
        .iter()
        .map(|cell| cell.ch)
        .collect();
    assert_eq!(chars, "£←½→↑#—¼‖¾÷ ");
}

#[test]
fn braille_uses_column_major_dot_order() {
    assert_eq!(braille(1), '⠁');
    assert_eq!(braille(4), '⠂');
    assert_eq!(braille(16), '⡄');
    assert_eq!(braille(2), '⠈');
    assert_eq!(braille(8), '⠐');
    assert_eq!(braille(32), '⢠');
    assert_eq!(braille(16 | 32), '⣤');
}

#[test]
fn unicode16_uses_the_assigned_separated_sextants() {
    assert_eq!(
        crate::ansi::separated_char(crate::decode::sextant(1), SeparatedStyle::Unicode16),
        '\u{1ce51}'
    );
    assert_eq!(
        crate::ansi::separated_char(crate::decode::sextant(63), SeparatedStyle::Unicode16),
        '\u{1ce8f}'
    );
}

#[test]
fn ansi_uses_basic_colours_and_resets_every_row() {
    let grid = decode(&page_with_row(0, &[0x01, b'R']), &DecodeOptions::default());
    let ansi = to_ansi(
        &grid,
        &AnsiOptions {
            separated: SeparatedStyle::Braille,
            wide: false,
        },
    );
    assert!(ansi.starts_with("\x1b[37;40m \x1b[31;40mR"));
    assert_eq!(ansi.matches("\x1b[0m\n").count(), 25);
    assert!(!ansi.contains("38;5"));
}

#[test]
fn wide_ansi_uses_fullwidth_text_and_doubles_mosaics() {
    let page = page_with_row(0, &[b'A', b' ', 0x11, 0x1a, 0x61]);
    let grid = decode(&page, &DecodeOptions::default());
    let ansi = to_ansi(
        &grid,
        &AnsiOptions {
            separated: SeparatedStyle::Braille,
            wide: true,
        },
    );
    assert!(ansi.starts_with("\x1b[37;40mＡ  "));
    assert!(ansi.contains("⠉⣤"));
}

#[test]
fn wide_mosaic_stretches_columns_instead_of_repeating_the_mask() {
    assert_eq!(crate::ansi::stretch_mask(1), (3, 0));
    assert_eq!(crate::ansi::stretch_mask(2), (0, 3));
    assert_eq!(crate::ansi::stretch_mask(4 | 32), (12, 48));
    assert_eq!(crate::ansi::stretch_mask(63), (63, 63));
}

#[test]
fn hamming_corrects_one_bit_and_rejects_two() {
    for nibble in 0..16 {
        let encoded = encode_hamming84(nibble);
        assert_eq!(hamming84(encoded), Some(nibble));
        assert_eq!(hamming84(encoded ^ 1), Some(nibble));
    }
    assert_eq!(hamming84(encode_hamming84(3) ^ 3), None);
}

#[test]
fn parity_strips_good_bytes_and_blanks_bad_bytes() {
    assert_eq!(parity_data(0xc1), b'A');
    assert_eq!(parity_data(0x41), b' ');
}

#[test]
fn tti_decodes_escaped_controls_and_ignores_unknown_keys() {
    let pages = Page::parse_tti("PN,10003\r\nSC,0123\r\nXX,junk\r\nOL,1,\x1bAR\r\n").unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].page_number(), Some(0x100));
    assert_eq!(pages[0].subpage_number(), Some(0x123));
    assert_eq!(&pages[0].raw()[1][..2], &[1, b'R']);
}

#[test]
fn exact_ansi_golden_for_plain_text_page() {
    let ansi = to_ansi(
        &decode(&page_with_row(0, b"HI"), &DecodeOptions::default()),
        &AnsiOptions {
            wide: false,
            ..AnsiOptions::default()
        },
    );
    let row = format!("\x1b[37;40mHI{}\x1b[0m\n", " ".repeat(38));
    assert_eq!(ansi, row.clone() + &row.replace("HI", "  ").repeat(24));
}

#[test]
fn exact_ansi_golden_for_coloured_mosaic_page() {
    let ansi = to_ansi(
        &decode(
            &page_with_row(0, &[0x11, 0x1a, 0x61]),
            &DecodeOptions::default(),
        ),
        &AnsiOptions {
            wide: false,
            ..AnsiOptions::default()
        },
    );
    let first = format!("\x1b[37;40m \x1b[31;40m ⢡{}\x1b[0m\n", " ".repeat(37));
    let blank = format!("\x1b[37;40m{}\x1b[0m\n", " ".repeat(40));
    assert_eq!(ansi, first + &blank.repeat(24));
}

#[test]
fn t42_assembles_interleaved_magazines_and_survives_garbage() {
    fn packet(magazine: u8, row: u8, fill: u8) -> [u8; 42] {
        let address = magazine | (row << 3);
        let mut packet = [fill; 42];
        packet[0] = encode_hamming84(address & 15);
        packet[1] = encode_hamming84(address >> 4);
        if row == 0 {
            for byte in &mut packet[2..10] {
                *byte = encode_hamming84(0);
            }
        }
        packet
    }
    let mut bytes = vec![0xff; 42];
    bytes.extend(packet(1, 0, 0));
    bytes.extend(packet(2, 0, 0));
    bytes.extend(packet(1, 1, 0xc1));
    bytes.extend(packet(2, 1, 0xc2));
    let pages = Page::parse_t42(&bytes).unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].raw()[1][0], b'A');
    assert_eq!(pages[1].raw()[1][0], b'B');
}
