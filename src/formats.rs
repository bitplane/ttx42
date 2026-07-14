use std::collections::HashMap;

use crate::Error;

pub const ROWS: usize = 25;
pub const COLS: usize = 40;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page {
    pub(crate) bytes: [[u8; COLS]; ROWS],
    number: Option<u16>,
    subpage: Option<u16>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            bytes: [[b' '; COLS]; ROWS],
            number: None,
            subpage: None,
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
}

fn parse_tti(text: &str) -> Result<Vec<Page>, Error> {
    let mut pages = Vec::new();
    let mut current: Option<Page> = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        let (key, value) = line.split_once(',').unwrap_or((line, ""));
        match key {
            "PN" => {
                if let Some(page) = current.take() {
                    pages.push(page);
                }
                let mut page = Page::default();
                let token = value.split(',').next().unwrap_or(value).trim();
                let token = token.trim_start_matches(|c: char| !c.is_ascii_hexdigit());
                if token.len() >= 3 {
                    page.number = u16::from_str_radix(&token[..3], 16).ok();
                    page.subpage = token
                        .get(3..)
                        .filter(|value| !value.is_empty())
                        .and_then(|value| u16::from_str_radix(value, 16).ok());
                }
                current = Some(page);
            }
            "SC" => {
                if let Some(page) = current.as_mut() {
                    page.subpage = u16::from_str_radix(value.trim(), 16).ok();
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
                let page = current.get_or_insert_with(Page::default);
                let decoded = decode_tti_line(data.as_bytes());
                for (column, byte) in decoded.into_iter().take(COLS).enumerate() {
                    page.bytes[row][column] = byte;
                }
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
            let mut page = Page::default();
            let units = hamming84(packet[2]).unwrap_or(0) as u16;
            let tens = hamming84(packet[3]).unwrap_or(0) as u16;
            page.number = Some(
                ((if magazine == 0 { 8 } else { magazine }) as u16) * 0x100 + tens * 0x10 + units,
            );
            let s1 = hamming84(packet[4]).unwrap_or(0) as u16;
            let s2 = hamming84(packet[5]).unwrap_or(0) as u16;
            let s3 = hamming84(packet[6]).unwrap_or(0) as u16;
            let s4 = hamming84(packet[7]).unwrap_or(0) as u16;
            page.subpage = Some(s1 | (s2 << 4) | ((s3 & 7) << 8) | ((s4 & 3) << 11));
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
