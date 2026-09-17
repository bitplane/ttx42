use std::collections::{BTreeSet, HashMap};

use crate::Error;

pub const ROWS: usize = 25;
pub const COLS: usize = 40;

#[derive(Clone, Debug, Eq, PartialEq)]
/// A 25×40 grid of display bytes with optional page identity and TTI metadata.
/// The default page is blank and has no identity or links.
pub struct Page {
    pub(crate) bytes: [[u8; COLS]; ROWS],
    number: Option<u16>,
    subpage: Option<u16>,
    fasttext: Option<FastTextLinks>,
    records: Vec<TtiRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// A retained TTI record, including unknown commands and unsupported OL rows.
pub struct TtiRecord {
    /// Command name, such as `DE`, `DS`, or `OL`.
    pub key: String,
    /// Retained value without the line ending. Metadata values follow the
    /// command's first comma. OL values contain the row number, a comma, and
    /// the normalized seven-bit payload with controls ESC-escaped.
    pub value: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// Six TTI Fasttext destination page numbers, such as `0x100`; zero means
/// no link. All links default to zero. T42 packet links are not yet recovered.
pub struct FastTextLinks {
    /// Red-key destination.
    pub red: u16,
    /// Green-key destination.
    pub green: u16,
    /// Yellow-key destination.
    pub yellow: u16,
    /// Cyan-key destination.
    pub cyan: u16,
    /// Fifth destination in an FL record.
    pub extra: u16,
    /// Index-key destination.
    pub index: u16,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
/// An ordered collection of pages or recovered transmissions. Parsing keeps
/// input order and duplicates; [`Service::insert`] sorts by identity. Defaults
/// to an empty collection. Repeated T42 transmissions are not merged.
pub struct Service {
    pages: Vec<Page>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            bytes: [[b' '; COLS]; ROWS],
            number: None,
            subpage: None,
            fasttext: None,
            records: Vec::new(),
        }
    }
}

impl Page {
    /// Read row-major display bytes, stripping the high parity bit.
    /// A 1000-byte buffer supplies rows 0 to 24; 960 bytes supply rows 1 to 24 and
    /// leave the header blank. This does not infer a page identity.
    ///
    /// # Errors
    /// Returns [`Error::InvalidRawLength`] for any other buffer length.
    pub fn from_raw(bytes: &[u8]) -> Result<Self, Error> {
        let start_row = match bytes.len() {
            960 => 1,
            1000 => 0,
            len => return Err(Error::InvalidRawLength(len)),
        };
        let mut page = Self::default();
        for (index, byte) in bytes.iter().enumerate() {
            page.bytes[start_row + index / COLS][index % COLS] = byte & 0x7f;
        }
        Ok(page)
    }

