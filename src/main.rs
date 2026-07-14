use std::{
    env, fs,
    io::{self, Read},
    process::ExitCode,
};

use ttx42::{AnsiOptions, DecodeOptions, Page, SeparatedStyle, decode, to_ansi};

#[derive(Clone, Copy)]
enum Format {
    Raw,
    Tti,
    T42,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ttx42: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut file = None;
    let mut format = None;
    let mut page_number = None;
    let mut subpage = None;
    let mut reveal = false;
    let mut separated = SeparatedStyle::Braille;
    let mut wide = true;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                format = Some(parse_format(&args.next().ok_or("--format needs a value")?)?)
            }
            "--page" => page_number = Some(parse_hex(&args.next().ok_or("--page needs a value")?)?),
            "--subpage" => {
                subpage = Some(parse_hex(&args.next().ok_or("--subpage needs a value")?)?)
            }
            "--reveal" => reveal = true,
            "--wide" => wide = true,
            "--narrow" => wide = false,
            "--separated" => {
                separated = match args.next().as_deref() {
                    Some("braille") => SeparatedStyle::Braille,
                    Some("contiguous") => SeparatedStyle::Contiguous,
                    Some("unicode16") => SeparatedStyle::Unicode16,
                    _ => return Err("--separated expects braille, contiguous, or unicode16".into()),
                }
            }
            "-h" | "--help" => {
                usage();
                return Ok(());
            }
            value if value.starts_with('-') && value != "-" => {
                return Err(format!("unknown option: {value}").into());
            }
            value if file.is_none() => file = Some(value.to_owned()),
            value => return Err(format!("unexpected argument: {value}").into()),
        }
    }
    let bytes = match file.as_deref() {
        None | Some("-") => {
            let mut bytes = Vec::new();
            io::stdin().read_to_end(&mut bytes)?;
            bytes
        }
        Some(path) => fs::read(path)?,
    };
    let format = format.unwrap_or_else(|| sniff(&bytes));
    let pages = match format {
        Format::Raw => vec![Page::from_raw(&bytes)?],
        Format::Tti => Page::parse_tti(&String::from_utf8_lossy(&bytes))?,
        Format::T42 => Page::parse_t42(&bytes)?,
    };
    if matches!(format, Format::T42) && pages.len() > 1 && page_number.is_none() {
        for page in &pages {
            println!(
                "{:03X} {:04X}",
                page.page_number().unwrap_or(0),
                page.subpage_number().unwrap_or(0)
            );
        }
        return Ok(());
    }
    let (_, grid) = pages
        .iter()
        .filter(|candidate| {
            page_number.is_none_or(|number| candidate.page_number() == Some(number))
                && subpage.is_none_or(|number| candidate.subpage_number() == Some(number))
        })
        .map(|page| {
            let grid = decode(page, &DecodeOptions { reveal });
            let visible = grid
                .rows()
                .iter()
                .flatten()
                .filter(|cell| cell.ch != ' ')
                .count();
            (visible, grid)
        })
        .max_by_key(|(visible, _)| *visible)
        .ok_or("requested page not found")?;
    print!("{}", to_ansi(&grid, &AnsiOptions { separated, wide }));
    Ok(())
}

fn parse_format(value: &str) -> Result<Format, Box<dyn std::error::Error>> {
    match value {
        "raw" => Ok(Format::Raw),
        "tti" => Ok(Format::Tti),
        "t42" => Ok(Format::T42),
        _ => Err(format!("unknown format: {value}").into()),
    }
}

fn parse_hex(value: &str) -> Result<u16, Box<dyn std::error::Error>> {
    Ok(u16::from_str_radix(value.trim_start_matches("0x"), 16)?)
}

fn sniff(bytes: &[u8]) -> Format {
    if bytes.starts_with(b"PN,") || bytes.windows(4).any(|window| window == b"\nOL,") {
        Format::Tti
    } else if bytes.len().is_multiple_of(42) && !bytes.is_empty() {
        Format::T42
    } else {
        Format::Raw
    }
}

fn usage() {
    println!(
        "ttx42 [FILE|-] [--format raw|tti|t42] [--page HEX] [--subpage HEX] [--reveal] [--wide|--narrow] [--separated braille|contiguous|unicode16]"
    );
}
