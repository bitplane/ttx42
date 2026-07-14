use ttx42::{AnsiOptions, DecodeOptions, Page, decode, to_ansi};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let wide = !std::env::args().any(|arg| arg == "--narrow");
    let mut raw = [b' '; 1000];
    put(&mut raw, 1, 8, &[0x03, 0x0d, b'T', b'T', b'X', b'4', b'2']);
    put(&mut raw, 4, 8, &[0x11, 0x1a, 0x7f, 0x61, 0x7f, 0x61]);
    put(&mut raw, 5, 8, &[0x12, 0x1a, 0x61, 0x7f, 0x61, 0x7f]);
    put(
        &mut raw,
        7,
        8,
        &[0x07, b'L', b'E', b'V', b'E', b'L', b' ', b'1'],
    );
    let grid = decode(&Page::from_raw(&raw)?, &DecodeOptions::default());
    print!(
        "{}",
        to_ansi(
            &grid,
            &AnsiOptions {
                wide,
                ..AnsiOptions::default()
            }
        )
    );
    Ok(())
}

fn put(raw: &mut [u8; 1000], row: usize, column: usize, bytes: &[u8]) {
    let start = row * 40 + column;
    raw[start..start + bytes.len()].copy_from_slice(bytes);
}