    /// Parse TTI text containing ASCII display codes and ESC-escaped controls.
    /// Returns pages in input order. Use [`Self::parse_tti_bytes`] for legacy
    /// files with raw high-bit controls. Errors match that method.
    pub fn parse_tti(text: &str) -> Result<Vec<Self>, Error> {
        Self::parse_tti_bytes(text.as_bytes())
    }
    /// Parse TTI files, preserving legacy high-bit controls in output lines.
    /// Supports LF/CRLF lines and ESC escapes. Non-display metadata is decoded
    /// as UTF-8 with replacement for invalid sequences. Repeated display rows
    /// replace earlier rows; unsupported OL rows are retained as records.
    /// Lines without a comma are ignored. Display rows are limited to 40
    /// decoded bytes. A trailing lone ESC is retained as control 0x1b and
    /// escaped on export.
    /// Leading SC and FL records apply to the first page. An explicit SC
    /// takes precedence over the subpage suffix in that page's PN record.
    ///
    /// # Errors
    /// Returns [`Error::InvalidTti`] for malformed PN or OL records, or
    /// [`Error::NoPages`] if no page is found. Malformed SC values become an
    /// unspecified subpage. FL records with fewer than six fields are ignored,
    /// retaining any earlier links. In records with at least six fields,
    /// malformed destinations become zero.
    pub fn parse_tti_bytes(bytes: &[u8]) -> Result<Vec<Self>, Error> {
        parse_tti(bytes)
    }
    /// Recover pages from T42 packets, or return `Error::NoPages` if none decode.
    /// Corrects single-bit Hamming errors, blanks bad-parity display bytes,
    /// and ignores incomplete trailing packets. Handles serial/parallel page
    /// boundaries and filler headers. Output follows header arrival order and
    /// retains repeated transmissions. Packet 27 Fasttext links are not decoded.
    pub fn parse_t42(bytes: &[u8]) -> Result<Vec<Self>, Error> {
        let pages = parse_t42(bytes);
        if pages.is_empty() {
            Err(Error::NoPages)
        } else {
            Ok(pages)
        }
    }
    /// Page address, such as `Some(0x100)`, or `None` when unspecified.
    pub fn page_number(&self) -> Option<u16> {
        self.number
    }
    /// Full subcode, such as `Some(0x0010)`, or `None` when unspecified.
    /// Service lookup methods treat `None` as zero.
    pub fn subpage_number(&self) -> Option<u16> {
        self.subpage
    }
    /// Borrow all 25 rows of 40 display bytes, including header row zero.
    pub fn raw(&self) -> &[[u8; COLS]; ROWS] {
        &self.bytes
    }
    /// Edit display bytes. Decoding and TTI export ignore the high parity bit.
    pub fn raw_mut(&mut self) -> &mut [[u8; COLS]; ROWS] {
        &mut self.bytes
    }
    /// Set page number and full subcode without validation. Use page numbers
    /// `0x100..=0x8fe` excluding `xFF`, and subcodes fitting mask `0x3f7f`.
    pub fn set_identity(&mut self, number: u16, subpage: u16) {
        self.number = Some(number);
        self.subpage = Some(subpage);
    }
    /// Return the TTI Fasttext links, if present.
    pub fn fasttext(&self) -> Option<FastTextLinks> {
        self.fasttext
    }
    /// Replace Fasttext links, or remove them with `None`.
    pub fn set_fasttext(&mut self, links: Option<FastTextLinks>) {
        self.fasttext = links;
    }
    /// Borrow retained metadata and unsupported output-row records in order.
    pub fn preserved_records(&self) -> &[TtiRecord] {
        &self.records
    }
}

impl Service {
    /// Parse an ordered service from TTI text; see [`Page::parse_tti`] for
    /// supported encodings and errors.
    pub fn parse_tti(text: &str) -> Result<Self, Error> {
        Self::parse_tti_bytes(text.as_bytes())
    }
    /// Parse TTI files, preserving legacy high-bit controls in output lines.
    /// See [`Page::parse_tti_bytes`] for format handling and errors.
    pub fn parse_tti_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self {
            pages: parse_tti(bytes)?,
        })
    }
    /// Recover a service, or return `Error::NoPages` if no T42 pages decode.
    /// See [`Page::parse_t42`] for recovery behavior and limitations.
    pub fn parse_t42(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self {
            pages: Page::parse_t42(bytes)?,
        })
    }
    /// Borrow all pages in their current order, including duplicates.
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }
    /// Edit pages directly. Mutations do not automatically sort or deduplicate.
    pub fn pages_mut(&mut self) -> &mut Vec<Page> {
        &mut self.pages
    }
    /// Append a page, then stably sort by page number and subcode, treating
    /// unspecified identities as zero. Existing duplicates are retained.
    pub fn insert(&mut self, page: Page) {
        self.pages.push(page);
        self.pages
            .sort_by_key(|page| (page.number.unwrap_or(0), page.subpage.unwrap_or(0)));
    }
    /// Remove and return the first matching page; unspecified subcodes match zero.
    pub fn remove(&mut self, number: u16, subpage: u16) -> Option<Page> {
        let index = self
            .pages
            .iter()
            .position(|page| page.number == Some(number) && page.subpage.unwrap_or(0) == subpage)?;
        Some(self.pages.remove(index))
    }
    /// Find the first matching page; unspecified subcodes match zero.
    pub fn page(&self, number: u16, subpage: u16) -> Option<&Page> {
        self.pages
            .iter()
            .find(|page| page.number == Some(number) && page.subpage.unwrap_or(0) == subpage)
    }
    /// Iterate over all entries for a page number, preserving their current order.
    pub fn subpages(&self, number: u16) -> impl Iterator<Item = &Page> {
        self.pages
            .iter()
            .filter(move |page| page.number == Some(number))
    }
    /// Iterate over distinct known page numbers in ascending order.
    pub fn page_numbers(&self) -> impl Iterator<Item = u16> + '_ {
        let mut numbers = BTreeSet::new();
        for page in &self.pages {
            if let Some(number) = page.number {
                numbers.insert(number);
            }
        }
        numbers.into_iter()
    }
    /// Write canonical TTI. PN uses the five-digit `mppss` form; SC carries
    /// the full subcode. Subcodes outside two BCD digits use `00` in PN.
    ///
    /// Output uses CRLF, ESC-escaped controls and seven-bit display bytes.
    /// Trailing display-row spaces and completely blank display rows are
    /// omitted. Missing page identity becomes `0x100`, subcode zero.
    /// Leading file records, including DE, DS and SP, belong to the first
    /// parsed page and are emitted after that page's PN and SC records.
    /// Unsupported OL records retain their full normalized payloads. Output
    /// preserves supported page content and metadata, not original formatting.
    pub fn to_tti(&self) -> String {
        let mut out = String::new();
        for page in &self.pages {
            let number = page.number.unwrap_or(0x100);
            let subpage = page.subpage.unwrap_or(0);
            // MRG PN has two decimal subpage digits. Vbit2 extracts the page
            // by shifting past exactly those two digits; SC is authoritative.
            let suffix = if subpage <= 0x99 && subpage & 0x0f <= 9 {
                subpage
            } else {
                0
            };
            out.push_str(&format!(
                "PN,{number:03X}{suffix:02X}\r\nSC,{subpage:04X}\r\n"
            ));
            for record in &page.records {
                out.push_str(&record.key);
                out.push(',');
                out.push_str(&record.value);
                out.push_str("\r\n");
            }
            if let Some(links) = page.fasttext {
                out.push_str(&format!(
                    "FL,{:03X},{:03X},{:03X},{:03X},{:03X},{:03X}\r\n",
                    links.red, links.green, links.yellow, links.cyan, links.extra, links.index
                ));
            }
            for (row, bytes) in page.bytes.iter().enumerate() {
                let end = bytes
                    .iter()
                    .rposition(|byte| byte & 0x7f != b' ')
                    .map_or(0, |index| index + 1);
                if end == 0 {
                    continue;
                }
                out.push_str(&format!("OL,{row},"));
                out.push_str(&encode_tti_line(&bytes[..end]));
                out.push_str("\r\n");
            }
        }
        out
    }
}

