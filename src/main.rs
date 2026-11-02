use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs,
    io::{self, Read, Write},
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
    let mut output = io::stdout().lock();
    let result = run(&mut output).and_then(|()| output.flush().map_err(Into::into));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe) =>
        {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ttx42: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(output: &mut impl Write) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = None;
    let mut format = None;
    let mut page_number = None;
    let mut subpage = None;
    let mut reveal = false;
    let mut separated = SeparatedStyle::Braille;
    let mut wide = true;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str().unwrap_or("") {
            "--format" => format = Some(parse_format(&option_value(&mut args, "--format")?)?),
            "--page" => {
                page_number = Some(parse_hex("--page", &option_value(&mut args, "--page")?)?)
            }
            "--subpage" => {
                subpage = Some(parse_hex(
                    "--subpage",
                    &option_value(&mut args, "--subpage")?,
                )?)
            }
            "--reveal" => reveal = true,
            "--wide" => wide = true,
            "--narrow" => wide = false,
            "--separated" => {
                separated = match option_value(&mut args, "--separated")?.as_str() {
                    "braille" => SeparatedStyle::Braille,
                    "contiguous" => SeparatedStyle::Contiguous,
                    "unicode16" => SeparatedStyle::Unicode16,
                    _ => return Err("--separated expects braille, contiguous, or unicode16".into()),
                }
            }
            "-V" | "--version" => {
                writeln!(output, "ttx42 {}", env!("CARGO_PKG_VERSION"))?;
                return Ok(());
            }
            "-h" | "--help" => {
                usage(output)?;
                return Ok(());
            }
            _ if arg.as_encoded_bytes().starts_with(b"-") && arg != "-" => {
                return Err(format!("unknown option: {}", arg.to_string_lossy()).into());
            }
            _ if file.is_none() => file = Some(arg),
            _ => return Err(format!("unexpected argument: {}", arg.to_string_lossy()).into()),
        }
    }
    let bytes = match file.as_deref().filter(|path| *path != "-") {
        None => {
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
        let mut counts = BTreeMap::new();
        for page in &pages {
            let identity = (
                page.page_number().unwrap_or(0),
                page.subpage_number().unwrap_or(0),
            );
            *counts.entry(identity).or_insert(0usize) += 1;
        }
        for ((page, subpage), count) in counts {
            writeln!(output, "{page:03X} {subpage:04X} {count}")?;
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
                .filter(|cell| cell.ch != ' ' && cell.fg != cell.bg)
                .count();
            (visible, grid)
        })
        .max_by_key(|(visible, _)| *visible)
        .ok_or("requested page not found")?;
    output.write_all(to_ansi(&grid, &AnsiOptions { separated, wide }).as_bytes())?;
    Ok(())
}

fn option_value(
    args: &mut impl Iterator<Item = OsString>,
    flag: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    args.next()
        .ok_or_else(|| format!("{flag} needs a value"))?
        .into_string()
        .map_err(|_| format!("{flag}: value must be valid UTF-8").into())
}

fn parse_format(value: &str) -> Result<Format, Box<dyn std::error::Error>> {
    match value {
        "raw" => Ok(Format::Raw),
        "tti" => Ok(Format::Tti),
        "t42" => Ok(Format::T42),
        _ => Err(format!("unknown format: {value}").into()),
    }
}

fn parse_hex(flag: &str, value: &str) -> Result<u16, Box<dyn std::error::Error>> {
    u16::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|error| format!("{flag}: invalid hexadecimal value {value:?}: {error}").into())
}

fn sniff(bytes: &[u8]) -> Format {
    // Raw display bytes can contain arbitrary text, including TTI record
    // prefixes and newlines. Resolve ambiguous exact lengths as raw; callers
    // can explicitly select TTI for text files of these lengths.
    if matches!(bytes.len(), 960 | 1000) {
        return Format::Raw;
    }
    // Recognize a bounded prefix of TTI records, not PN-like bytes anywhere
    // in a binary capture. Allow text comments without commas, as the TTI
    // parser does, but stop at binary data or malformed record prefixes.
    let prefix = &bytes[..bytes.len().min(4096)];
    let tti = prefix
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty() && *line != b"\r")
        .take_while(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            if !line.contains(&b',') {
                return std::str::from_utf8(line)
                    .is_ok_and(|text| text.chars().all(|ch| !ch.is_control() || ch == '\t'));
            }
            line.len() >= 3 && line[..2].iter().all(u8::is_ascii_uppercase) && line[2] == b','
        })
        .any(|line| line.starts_with(b"PN,") || line.starts_with(b"OL,"));
    if tti {
        Format::Tti
    } else if bytes.len() < 42 {
        Format::Raw
    } else {
        // T42 recovery accepts a trailing partial packet. Requiring an exact
        // multiple here would prevent that recovery before the parser runs.
        Format::T42
    }
}

fn usage(output: &mut impl Write) -> io::Result<()> {
    writeln!(
        output,
        "ttx42 [--version] [FILE|-] [--format raw|tti|t42] [--page HEX] [--subpage HEX] [--reveal] [--wide|--narrow] [--separated braille|contiguous|unicode16]\n\
         Reads stdin when FILE is omitted or '-'.\n\
         Raw input has no page number; omit --page for raw files.\n\
         Multi-transmission T42 input lists page/subpage numbers and transmission counts unless either selector is given.\n\
         When multiple entries match, renders the one with most visible non-space cells;\n\
         ties select the last match in input order. Transmissions are not merged."
    )
}
