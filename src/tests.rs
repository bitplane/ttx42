use crate::ansi::braille;
use crate::formats::{encode_hamming84, hamming84, parity_data};
use crate::{AnsiOptions, CellSize, DecodeOptions, Page, SeparatedStyle, decode, to_ansi};
use crate::{FastTextLinks, Service, VisualCell, compile_visual_row, present};

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
fn every_printable_g0_character_has_a_double_height_font_glyph() {
    for code in 0x21..=0x7f {
        let grid = decode(&page_with_row(0, &[code]), &DecodeOptions::default());
        let ch = grid.cell(0, 0).unwrap().ch;
        assert!(
            crate::saa5050::pixels(ch).is_some(),
            "missing {code:02x}: {ch}"
        );
    }
    for (code, alias) in [(0x60, '–'), (0x7f, '█')] {
        let grid = decode(&page_with_row(0, &[0x0d, code]), &DecodeOptions::default());
        let rendered = present(&grid, &AnsiOptions::default());
        let expected = crate::saa5050::glyph(alias).unwrap();
        for row in 0..2 {
            assert_eq!(
                rendered[row][1].glyphs,
                expected[row].iter().collect::<String>()
            );
        }
    }
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
fn separated_blanks_are_spaces_in_every_presentation_mode() {
    // Separated graphics, a mosaic with an empty right half, then alpha text.
    let page = page_with_row(0, &[0x11, 0x1a, b' ', 0x21, 0x07, b'A', b' ']);
    let grid = decode(&page, &DecodeOptions::default());
    for wide in [false, true] {
        for separated in [
            SeparatedStyle::Braille,
            SeparatedStyle::Contiguous,
            SeparatedStyle::Unicode16,
        ] {
            let options = AnsiOptions { wide, separated };
            let rendered = present(&grid, &options);
            let blank = if wide { "  " } else { " " };
            assert_eq!(rendered[0][2].glyphs, blank);
            assert_eq!(rendered[0][6].glyphs, blank);
            if wide {
                assert!(rendered[0][3].glyphs.ends_with(' '));
            }
            assert!(!to_ansi(&grid, &options).contains('\u{2800}'));
        }
    }
}

#[test]
fn wide_double_height_mosaics_stretch_each_segment_vertically() {
    // Each original mosaic row occupies two rows across the pair of cells.
    let cases = [
        (0, 0, 0),
        (1, 5, 0),
        (2, 10, 0),
        (4, 16, 1),
        (8, 32, 2),
        (16, 0, 20),
        (32, 0, 40),
        (21, 21, 21),
        (42, 42, 42),
        (63, 63, 63),
    ];
    for separated in [false, true] {
        for style in [
            SeparatedStyle::Braille,
            SeparatedStyle::Contiguous,
            SeparatedStyle::Unicode16,
        ] {
            let options = AnsiOptions {
                wide: true,
                separated: style,
            };
            for (mask, top_mask, bottom_mask) in cases {
                let mode = if separated { 0x1a } else { 0x19 };
                let page = page_with_row(0, &[0x11, mode, 0x0d, crate::mosaic_code(mask)]);
                let rendered = present(&decode(&page, &DecodeOptions::default()), &options);
                let expected_page = page_with_row(
                    0,
                    &[
                        0x11,
                        mode,
                        crate::mosaic_code(top_mask),
                        crate::mosaic_code(bottom_mask),
                    ],
                );
                let expected =
                    present(&decode(&expected_page, &DecodeOptions::default()), &options);
                assert_eq!(
                    rendered[0][3].glyphs, expected[0][2].glyphs,
                    "top mask={mask}, separated={separated}, style={style:?}"
                );
                assert_eq!(
                    rendered[1][3].glyphs, expected[0][3].glyphs,
                    "bottom mask={mask}, separated={separated}, style={style:?}"
                );
            }
        }
    }
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
fn separated_graphics_preserve_blast_through_and_following_alpha_text() {
    let grid = decode(
        &page_with_row(0, &[0x11, 0x1a, b'A', 0x07, b'B']),
        &DecodeOptions::default(),
    );
    for separated in [
        SeparatedStyle::Braille,
        SeparatedStyle::Contiguous,
        SeparatedStyle::Unicode16,
    ] {
        for wide in [false, true] {
            let options = AnsiOptions { separated, wide };
            let rendered = present(&grid, &options);
            let (a, b) = if wide { ("Ａ", "Ｂ") } else { ("A", "B") };
            assert_eq!(rendered[0][2].glyphs, a);
            assert_eq!(rendered[0][4].glyphs, b);
            let ansi = to_ansi(&grid, &options);
            assert!(ansi.contains(a));
            assert!(ansi.contains(b));
        }
    }
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
fn tti_rejects_malformed_page_numbers_without_panicking() {
    for number in ["1€", "10€", "1�", "1é", "12", "", "1GG"] {
        assert!(matches!(
            Page::parse_tti(&format!("PN,{number}\n")),
            Err(crate::Error::InvalidTti(_))
        ));
    }
    for number in ["100", "1AB", "1ab"] {
        let pages = Page::parse_tti(&format!("PN,{number}\n")).unwrap();
        assert_eq!(
            pages[0].page_number(),
            Some(u16::from_str_radix(number, 16).unwrap())
        );
    }
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

#[test]
fn t42_subpages_preserve_all_digits_and_exclude_header_flags() {
    for subpage in [0x0000u16, 0x0001, 0x0070, 0x0800, 0x1000, 0x3f7f] {
        for flags in 0..8 {
            let mut packet = [b' '; 42];
            let nibbles = [
                1,
                0,
                0,
                0,
                (subpage & 15) as u8,
                ((subpage >> 4) & 7) as u8 | ((flags & 1) << 3),
                ((subpage >> 8) & 15) as u8,
                ((subpage >> 12) & 3) as u8 | ((flags >> 1) << 2),
                0,
                0,
            ];
            for (byte, nibble) in packet.iter_mut().zip(nibbles) {
                *byte = encode_hamming84(nibble);
            }
            let pages = Page::parse_t42(&packet).unwrap();
            assert_eq!(pages[0].subpage_number(), Some(subpage));
        }
    }
}

#[test]
fn repeated_tti_display_rows_replace_old_text_and_attributes() {
    for (replacement, expected) in [
        (&b"X"[..], &b"X"[..]),
        (&b""[..], &b""[..]),
        (&b"\x81R"[..], &b"\x01R"[..]),
    ] {
        let mut input = b"PN,10000\r\nOL,1,HELLO\x1bARED\r\nOL,2,UNCHANGED\r\nOL,1,".to_vec();
        input.extend(replacement);
        input.extend(b"\r\n");
        let service = Service::parse_tti_bytes(&input).unwrap();
        let row = &service.pages()[0].raw()[1];
        assert_eq!(&row[..expected.len()], expected);
        assert!(row[expected.len()..].iter().all(|&byte| byte == b' '));
        assert_eq!(&service.pages()[0].raw()[2][..9], b"UNCHANGED");
        assert_eq!(Service::parse_tti(&service.to_tti()).unwrap(), service);
    }
    // Enhancement rows can have multiple designation codes; retain them all.
    let service = Service::parse_tti("PN,10000\nOL,26,first\nOL,26,second\n").unwrap();
    assert_eq!(service.pages()[0].preserved_records().len(), 2);
    assert_eq!(Service::parse_tti(&service.to_tti()).unwrap(), service);
}

#[test]
fn tti_export_normalizes_parity_bits_from_mutable_raw_data() {
    let mut page = Page::default();
    page.set_identity(0x100, 0);
    for (cell, byte) in page.raw_mut().iter_mut().flatten().zip(0..=255u8) {
        *cell = byte;
    }
    page.raw_mut()[24].fill(b' ' | 0x80);
    let before = decode(&page, &DecodeOptions { reveal: true });
    let mut service = Service::default();
    service.insert(page.clone());
    let output = service.to_tti();
    assert!(output.is_ascii());
    assert!(!output.contains("OL,24,"));
    let restored = Service::parse_tti(&output).unwrap();
    for (original, normalized) in page
        .raw()
        .iter()
        .flatten()
        .zip(restored.pages()[0].raw().iter().flatten())
    {
        assert_eq!(original & 0x7f, *normalized);
    }
    assert_eq!(
        decode(&restored.pages()[0], &DecodeOptions { reveal: true }),
        before
    );
    assert_eq!(restored.to_tti(), output);
}

#[test]
fn tti_writes_five_digit_page_numbers_and_full_subcodes() {
    for (subpage, suffix) in [
        (0, "00"),
        (1, "01"),
        (0x10, "10"),
        (0x79, "79"),
        (0x99, "99"),
        (0x0a, "00"),
        (0x1279, "00"),
        (0x3f7f, "00"),
    ] {
        let mut page = Page::default();
        page.set_identity(0x2af, subpage);
        let mut service = Service::default();
        service.insert(page);
        let output = service.to_tti();
        assert!(output.starts_with(&format!("PN,2AF{suffix}\r\nSC,{subpage:04X}\r\n")));
        let pn = output.lines().next().unwrap().strip_prefix("PN,").unwrap();
        assert_eq!(pn.len(), 5);
        // Vbit2 LoadTTI extracts the page by dropping the final two nibbles.
        assert_eq!((u32::from_str_radix(pn, 16).unwrap() & 0xfff00) >> 8, 0x2af);
        assert_eq!(Service::parse_tti(&output).unwrap(), service);
    }
}

#[test]
fn tti_preserves_unsupported_output_rows_without_displaying_them() {
    let mut input = b"PN,10001\r\nOL,1,VISIBLE\r\n".to_vec();
    for row in 25..=28 {
        input.extend(format!("OL,{row},").bytes());
        input.extend(0x80..=0x9f);
        input.extend(b",PAYLOAD WITH TRAILING SPACES   \r\n");
    }
    let service = Service::parse_tti_bytes(&input).unwrap();
    let page = &service.pages()[0];
    assert_eq!(&page.raw()[1][..7], b"VISIBLE");
    assert_eq!(page.preserved_records().len(), 4);
    for (record, row) in page.preserved_records().iter().zip(25..=28) {
        assert_eq!(record.key, "OL");
        let mut expected = format!("{row},");
        for control in 0..32u8 {
            expected.push('\x1b');
            expected.push((control + 0x40) as char);
        }
        expected.push_str(",PAYLOAD WITH TRAILING SPACES   ");
        assert_eq!(record.value, expected);
    }
    assert_eq!(Service::parse_tti(&service.to_tti()).unwrap(), service);
    let extension_only = Service::parse_tti("OL,28,opaque  \r\n").unwrap();
    assert_eq!(extension_only.pages().len(), 1);
    assert_eq!(
        extension_only.pages()[0].preserved_records()[0].value,
        "28,opaque  "
    );
}

#[test]
fn tti_byte_parser_preserves_all_legacy_controls_and_utf8_metadata() {
    let mut input = "PN,10001\r\nDE,café\r\nOL,1,".as_bytes().to_vec();
    input.extend(0x80..=0x9f);
    input.extend(b",END\r\n");
    let service = Service::parse_tti_bytes(&input).unwrap();
    let page = &service.pages()[0];
    assert_eq!(&page.raw()[1][..32], &(0..32).collect::<Vec<u8>>());
    assert_eq!(&page.raw()[1][32..36], b",END");
    assert_eq!(page.preserved_records()[0].value, "café");
    assert_eq!(Page::parse_tti_bytes(&input).unwrap(), service.pages());
    assert_eq!(Service::parse_tti(&service.to_tti()).unwrap(), service);
}

#[test]
fn tti_leading_subcode_and_links_apply_only_to_the_first_page() {
    for start in ["PN,10002\nOL,1,FIRST", "OL,1,FIRST"] {
        let input = format!(
            "SC,0001\nFL,101,102,103,104,105,100\nFL,999\n{start}\nPN,20000\nOL,1,SECOND\n"
        );
        let service = Service::parse_tti(&input).unwrap();
        assert_eq!(service.pages().len(), 2);
        let first = &service.pages()[0];
        assert_eq!(first.subpage_number(), Some(1));
        assert_eq!(first.fasttext().unwrap().red, 0x101);
        assert_eq!(first.fasttext().unwrap().index, 0x100);
        assert_eq!(service.pages()[1].subpage_number(), Some(0));
        assert_eq!(service.pages()[1].fasttext(), None);
        let saved = service.to_tti();
        let restored = Service::parse_tti(&saved).unwrap();
        assert_eq!(restored.pages()[0].subpage_number(), Some(1));
        assert_eq!(restored.pages()[0].fasttext(), first.fasttext());
        assert_eq!(restored.to_tti(), saved);
    }
    let service = Service::parse_tti(
        "SC,0001\nFL,101,102,103,104,105,100\nPN,100\nSC,0003\nFL,201,202,203,204,205,200\n",
    )
    .unwrap();
    assert_eq!(service.pages()[0].subpage_number(), Some(3));
    assert_eq!(service.pages()[0].fasttext().unwrap().red, 0x201);
    assert_eq!(
        Service::parse_tti("SC,0001\nFL,101,102,103,104,105,100\n"),
        Err(crate::Error::NoPages)
    );
}

#[test]
fn tti_preserves_leading_records_on_the_first_page() {
    for first_page in ["PN,1000001\r\nOL,1,HELLO", "OL,1,HELLO"] {
        let input = format!(
            "DE,Page description\r\nXX,vendor,data\r\n{first_page}\r\nPN,2000001\r\nOL,1,WORLD\r\n"
        );
        let service = Service::parse_tti(&input).unwrap();
        assert_eq!(service.pages().len(), 2);
        let records = service.pages()[0].preserved_records();
        assert_eq!(records.len(), 2);
        assert_eq!(
            (records[0].key.as_str(), records[0].value.as_str()),
            ("DE", "Page description")
        );
        assert_eq!(
            (records[1].key.as_str(), records[1].value.as_str()),
            ("XX", "vendor,data")
        );
        assert!(service.pages()[1].preserved_records().is_empty());
        let serialized = service.to_tti();
        let reparsed = Service::parse_tti(&serialized).unwrap();
        assert_eq!(reparsed.pages()[0].preserved_records(), records);
        assert_eq!(reparsed.to_tti(), serialized);
        assert_eq!(Page::parse_tti(&input).unwrap(), service.pages());
    }
    assert_eq!(
        Service::parse_tti("DE,description\r\n"),
        Err(crate::Error::NoPages)
    );
}

#[test]
fn service_round_trips_subpages_fasttext_and_unknown_records() {
    let input = "PN,2000001\r\nSC,0001\r\nDE,kept\r\nFL,201,202,203,204,205,100\r\nOL,1,\x1bAHELLO\r\nPN,2000002\r\nSC,0002\r\nOL,1,WORLD\r\n";
    let service = Service::parse_tti(input).unwrap();
    assert_eq!(service.pages().len(), 2);
    assert_eq!(
        service.page(0x200, 1).unwrap().preserved_records()[0].key,
        "DE"
    );
    assert_eq!(
        service.page(0x200, 1).unwrap().fasttext(),
        Some(FastTextLinks {
            red: 0x201,
            green: 0x202,
            yellow: 0x203,
            cyan: 0x204,
            extra: 0x205,
            index: 0x100
        })
    );
    assert_eq!(Service::parse_tti(&service.to_tti()).unwrap(), service);
}

#[test]
fn presentation_is_exactly_eighty_columns_and_keeps_flash() {
    let grid = decode(&page_with_row(0, &[0x08, b'X']), &DecodeOptions::default());
    let rendered = present(&grid, &AnsiOptions::default());
    assert_eq!(
        rendered[0]
            .iter()
            .map(|cell| cell.width as usize)
            .sum::<usize>(),
        80
    );
    assert!(rendered[0][1].flash);
}

#[test]
fn visual_compiler_ignores_invisible_blank_attributes() {
    for mosaic in [false, true] {
        let mut row = [VisualCell::default(); 40];
        for (column, ch) in b"RED BLUE".iter().copied().enumerate() {
            if ch != b' ' {
                row[column] = VisualCell {
                    ch: if mosaic { 0x7f } else { ch },
                    fg: if column < 3 { 1 } else { 4 },
                    mosaic,
                    separated: mosaic,
                    ..VisualCell::default()
                };
            }
        }
        let plain = compile_visual_row(&row);
        row[3] = VisualCell {
            fg: 3,
            mosaic: !mosaic,
            separated: true,
            flash: true,
            conceal: true,
            ..VisualCell::default()
        };
        let blank = row[3];
        row[8..].fill(blank);
        let styled = compile_visual_row(&row);
        assert_eq!(plain, styled);
        let grid = decode(&page_with_row(0, &plain.bytes), &DecodeOptions::default());
        for column in 4..8 {
            assert!(
                !plain
                    .warnings
                    .iter()
                    .any(|warning| warning.column == column)
            );
            assert_eq!(grid.cell(0, column).unwrap().fg, 4);
            assert_ne!(grid.cell(0, column).unwrap().ch, ' ');
        }
        assert!(plain.bytes[8..].iter().all(|&byte| byte == b' '));
    }
}

#[test]
fn visual_compiler_replaces_control_glyphs_without_changing_following_state() {
    for code in (0x00..=0x1f).chain(0x80..=0x9f) {
        let row = [
            VisualCell {
                ch: code,
                ..VisualCell::default()
            },
            VisualCell {
                ch: b'X',
                ..VisualCell::default()
            },
        ];
        let compiled = compile_visual_row(&row);
        assert_eq!(compiled.warnings.len(), 1);
        assert_eq!(compiled.warnings[0].column, 0);
        assert!(compiled.warnings[0].message.contains("control code"));
        assert_eq!(&compiled.bytes[..2], b" X");
        let grid = decode(
            &page_with_row(0, &compiled.bytes),
            &DecodeOptions::default(),
        );
        assert_eq!(
            *grid.cell(0, 1).unwrap(),
            crate::Cell {
                ch: 'X',
                ..crate::Cell::default()
            }
        );
    }
    let row = [
        VisualCell {
            ch: 0x81,
            ..VisualCell::default()
        },
        VisualCell {
            ch: b'X',
            fg: 1,
            ..VisualCell::default()
        },
    ];
    let compiled = compile_visual_row(&row);
    assert_eq!(&compiled.bytes[..2], &[0x01, b'X']);
    assert_eq!(compiled.warnings.len(), 1);
    assert_eq!(compiled.warnings[0].column, 0);
    let printable = compile_visual_row(&[VisualCell {
        ch: b'X' | 0x80,
        ..VisualCell::default()
    }]);
    assert_eq!(printable.bytes[0], b'X');
    assert!(printable.warnings.is_empty());
}

#[test]
fn visual_compiler_keeps_set_at_background_in_the_requested_blank() {
    for (flash, double_height) in [(false, false), (true, false), (false, true)] {
        let mut row = [VisualCell::default(); 4];
        row[0].ch = b'A';
        row[2].bg = 7;
        row[2].flash = flash;
        row[2].double_height = double_height;
        row[3] = VisualCell { ch: b'B', ..row[2] };
        let compiled = compile_visual_row(&row);
        assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
        assert_eq!(compiled.bytes[2], 0x1d);
        let grid = decode(
            &page_with_row(0, &compiled.bytes),
            &DecodeOptions::default(),
        );
        assert_eq!(
            grid.rows()[0][..4]
                .iter()
                .map(|cell| cell.bg)
                .collect::<Vec<_>>(),
            [0, 0, 7, 7]
        );
        assert_eq!(grid.cell(0, 2).unwrap().flash, flash);
        assert_eq!(
            grid.cell(0, 2).unwrap().size,
            if double_height {
                CellSize::DoubleTop
            } else {
                CellSize::Normal
            }
        );
        assert_eq!(grid.cell(0, 3).unwrap().ch, 'B');
    }
}

#[test]
fn visual_compiler_keeps_size_and_colour_dependencies_before_blanks() {
    let mut row = [VisualCell::default(); 5];
    row[1].double_height = true;
    let compiled = compile_visual_row(&row);
    assert_eq!(compiled.bytes[0], 0x0d);
    let grid = decode(
        &page_with_row(0, &compiled.bytes),
        &DecodeOptions::default(),
    );
    assert_eq!(grid.cell(0, 1).unwrap().size, CellSize::DoubleTop);

    row = [VisualCell::default(); 5];
    row[3] = VisualCell {
        fg: 1,
        bg: 4,
        ..VisualCell::default()
    };
    row[4] = VisualCell { ch: b'X', ..row[3] };
    let compiled = compile_visual_row(&row);
    let grid = decode(
        &page_with_row(0, &compiled.bytes),
        &DecodeOptions::default(),
    );
    assert_eq!(
        (grid.cell(0, 3).unwrap().bg, grid.cell(0, 4).unwrap().fg),
        (4, 1)
    );
    assert_eq!(grid.cell(0, 4).unwrap().ch, 'X');
}

#[test]
fn visual_compiler_uses_partial_blank_runs_before_sacrificing_text() {
    let styled = VisualCell {
        ch: b'X',
        fg: 1,
        bg: 4,
        flash: true,
        ..VisualCell::default()
    };
    let mut row = [styled; 8];
    row[..2].fill(VisualCell::default());
    let compiled = compile_visual_row(&row);
    assert_eq!(
        &compiled.bytes[..8],
        &[0x04, 0x1d, 0x01, 0x08, b'X', b'X', b'X', b'X']
    );
    let consumed: Vec<_> = compiled
        .warnings
        .iter()
        .filter(|warning| row[warning.column].ch != b' ')
        .map(|warning| warning.column)
        .collect();
    assert_eq!(consumed, [2, 3]);
    assert!(compiled.warnings.iter().any(|warning| warning.column == 1));
    let grid = decode(
        &page_with_row(0, &compiled.bytes),
        &DecodeOptions::default(),
    );
    for column in 4..8 {
        let cell = grid.cell(0, column).unwrap();
        assert_eq!((cell.ch, cell.fg, cell.bg, cell.flash), ('X', 1, 4, true));
    }

    // The blank before A must not be reused: doing so would recolour A.
    row[..3].fill(VisualCell::default());
    row[1].ch = b'A';
    let compiled = compile_visual_row(&row);
    let grid = decode(
        &page_with_row(0, &compiled.bytes),
        &DecodeOptions::default(),
    );
    let a = grid.cell(0, 1).unwrap();
    assert_eq!((a.ch, a.fg, a.bg, a.flash), ('A', 7, 0, false));
    assert_eq!(&compiled.bytes[..4], &[b' ', b'A', 0x04, 0x1d]);
}

#[test]
fn visual_compiler_uses_a_blank_before_a_colour_transition() {
    let mut row = [VisualCell::default(); 40];
    row[1].fg = 1;
    row[1].ch = b'R';
    let compiled = compile_visual_row(&row);
    assert_eq!(&compiled.bytes[..2], &[0x01, b'R']);
    assert!(compiled.warnings.is_empty());
}

#[test]
fn visual_compiler_clears_conceal_without_changing_colour_or_mode() {
    for mosaic in [false, true] {
        let mut row = [VisualCell {
            mosaic,
            ..VisualCell::default()
        }; 10];
        row[3].ch = b'X';
        row[3].conceal = true;
        row[7].ch = b'Y';
        let compiled = compile_visual_row(&row);
        assert!(compiled.warnings.is_empty());
        let page = page_with_row(0, &compiled.bytes);
        let hidden = decode(&page, &DecodeOptions::default());
        let revealed = decode(&page, &DecodeOptions { reveal: true });
        assert_eq!(hidden.cell(0, 3).unwrap().ch, ' ');
        assert_eq!(revealed.cell(0, 3).unwrap().ch, 'X');
        let visible = hidden.cell(0, 7).unwrap();
        assert_eq!(visible.ch, 'Y');
        assert!(!visible.conceal);
        assert_eq!(visible.fg, 7);
    }
}

#[test]
fn visual_compiler_supports_independent_foreground_and_background() {
    for mosaic in [false, true] {
        for fg in 0..8 {
            for bg in 0..8 {
                let mut row = [VisualCell::default(); 8];
                row[7] = VisualCell {
                    ch: b'X',
                    fg,
                    bg,
                    mosaic,
                    conceal: true,
                    ..VisualCell::default()
                };
                let compiled = compile_visual_row(&row);
                assert!(!compiled.warnings.iter().any(|warning| warning.column == 7));
                let grid = decode(
                    &page_with_row(0, &compiled.bytes),
                    &DecodeOptions { reveal: true },
                );
                let cell = grid.cell(0, 7).unwrap();
                assert_eq!(
                    (cell.ch, cell.fg, cell.bg, cell.conceal),
                    ('X', fg, bg, true)
                );
            }
        }
    }
}

#[test]
fn visual_compiler_completes_background_transition_without_spare_blanks() {
    let row = [VisualCell {
        ch: b'X',
        fg: 1,
        bg: 4,
        ..VisualCell::default()
    }; 8];
    let compiled = compile_visual_row(&row);
    let grid = decode(
        &page_with_row(0, &compiled.bytes),
        &DecodeOptions::default(),
    );
    let cell = grid.cell(0, 3).unwrap();
    assert_eq!((cell.ch, cell.fg, cell.bg), ('X', 1, 4));
    assert_eq!(
        compiled
            .warnings
            .iter()
            .map(|warning| warning.column)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn mosaic_codes_round_trip_all_six_bit_masks() {
    for mask in 0..64 {
        assert_eq!(crate::mosaic_mask(crate::mosaic_code(mask)), Some(mask));
    }
}

#[test]
fn visual_compiler_output_decodes_to_requested_combined_style() {
    let mut row = [VisualCell::default(); 40];
    row[4] = VisualCell {
        ch: b'X',
        fg: 1,
        bg: 1,
        flash: true,
        ..VisualCell::default()
    };
    let compiled = compile_visual_row(&row);
    let page = page_with_row(0, &compiled.bytes);
    let grid = decode(&page, &DecodeOptions::default());
    let cell = grid.cell(0, 4).unwrap();
    assert_eq!((cell.ch, cell.fg, cell.bg, cell.flash), ('X', 1, 1, true));
}

#[test]
fn visual_compiler_reports_background_changes_to_reused_blanks() {
    let mut row = [VisualCell::default(); 3];
    row[2] = VisualCell {
        ch: b'X',
        bg: 7,
        ..VisualCell::default()
    };
    let compiled = compile_visual_row(&row);
    let grid = decode(
        &page_with_row(0, &compiled.bytes),
        &DecodeOptions::default(),
    );
    assert_eq!(grid.cell(0, 1).unwrap().bg, 7);
    assert_eq!(compiled.warnings.len(), 1);
    assert_eq!(compiled.warnings[0].column, 1);
    assert!(compiled.warnings[0].message.contains("background"));
    assert_eq!(
        (grid.cell(0, 2).unwrap().ch, grid.cell(0, 2).unwrap().bg),
        ('X', 7)
    );
}

#[test]
fn visual_compiler_reports_unfinished_background_transition_on_blank() {
    let row = [VisualCell {
        bg: 4,
        ..VisualCell::default()
    }; 4];
    let compiled = compile_visual_row(&row);
    let grid = decode(
        &page_with_row(0, &compiled.bytes),
        &DecodeOptions::default(),
    );
    for (column, target) in row.iter().enumerate() {
        let changed = grid.cell(0, column).unwrap().bg != target.bg;
        assert_eq!(
            compiled
                .warnings
                .iter()
                .any(|warning| warning.column == column),
            changed
        );
    }
    assert_eq!(compiled.warnings.len(), 1);
}

#[test]
fn t42_serial_headers_close_other_magazines_even_at_damaged_boundaries() {
    fn header(magazine: u8, serial: bool) -> [u8; 42] {
        let mut packet = [b' '; 42];
        for (byte, nibble) in
            packet
                .iter_mut()
                .zip([magazine, 0, 0, 0, 0, 0, 0, 0, 0, u8::from(serial)])
        {
            *byte = encode_hamming84(nibble);
        }
        packet
    }
    fn row(magazine: u8, ch: u8) -> [u8; 42] {
        let mut packet = [if ch.count_ones() % 2 == 1 {
            ch
        } else {
            ch | 0x80
        }; 42];
        packet[0] = encode_hamming84(magazine | 8);
        packet[1] = encode_hamming84(0);
        packet
    }
    for variant in 0..5 {
        let mut first = header(1, true);
        first[9] ^= 1; // C11 must survive a correctable error.
        let mut boundary = header(2, variant != 1);
        match variant {
            2 => boundary[2] ^= 3, // unreadable page identity
            3 => boundary[9] ^= 3, // unreadable C11: retain serial mode
            4 => {
                boundary[2] = encode_hamming84(15);
                boundary[3] = encode_hamming84(15);
            }
            _ => {}
        }
        let input = [
            first,
            row(1, b'A'),
            boundary,
            row(1, b'B'),
            header(1, true),
            row(1, b'C'),
        ]
        .concat();
        let pages = Page::parse_t42(&input).unwrap();
        assert_eq!(pages[0].raw()[1][0], b'A', "boundary variant {variant}");
        assert_eq!(pages.last().unwrap().raw()[1][0], b'C');
        assert_eq!(pages.len(), if matches!(variant, 2 | 4) { 2 } else { 3 });
    }
    // Entering serial mode also closes all outstanding parallel magazines.
    let input = [
        header(1, false),
        row(1, b'A'),
        header(2, false),
        row(2, b'B'),
        header(3, true),
        row(1, b'C'),
        row(2, b'D'),
    ]
    .concat();
    let pages = Page::parse_t42(&input).unwrap();
    assert_eq!(pages.len(), 3);
    assert_eq!(pages[0].raw()[1][0], b'A');
    assert_eq!(pages[1].raw()[1][0], b'B');
    assert_eq!(
        pages.iter().map(Page::page_number).collect::<Vec<_>>(),
        [Some(0x100), Some(0x200), Some(0x300)]
    );
}

#[test]
fn t42_filler_headers_end_transmissions_without_becoming_pages() {
    fn header(magazine: u8, number: u8, subpage: u16) -> [u8; 42] {
        let mut packet = [b' '; 42];
        let nibbles = [
            magazine & 7,
            0,
            number & 15,
            number >> 4,
            (subpage & 15) as u8,
            ((subpage >> 4) & 7) as u8,
            ((subpage >> 8) & 15) as u8,
            ((subpage >> 12) & 3) as u8,
            0,
            0,
        ];
        for (byte, nibble) in packet.iter_mut().zip(nibbles) {
            *byte = encode_hamming84(nibble);
        }
        packet
    }
    fn row(magazine: u8, ch: u8) -> [u8; 42] {
        let parity = if ch.count_ones() % 2 == 1 {
            ch
        } else {
            ch | 0x80
        };
        let mut packet = [parity; 42];
        packet[0] = encode_hamming84((magazine & 7) | 8);
        packet[1] = encode_hamming84(0);
        packet
    }
    for magazine in 1..=8 {
        for subpage in [0, 0x3f7e, 0x3f7f] {
            let mut filler = header(magazine, 0xff, subpage);
            filler[2] ^= 1; // Correctable damage must not admit a filler page.
            assert_eq!(Page::parse_t42(&filler), Err(crate::Error::NoPages));
            let input = [
                header(magazine, 0x10, 0),
                row(magazine, b'A'),
                filler,
                row(magazine, b'B'),
                header(magazine, 0x11, 0),
                row(magazine, b'C'),
            ]
            .concat();
            let pages = Page::parse_t42(&input).unwrap();
            assert_eq!(pages.len(), 2);
            assert_eq!(
                pages[0].page_number(),
                Some(u16::from(magazine) * 0x100 + 0x10)
            );
            assert_eq!(
                pages[1].page_number(),
                Some(u16::from(magazine) * 0x100 + 0x11)
            );
            assert_eq!(pages[0].raw()[1][0], b'A');
            assert_eq!(pages[1].raw()[1][0], b'C');
        }
    }
}

#[test]
fn t42_parsers_report_no_decodable_pages() {
    for input in [&[][..], &[0xff; 42][..], &[0x15; 41][..]] {
        assert_eq!(Page::parse_t42(input), Err(crate::Error::NoPages));
        assert_eq!(Service::parse_t42(input), Err(crate::Error::NoPages));
    }
    let mut header = [b' '; 42];
    for (byte, nibble) in header.iter_mut().zip([1, 0, 0, 0, 0, 0, 0, 0, 0, 0]) {
        *byte = encode_hamming84(nibble);
    }
    // A valid page is still returned when preceded by unrecoverable packets.
    let mut input = vec![0xff; 42];
    input.extend(header);
    let pages = Page::parse_t42(&input).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(Service::parse_t42(&input).unwrap().pages(), pages);
}

#[test]
fn t42_rejects_damaged_identities_without_misattributing_rows() {
    fn header() -> [u8; 42] {
        let mut packet = [b' '; 42];
        for (byte, nibble) in packet.iter_mut().zip([1, 0, 3, 0, 4, 3, 2, 1, 0, 0]) {
            *byte = encode_hamming84(nibble);
        }
        packet
    }
    fn row(magazine: u8, ch: u8) -> [u8; 42] {
        let parity = if ch.count_ones() % 2 == 1 {
            ch
        } else {
            ch | 0x80
        };
        let mut packet = [parity; 42];
        packet[0] = encode_hamming84(magazine | 8);
        packet[1] = encode_hamming84(0);
        packet
    }
    for field in 2..8 {
        for bit in 0..8 {
            let mut corrected = header();
            corrected[field] ^= 1 << bit;
            let pages = Page::parse_t42(&corrected).unwrap();
            assert_eq!(pages.len(), 1);
            assert_eq!(pages[0].page_number(), Some(0x103));
            assert_eq!(pages[0].subpage_number(), Some(0x1234));
            for other in bit + 1..8 {
                let mut damaged = corrected;
                damaged[field] ^= 1 << other;
                assert_eq!(Page::parse_t42(&damaged), Err(crate::Error::NoPages));
                let mut first = header();
                first[2] = encode_hamming84(1);
                let mut other_magazine = header();
                other_magazine[0] = encode_hamming84(2);
                let packets = [
                    first,
                    row(1, b'A'),
                    other_magazine,
                    damaged,
                    row(1, b'B'),
                    row(2, b'C'),
                    header(),
                    row(1, b'D'),
                ];
                let pages = Page::parse_t42(&packets.concat()).unwrap();
                assert_eq!(pages.len(), 3);
                assert_eq!(
                    pages.iter().map(Page::page_number).collect::<Vec<_>>(),
                    [Some(0x101), Some(0x203), Some(0x103)]
                );
                assert_eq!(pages[0].raw()[1][0], b'A');
                assert_eq!(pages[1].raw()[1][0], b'C');
                assert_eq!(pages[2].raw()[1][0], b'D');
            }
        }
    }
}

#[test]
fn visual_compiler_preserves_rainbow_backgrounds_and_footer_spacing() {
    for backgrounds in [vec![0, 0, 1, 1, 2, 2, 4, 4], vec![0, 1, 1, 1, 1, 1, 1]] {
        let mut row: Vec<_> = backgrounds
            .iter()
            .map(|&bg| VisualCell {
                bg,
                ..VisualCell::default()
            })
            .collect();
        if row.len() == 7 {
            for (cell, ch) in row[3..].iter_mut().zip(b"More") {
                cell.ch = *ch;
            }
        }
        let compiled = compile_visual_row(&row);
        assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
        let grid = decode(
            &page_with_row(0, &compiled.bytes),
            &DecodeOptions::default(),
        );
        for (column, target) in row.iter().enumerate() {
            let actual = grid.cell(0, column).unwrap();
            assert_eq!(actual.bg, target.bg, "column {column}");
            assert_eq!(actual.ch, char::from(target.ch), "column {column}");
            if target.ch != b' ' {
                assert_eq!(actual.fg, target.fg);
            }
        }
    }
}

#[test]
fn visual_compiler_reports_every_blank_height_mismatch() {
    for pattern in 0u16..256 {
        let row: Vec<_> = (0..8)
            .map(|column| VisualCell {
                double_height: pattern & (1 << column) != 0,
                ..VisualCell::default()
            })
            .collect();
        let compiled = compile_visual_row(&row);
        let grid = decode(
            &page_with_row(0, &compiled.bytes),
            &DecodeOptions::default(),
        );
        for (column, target) in row.iter().enumerate() {
            let actual = grid.cell(0, column).unwrap().size == CellSize::DoubleTop;
            let warned = compiled
                .warnings
                .iter()
                .any(|warning| warning.column == column && warning.message.contains("height"));
            assert_eq!(
                warned,
                actual != target.double_height,
                "pattern {pattern}, column {column}"
            );
        }
    }
}

#[test]
fn tti_dos_eof_marker_is_not_exported_as_a_record() {
    for ending in ["\x1a", "\x1a\r\n", "\x1a\r\nPN,20000\r\nOL,1,IGNORED\r\n"] {
        let input = format!("PN,10000\r\nOL,1,HELLO\r\n{ending}");
        let service = Service::parse_tti(&input).unwrap();
        let output = service.to_tti();
        assert!(!output.contains('\x1a'));
        assert!(!output.contains("IGNORED"));
        assert!(output.contains("OL,1,HELLO\r\n"));
        assert_eq!(Service::parse_tti(&output).unwrap().to_tti(), output);
    }
    // A raw control inside an OL payload is still teletext data.
    let service = Service::parse_tti("PN,10000\nOL,1,A\x1aB\n").unwrap();
    assert!(service.to_tti().contains("OL,1,A\x1bZB"));
}

#[test]
fn visual_compiler_ignores_separation_on_alpha_text() {
    let row = [VisualCell {
        ch: b'A',
        separated: true,
        ..VisualCell::default()
    }; 40];
    let compiled = compile_visual_row(&row);
    assert_eq!(compiled.bytes, [b'A'; 40]);
    assert!(compiled.warnings.is_empty());
    let mut row = [VisualCell::default(); 8];
    row[2] = VisualCell {
        ch: 0x7f,
        mosaic: true,
        separated: true,
        ..VisualCell::default()
    };
    row[4].ch = b'A';
    row[7] = VisualCell {
        separated: false,
        ..row[2]
    };
    let compiled = compile_visual_row(&row);
    let grid = decode(
        &page_with_row(0, &compiled.bytes),
        &DecodeOptions::default(),
    );
    assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
    assert!(grid.cell(0, 2).unwrap().separated);
    assert_eq!(grid.cell(0, 4).unwrap().ch, 'A');
    assert!(!grid.cell(0, 7).unwrap().separated);
}