fn parse_tti(bytes: &[u8]) -> Result<Vec<Page>, Error> {
    let mut pages = Vec::new();
    let mut current: Option<Page> = None;
    let mut leading_records = Vec::new();
    let mut leading_subpage = None;
    let mut leading_links = None;
    for line_bytes in bytes.split(|&byte| byte == b'\n') {
        let line_bytes = line_bytes.strip_suffix(b"\r").unwrap_or(line_bytes);
        // DOS text files may end with a standalone Ctrl-Z marker. It is not
        // a record and must never be relocated ahead of the output rows.
        if line_bytes.starts_with(b"\x1a") {
            break;
        }
        let line = String::from_utf8_lossy(line_bytes);
        let line = line.as_ref();
        let Some((key, value)) = line.split_once(',') else {
            continue;
        };
        match key {
            "PN" => {
                if let Some(page) = current.take() {
                    pages.push(page);
                }
                let mut page = Page {
                    records: std::mem::take(&mut leading_records),
                    ..Page::default()
                };
                let token = value.split(',').next().unwrap_or(value).trim();
                let token = token.trim_start_matches(|c: char| !c.is_ascii_hexdigit());
                let number = token
                    .get(..3)
                    .and_then(|number| u16::from_str_radix(number, 16).ok())
                    .ok_or_else(|| Error::InvalidTti(format!("bad PN page number: {value}")))?;
                page.number = Some(number);
                page.subpage = token
                    .get(3..)
                    .filter(|value| !value.is_empty())
                    .and_then(|value| u16::from_str_radix(value, 16).ok());
                page.subpage = leading_subpage.take().unwrap_or(page.subpage);
                page.fasttext = leading_links.take();
                current = Some(page);
            }
            "SC" => {
                let subpage = u16::from_str_radix(value.trim(), 16).ok();
                if let Some(page) = current.as_mut() {
                    page.subpage = subpage;
                } else {
                    leading_subpage = Some(subpage);
                }
            }
            "FL" => {
                let values: Vec<_> = value.split(',').map(str::trim).collect();
                if values.len() >= 6 {
                    let parse = |value: &str| {
                        u16::from_str_radix(
                            value.trim_start_matches(|c: char| !c.is_ascii_hexdigit()),
                            16,
                        )
                        .unwrap_or(0)
                    };
                    let links = FastTextLinks {
                        red: parse(values[0]),
                        green: parse(values[1]),
                        yellow: parse(values[2]),
                        cyan: parse(values[3]),
                        extra: parse(values[4]),
                        index: parse(values[5]),
                    };
                    if let Some(page) = current.as_mut() {
                        page.fasttext = Some(links);
                    } else {
                        leading_links = Some(links);
                    }
                }
            }
            "OL" => {
                let (row, _) = value
                    .split_once(',')
                    .ok_or_else(|| Error::InvalidTti(format!("OL without row/data: {line}")))?;
                let row: usize = row
                    .parse()
                    .map_err(|_| Error::InvalidTti(format!("bad OL row: {row}")))?;
                let page = current.get_or_insert_with(|| Page {
                    subpage: leading_subpage.take().flatten(),
                    fasttext: leading_links.take(),
                    records: std::mem::take(&mut leading_records),
                    ..Page::default()
                });
                // Decode the original bytes: legacy controls are not UTF-8.
                let data = line_bytes.splitn(3, |&byte| byte == b',').nth(2).unwrap();
                let decoded = decode_tti_line(data);
                if row >= ROWS {
                    // Keep unsupported packets as canonical records, including
                    // their complete payload and trailing spaces.
                    page.records.push(TtiRecord {
                        key: "OL".into(),
                        value: format!("{row},{}", encode_tti_line(&decoded)),
                    });
                    continue;
                }
                // A repeated display-row record replaces the entire row,
                // including padding omitted by shorter or empty records.
                page.bytes[row].fill(b' ');
                for (column, byte) in decoded.into_iter().take(COLS).enumerate() {
                    page.bytes[row][column] = byte;
                }
            }
            _ if !key.is_empty() => {
                let records = current
                    .as_mut()
                    .map_or(&mut leading_records, |page| &mut page.records);
                records.push(TtiRecord {
                    key: key.to_string(),
                    value: value.to_string(),
                });
            }
            _ => {}
        }
    }
    if let Some(page) = current {
        pages.push(page);
    }
    if pages.is_empty() {
        Err(Error::NoPages)
    } else {
        Ok(pages)
    }
}

