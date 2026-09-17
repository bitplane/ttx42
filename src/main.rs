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
            "-V" | "--version" => {
                println!("ttx42 {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
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
        Some(path) => fs::read(path)
            .map_err(|error| io::Error::new(error.kind(), format!("{path:?}: {error}")))?,
    };
    let format = format.unwrap_or_else(|| sniff(&bytes));
    let pages = match format {
        Format::Raw => vec![Page::from_raw(&bytes)?],
        Format::Tti => Page::parse_tti_bytes(&bytes)?,
        Format::T42 => Page::parse_t42(&bytes)?,
    };
    if matches!(format, Format::T42)
        && pages.len() > 1
        && page_number.is_none()
        && subpage.is_none()
    {
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
                && subpage.is_none_or(|number| candidate.subpage_number().unwrap_or(0) == number)
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
    // Recognize a bounded prefix of TTI records, not PN-like bytes anywhere
    // in a binary capture. Unknown two-letter leading records are allowed.
    let prefix = &bytes[..bytes.len().min(4096)];
    let tti = prefix
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty() && *line != b"\r")
        .take_while(|line| {
            line.len() >= 3 && line[..2].iter().all(u8::is_ascii_uppercase) && line[2] == b','
        })
        .any(|line| line.starts_with(b"PN,") || line.starts_with(b"OL,"));
    if tti {
        Format::Tti
    } else if matches!(bytes.len(), 960 | 1000) || bytes.len() < 42 {
        Format::Raw
    } else {
        // T42 recovery accepts a trailing partial packet. Requiring an exact
        // multiple here would prevent that recovery before the parser runs.
        Format::T42
    }
}

fn usage() {
    println!(
        "ttx42 [--version] [FILE|-] [--format raw|tti|t42] [--page HEX] [--subpage HEX] [--reveal] [--wide|--narrow] [--separated braille|contiguous|unicode16]\n\
         Reads stdin when FILE is omitted or '-'.\n\
         Raw input has no page number; omit --page for raw files.\n\
         Multi-transmission T42 input lists page/subpage numbers unless either selector is given.\n\
         When multiple entries match, renders the one with most visible non-space cells;\n\
         ties select the last match in input order. Transmissions are not merged."
    );
}
