use std::collections::{BTreeMap, HashMap};

use crate::Error;

pub const ROWS: usize = 25;
pub const COLS: usize = 40;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page {
    pub(crate) bytes: [[u8; COLS]; ROWS],
    number: Option<u16>,
    subpage: Option<u16>,
    fasttext: Option<FastTextLinks>,
    records: Vec<TtiRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtiRecord {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FastTextLinks {
    pub red: u16,
    pub green: u16,
    pub yellow: u16,
    pub cyan: u16,
    pub extra: u16,
    pub index: u16,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
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

    pub fn parse_tti(text: &str) -> Result<Vec<Self>, Error> {
        parse_tti(text)
    }
    pub fn parse_t42(bytes: &[u8]) -> Result<Vec<Self>, Error> {
        Ok(parse_t42(bytes))
    }
    pub fn page_number(&self) -> Option<u16> {
        self.number
    }
    pub fn subpage_number(&self) -> Option<u16> {
        self.subpage
    }
    pub fn raw(&self) -> &[[u8; COLS]; ROWS] {
        &self.bytes
    }
    pub fn raw_mut(&mut self) -> &mut [[u8; COLS]; ROWS] {
        &mut self.bytes
    }
    pub fn set_identity(&mut self, number: u16, subpage: u16) {
        self.number = Some(number);
        self.subpage = Some(subpage);
    }
    pub fn fasttext(&self) -> Option<FastTextLinks> {
        self.fasttext
    }
    pub fn set_fasttext(&mut self, links: Option<FastTextLinks>) {
        self.fasttext = links;
    }
    pub fn preserved_records(&self) -> &[TtiRecord] {
        &self.records
    }
}

impl Service {
    pub fn parse_tti(text: &str) -> Result<Self, Error> {
        Ok(Self {
            pages: parse_tti(text)?,
        })
    }
    pub fn parse_t42(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self {
            pages: parse_t42(bytes),
        })
    }
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }
    pub fn pages_mut(&mut self) -> &mut Vec<Page> {
        &mut self.pages
    }
    pub fn insert(&mut self, page: Page) {
        self.pages.push(page);
        self.pages
            .sort_by_key(|page| (page.number.unwrap_or(0), page.subpage.unwrap_or(0)));
    }
    pub fn remove(&mut self, number: u16, subpage: u16) -> Option<Page> {
        let index = self
            .pages
            .iter()
            .position(|page| page.number == Some(number) && page.subpage.unwrap_or(0) == subpage)?;
        Some(self.pages.remove(index))
    }
    pub fn page(&self, number: u16, subpage: u16) -> Option<&Page> {
        self.pages
            .iter()
            .find(|page| page.number == Some(number) && page.subpage.unwrap_or(0) == subpage)
    }
    pub fn subpages(&self, number: u16) -> impl Iterator<Item = &Page> {
        self.pages
            .iter()
            .filter(move |page| page.number == Some(number))
    }
    pub fn page_numbers(&self) -> impl Iterator<Item = u16> + '_ {
        let mut numbers = BTreeMap::new();
        for page in &self.pages {
            if let Some(number) = page.number {
                numbers.insert(number, ());
            }
        }
        numbers.into_keys()
    }
    pub fn to_tti(&self) -> String {
        let mut out = String::new();
        for page in &self.pages {
            let number = page.number.unwrap_or(0x100);
            let subpage = page.subpage.unwrap_or(0);
            out.push_str(&format!(
                "PN,{number:03X}{subpage:04X}\r\nSC,{subpage:04X}\r\n"
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
                    .rposition(|byte| *byte != b' ')
                    .map_or(0, |index| index + 1);
                if end == 0 {
                    continue;
                }
                out.push_str(&format!("OL,{row},"));
                for &byte in &bytes[..end] {
                    if byte < 0x20 {
                        out.push('\x1b');
                        out.push((byte + 0x40) as char);
                    } else {
                        out.push(byte as char);
                    }
                }
                out.push_str("\r\n");
            }
        }
        out
    }
}

fn parse_tti(text: &str) -> Result<Vec<Page>, Error> {
    let mut pages = Vec::new();
    let mut current: Option<Page> = None;
    let mut leading_records = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        let (key, value) = line.split_once(',').unwrap_or((line, ""));
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
                current = Some(page);
            }
            "SC" => {
                if let Some(page) = current.as_mut() {
                    page.subpage = u16::from_str_radix(value.trim(), 16).ok();
                }
            }
            "FL" => {
                if let Some(page) = current.as_mut() {
                    let values: Vec<_> = value.split(',').map(str::trim).collect();
                    if values.len() >= 6 {
                        let parse = |value: &str| {
                            u16::from_str_radix(
                                value.trim_start_matches(|c: char| !c.is_ascii_hexdigit()),
                                16,
                            )
                            .unwrap_or(0)
                        };
                        page.fasttext = Some(FastTextLinks {
                            red: parse(values[0]),
                            green: parse(values[1]),
                            yellow: parse(values[2]),
                            cyan: parse(values[3]),
                            extra: parse(values[4]),
                            index: parse(values[5]),
                        });
                    }
                }
            }
            "OL" => {
                let (row, data) = value
                    .split_once(',')
                    .ok_or_else(|| Error::InvalidTti(format!("OL without row/data: {line}")))?;
                let row: usize = row
                    .parse()
                    .map_err(|_| Error::InvalidTti(format!("bad OL row: {row}")))?;
                if row >= ROWS {
                    continue;
                }
                let page = current.get_or_insert_with(|| Page {
                    records: std::mem::take(&mut leading_records),
                    ..Page::default()
                });
                let decoded = decode_tti_line(data.as_bytes());
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
            if let Some(page) = active.remove(&magazine) {
                completed.push(page);
            }
            // A new header ends the previous page even if its identity is
            // unreadable. Leave this magazine inactive until a valid header
            // arrives so subsequent rows cannot contaminate another page.
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