fn encode_tti_line(data: &[u8]) -> String {
    let mut output = String::new();
    for &byte in data {
        let byte = byte & 0x7f;
        if byte < 0x20 {
            output.push('\x1b');
            output.push((byte + 0x40) as char);
        } else {
            output.push(byte as char);
        }
    }
    output
}

fn decode_tti_line(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < data.len() {
        if data[index] == 0x1b && index + 1 < data.len() {
            output.push(data[index + 1].wrapping_sub(0x40) & 0x7f);
            index += 2;
        } else {
            output.push(data[index] & 0x7f);
            index += 1;
        }
    }
    output
}

fn parse_t42(bytes: &[u8]) -> Vec<Page> {
    let mut active: HashMap<u8, (usize, Page)> = HashMap::new();
    let mut completed = Vec::new();
    let mut sequence = 0;
    let mut serial_mode = false;
    for packet in bytes.chunks_exact(42) {
        let Some(a) = hamming84(packet[0]) else {
            continue;
        };
        let Some(b) = hamming84(packet[1]) else {
            continue;
        };
        let address = a | (b << 4);
        let magazine = address & 7;
        let row = (address >> 3) as usize;
        if row > 24 {
            continue;
        }
        if row == 0 {
            // C11 is bit zero of the final Hamming-coded header nibble.
            // Retain the last known mode if this control nibble is damaged.
            let next_serial_mode = hamming84(packet[9])
                .map(|flags| flags & 1 != 0)
                .unwrap_or(serial_mode);
            if serial_mode || next_serial_mode {
                completed.extend(active.drain().map(|(_, page)| page));
            } else if let Some(page) = active.remove(&magazine) {
                completed.push(page);
            }
            serial_mode = next_serial_mode;
            // A new header ends the previous page even if its identity is
            // unreadable. Leave the affected magazines inactive until a valid
            // header arrives so subsequent rows cannot contaminate a page.
            let identity: [Option<u16>; 6] =
                std::array::from_fn(|index| hamming84(packet[index + 2]).map(u16::from));
            let [
                Some(units),
                Some(tens),
                Some(s1),
                Some(s2),
                Some(s3),
                Some(s4),
            ] = identity
            else {
                continue;
            };
            // FF headers terminate the previous transmission but are time
            // fillers, not pages carrying data (ETSI 300 706, annex A.1).
            if units == 0x0f && tens == 0x0f {
                continue;
            }
            let mut page = Page {
                number: Some(
                    ((if magazine == 0 { 8 } else { magazine }) as u16) * 0x100
                        + tens * 0x10
                        + units,
                ),
                subpage: Some(s1 | ((s2 & 7) << 4) | (s3 << 8) | ((s4 & 3) << 12)),
                ..Page::default()
            };
            for column in 8..40 {
                page.bytes[0][column] = parity_data(packet[column + 2]);
            }
            active.insert(magazine, (sequence, page));
            sequence += 1;
        } else if let Some((_, page)) = active.get_mut(&magazine) {
            for column in 0..40 {
                page.bytes[row][column] = parity_data(packet[column + 2]);
            }
        }
    }
    completed.extend(active.into_values());
    completed.sort_by_key(|(sequence, _)| *sequence);
    completed.into_iter().map(|(_, page)| page).collect()
}

pub(crate) fn parity_data(byte: u8) -> u8 {
    if byte.count_ones() % 2 == 1 {
        byte & 0x7f
    } else {
        b' '
    }
}

/// Decode SECDED Hamming 8/4 by choosing the unique codeword within one bit.
pub(crate) fn hamming84(byte: u8) -> Option<u8> {
    (0..16).find(|&nibble| (byte ^ encode_hamming84(nibble)).count_ones() <= 1)
}

pub(crate) fn encode_hamming84(n: u8) -> u8 {
    const CODEWORDS: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    CODEWORDS[n as usize & 0x0f]
}
