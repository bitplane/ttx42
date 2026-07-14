use std::{env, fs};

use ttx42::{AnsiOptions, DecodeOptions, Page, decode, to_ansi};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| "corpus/BBC1-1985-12-28-goodenough.t42".into());
    let bytes = fs::read(path)?;
    let pages = Page::parse_t42(&bytes)?;
    let (page, grid, count) = pages
        .iter()
        .map(|page| {
            let grid = decode(page, &DecodeOptions::default());
            let count = grid
                .rows()
                .iter()
                .flatten()
                .filter(|cell| cell.separated && cell.ch != ' ')
                .count();
            (page, grid, count)
        })
        .max_by_key(|(_, _, count)| *count)
        .ok_or("no pages")?;
    eprintln!(
        "most separated mosaics: page {:03X}/{:04X}, {count} cells, {} pages scanned",
        page.page_number().unwrap_or(0),
        page.subpage_number().unwrap_or(0),
        pages.len()
    );
    print!(
        "{}",
        to_ansi(
            &grid,
            &AnsiOptions {
                wide: true,
                ..AnsiOptions::default()
            }
        )
    );
    Ok(())
}
