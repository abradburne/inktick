//! Archives: every file in a zip or 7z, by name.
//!
//! Zip from PKWARE's APPNOTE.TXT (the central directory; stored and deflated entries). 7z from
//! 7-Zip's "7z Format description" (7zFormat.txt, version 18.06): the signature header, an
//! encoded or plain header, the streams' folders and sub-streams, the files' names and empty
//! streams; coders Copy, LZMA and LZMA2. Every file's CRC-32 is checked where the archive has it.

use std::collections::BTreeMap;

/// An archive's files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Archive {
    /// Contents by name (forward slashes, as stored).
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Archive {
    /// Reads a zip or 7z archive.
    pub fn read(data: &[u8]) -> Result<Archive, String> {
        if data.starts_with(b"PK\x03\x04") || data.starts_with(b"PK\x05\x06") {
            zip(data)
        } else if data.starts_with(&SEVEN_Z) {
            seven_z(data)
        } else {
            Err("not a zip or 7z archive".into())
        }
    }

    /// The file `name`, matched ignoring case.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_slice())
    }
}

fn u16le(d: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32le(d: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?) as usize)
}

fn zip(zip: &[u8]) -> Result<Archive, String> {
    // The end-of-central-directory record is in the last 64 KiB + 22 bytes.
    let search_from = zip.len().saturating_sub(0xffff + 22);
    let eocd = (search_from..zip.len().saturating_sub(21))
        .rev()
        .find(|&i| zip[i..].starts_with(b"PK\x05\x06"))
        .ok_or("not a zip archive")?;
    let bad = || "damaged zip".to_string();
    let entries = u16le(zip, eocd + 10).ok_or_else(bad)?;
    let mut at = u32le(zip, eocd + 16).ok_or_else(bad)?;
    let mut out = Archive::default();
    for _ in 0..entries {
        if !zip.get(at..).is_some_and(|d| d.starts_with(b"PK\x01\x02")) {
            return Err("damaged zip directory".into());
        }
        let method = u16le(zip, at + 10).ok_or_else(bad)?;
        let crc = u32le(zip, at + 16).ok_or_else(bad)? as u32;
        let packed = u32le(zip, at + 20).ok_or_else(bad)?;
        let size = u32le(zip, at + 24).ok_or_else(bad)?;
        let name_len = u16le(zip, at + 28).ok_or_else(bad)?;
        let extra_len = u16le(zip, at + 30).ok_or_else(bad)?;
        let comment_len = u16le(zip, at + 32).ok_or_else(bad)?;
        let local = u32le(zip, at + 42).ok_or_else(bad)?;
        let name = String::from_utf8_lossy(zip.get(at + 46..at + 46 + name_len).ok_or_else(bad)?)
            .into_owned();
        at += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            continue;
        }
        let lname = u16le(zip, local + 26).ok_or_else(bad)?;
        let lextra = u16le(zip, local + 28).ok_or_else(bad)?;
        let start = local + 30 + lname + lextra;
        let body = zip.get(start..start + packed).ok_or_else(bad)?;
        let data = match method {
            0 => body.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(body, 256 << 20)
                .map_err(|e| format!("{name}: {e:?}"))?,
            m => return Err(format!("{name}: compression method {m} is not supported")),
        };
        if data.len() != size {
            return Err(format!("{name}: {} bytes, the zip says {size}", data.len()));
        }
        if crc != 0 && crc32(&data) != crc {
            return Err(format!("{name}: CRC mismatch"));
        }
        out.files.insert(name, data);
    }
    Ok(out)
}

const SEVEN_Z: [u8; 6] = [b'7', b'z', 0xBC, 0xAF, 0x27, 0x1C];

mod id {
    pub const END: u8 = 0x00;
    pub const HEADER: u8 = 0x01;
    pub const ARCHIVE_PROPERTIES: u8 = 0x02;
    pub const ADDITIONAL_STREAMS: u8 = 0x03;
    pub const MAIN_STREAMS: u8 = 0x04;
    pub const FILES: u8 = 0x05;
    pub const PACK_INFO: u8 = 0x06;
    pub const UNPACK_INFO: u8 = 0x07;
    pub const SUBSTREAMS: u8 = 0x08;
    pub const SIZE: u8 = 0x09;
    pub const CRC: u8 = 0x0A;
    pub const FOLDER: u8 = 0x0B;
    pub const CODERS_UNPACK_SIZE: u8 = 0x0C;
    pub const NUM_UNPACK_STREAM: u8 = 0x0D;
    pub const EMPTY_STREAM: u8 = 0x0E;
    pub const NAME: u8 = 0x11;
    pub const ENCODED_HEADER: u8 = 0x17;
    pub const DUMMY: u8 = 0x19;
}

/// A cursor over header bytes, with 7z's number encoding.
struct Cur<'a> {
    d: &'a [u8],
    i: usize,
}

impl<'a> Cur<'a> {
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.d.get(self.i).ok_or("7z header ends early")?;
        self.i += 1;
        Ok(b)
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self
            .d
            .get(self.i..self.i + n)
            .ok_or("7z header ends early")?;
        self.i += n;
        Ok(s)
    }

    /// UINT64: the first byte's leading ones say how many bytes follow.
    fn num(&mut self) -> Result<u64, String> {
        let first = self.byte()?;
        let mut mask = 0x80u8;
        let mut value = 0u64;
        for i in 0..8 {
            if first & mask == 0 {
                let high = (first & mask.wrapping_sub(1)) as u64;
                return Ok(value | (high << (8 * i)));
            }
            value |= (self.byte()? as u64) << (8 * i);
            mask >>= 1;
        }
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().unwrap_or([0; 4]),
        ))
    }

    fn bits(&mut self, n: usize) -> Result<Vec<bool>, String> {
        let mut out = Vec::with_capacity(n);
        let mut b = 0;
        for i in 0..n {
            if i % 8 == 0 {
                b = self.byte()?;
            }
            out.push(b & (0x80 >> (i % 8)) != 0);
        }
        Ok(out)
    }

    fn digests(&mut self, n: usize) -> Result<Vec<Option<u32>>, String> {
        let all = self.byte()? != 0;
        let defined = if all { vec![true; n] } else { self.bits(n)? };
        defined
            .into_iter()
            .map(|d| if d { self.u32().map(Some) } else { Ok(None) })
            .collect()
    }
}

#[derive(Debug, Default, Clone)]
struct Coder {
    id: Vec<u8>,
    props: Vec<u8>,
}

#[derive(Debug, Default, Clone)]
struct Folder {
    coders: Vec<Coder>,
    unpack_sizes: Vec<u64>,
    crc: Option<u32>,
}

impl Folder {
    fn size(&self) -> u64 {
        // A single coder chain: the last output is the folder's.
        self.unpack_sizes.last().copied().unwrap_or(0)
    }
}

#[derive(Debug, Default)]
struct Streams {
    pack_pos: u64,
    pack_sizes: Vec<u64>,
    folders: Vec<Folder>,
    /// Per folder, the sub-streams' sizes and CRCs.
    subs: Vec<Vec<(u64, Option<u32>)>>,
}

fn streams(c: &mut Cur) -> Result<Streams, String> {
    let mut s = Streams::default();
    loop {
        match c.byte()? {
            id::END => break,
            id::PACK_INFO => {
                s.pack_pos = c.num()?;
                let n = c.num()? as usize;
                loop {
                    match c.byte()? {
                        id::END => break,
                        id::SIZE => {
                            s.pack_sizes = (0..n).map(|_| c.num()).collect::<Result<_, _>>()?
                        }
                        id::CRC => {
                            c.digests(n)?;
                        }
                        t => return Err(format!("7z: pack info property {t:#x}")),
                    }
                }
            }
            id::UNPACK_INFO => {
                if c.byte()? != id::FOLDER {
                    return Err("7z: no folders".into());
                }
                let n = c.num()? as usize;
                if c.byte()? != 0 {
                    return Err("7z: external folders are not supported".into());
                }
                for _ in 0..n {
                    s.folders.push(folder(c)?);
                }
                loop {
                    match c.byte()? {
                        id::END => break,
                        id::CODERS_UNPACK_SIZE => {
                            for f in &mut s.folders {
                                f.unpack_sizes = (0..f.coders.len())
                                    .map(|_| c.num())
                                    .collect::<Result<_, _>>()?;
                            }
                        }
                        id::CRC => {
                            let d = c.digests(s.folders.len())?;
                            for (f, crc) in s.folders.iter_mut().zip(d) {
                                f.crc = crc;
                            }
                        }
                        t => return Err(format!("7z: unpack info property {t:#x}")),
                    }
                }
            }
            id::SUBSTREAMS => {
                let mut counts = vec![1usize; s.folders.len()];
                let mut sizes: Vec<Vec<u64>> = Vec::new();
                let mut t = c.byte()?;
                if t == id::NUM_UNPACK_STREAM {
                    for k in &mut counts {
                        *k = c.num()? as usize;
                    }
                    t = c.byte()?;
                }
                if t == id::SIZE {
                    for (f, &k) in s.folders.iter().zip(&counts) {
                        let mut v = Vec::new();
                        let mut sum = 0;
                        for _ in 1..k {
                            let n = c.num()?;
                            sum += n;
                            v.push(n);
                        }
                        if k > 0 {
                            v.push(f.size().saturating_sub(sum));
                        }
                        sizes.push(v);
                    }
                    t = c.byte()?;
                } else {
                    sizes = s
                        .folders
                        .iter()
                        .zip(&counts)
                        .map(|(f, &k)| if k == 1 { vec![f.size()] } else { vec![] })
                        .collect();
                }
                // CRCs for the streams whose CRC isn't already the folder's.
                let unknown: usize = s
                    .folders
                    .iter()
                    .zip(&counts)
                    .map(|(f, &k)| if k == 1 && f.crc.is_some() { 0 } else { k })
                    .sum();
                let mut crcs = Vec::new();
                while t != id::END {
                    if t == id::CRC {
                        crcs = c.digests(unknown)?;
                    } else {
                        return Err(format!("7z: substreams property {t:#x}"));
                    }
                    t = c.byte()?;
                }
                let mut next = crcs.into_iter();
                s.subs = s
                    .folders
                    .iter()
                    .zip(sizes)
                    .zip(&counts)
                    .map(|((f, v), &k)| {
                        v.into_iter()
                            .map(|n| {
                                (
                                    n,
                                    if k == 1 && f.crc.is_some() {
                                        f.crc
                                    } else {
                                        next.next().flatten()
                                    },
                                )
                            })
                            .collect()
                    })
                    .collect();
            }
            t => return Err(format!("7z: streams property {t:#x}")),
        }
    }
    if s.subs.is_empty() {
        s.subs = s.folders.iter().map(|f| vec![(f.size(), f.crc)]).collect();
    }
    Ok(s)
}

fn folder(c: &mut Cur) -> Result<Folder, String> {
    let n = c.num()? as usize;
    let mut f = Folder::default();
    let (mut ins, mut outs) = (0u64, 0u64);
    for _ in 0..n {
        let flags = c.byte()?;
        let id = c.bytes((flags & 15) as usize)?.to_vec();
        let (i, o) = if flags & 0x10 != 0 {
            (c.num()?, c.num()?)
        } else {
            (1, 1)
        };
        ins += i;
        outs += o;
        let props = if flags & 0x20 != 0 {
            let len = c.num()? as usize;
            c.bytes(len)?.to_vec()
        } else {
            Vec::new()
        };
        f.coders.push(Coder { id, props });
    }
    for _ in 0..outs.saturating_sub(1) {
        c.num()?;
        c.num()?;
    }
    let packed = ins - outs.saturating_sub(1);
    if packed > 1 {
        for _ in 0..packed {
            c.num()?;
        }
    }
    if n != 1 {
        return Err("7z: only single-coder folders are supported".into());
    }
    Ok(f)
}

/// Unpacks folder `k` of `s` from the archive.
fn unpack(data: &[u8], s: &Streams, k: usize) -> Result<Vec<u8>, String> {
    let start = 32 + s.pack_pos + s.pack_sizes[..k].iter().sum::<u64>();
    let len = *s.pack_sizes.get(k).ok_or("7z: missing pack size")?;
    let packed = data
        .get(start as usize..(start + len) as usize)
        .ok_or("7z: data ends early")?;
    let f = &s.folders[k];
    let coder = &f.coders[0];
    let out = match coder.id.as_slice() {
        [0x00] => packed.to_vec(),
        [0x03, 0x01, 0x01] => {
            let mut input = coder.props.clone();
            input.extend_from_slice(packed);
            let mut out = Vec::new();
            let opts = lzma_rs::decompress::Options {
                unpacked_size: lzma_rs::decompress::UnpackedSize::UseProvided(Some(f.size())),
                ..Default::default()
            };
            lzma_rs::lzma_decompress_with_options(&mut &input[..], &mut out, &opts)
                .map_err(|e| format!("7z LZMA: {e}"))?;
            out
        }
        [0x21] => {
            let mut out = Vec::new();
            lzma_rs::lzma2_decompress(&mut &packed[..], &mut out)
                .map_err(|e| format!("7z LZMA2: {e}"))?;
            out
        }
        other => return Err(format!("7z: coder {other:02x?} is not supported")),
    };
    if out.len() as u64 != f.size() {
        return Err("7z: unpacked size differs".into());
    }
    Ok(out)
}

fn seven_z(data: &[u8]) -> Result<Archive, String> {
    let mut c = Cur { d: data, i: 12 };
    let next_offset = u64::from_le_bytes(c.bytes(8)?.try_into().unwrap_or([0; 8]));
    let next_size = u64::from_le_bytes(c.bytes(8)?.try_into().unwrap_or([0; 8]));
    let start = 32 + next_offset as usize;
    let mut header = data
        .get(start..start + next_size as usize)
        .ok_or("7z: header out of range")?
        .to_vec();
    if header.first() == Some(&id::ENCODED_HEADER) {
        let mut hc = Cur { d: &header, i: 1 };
        let s = streams(&mut hc)?;
        header = unpack(data, &s, 0)?;
    }
    let mut c = Cur { d: &header, i: 0 };
    if c.byte()? != id::HEADER {
        return Err("7z: no header".into());
    }
    let mut main = Streams::default();
    let mut names: Vec<String> = Vec::new();
    let mut empty: Vec<bool> = Vec::new();
    loop {
        match c.byte()? {
            id::END => break,
            id::ARCHIVE_PROPERTIES => loop {
                if c.byte()? == 0 {
                    break;
                }
                let n = c.num()? as usize;
                c.bytes(n)?;
            },
            id::ADDITIONAL_STREAMS => {
                streams(&mut c)?;
            }
            id::MAIN_STREAMS => main = streams(&mut c)?,
            id::FILES => {
                let n = c.num()? as usize;
                empty = vec![false; n];
                loop {
                    let t = c.byte()?;
                    if t == id::END {
                        break;
                    }
                    let size = c.num()? as usize;
                    let body = c.bytes(size)?;
                    let mut b = Cur { d: body, i: 0 };
                    match t {
                        id::EMPTY_STREAM => empty = b.bits(n)?,
                        id::NAME => {
                            if b.byte()? != 0 {
                                return Err("7z: external names are not supported".into());
                            }
                            let units: Vec<u16> = body[1..]
                                .chunks_exact(2)
                                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                                .collect();
                            names = units
                                .split(|&u| u == 0)
                                .take(n)
                                .map(String::from_utf16_lossy)
                                .collect();
                        }
                        id::DUMMY => {}
                        _ => {}
                    }
                }
            }
            t => return Err(format!("7z: header property {t:#x}")),
        }
    }
    let mut out = Archive::default();
    let mut streams_iter = Vec::new();
    for (k, subs) in main.subs.iter().enumerate() {
        let data = unpack(data, &main, k)?;
        let mut at = 0usize;
        for &(n, crc) in subs {
            let piece = data
                .get(at..at + n as usize)
                .ok_or("7z: sub-stream out of range")?
                .to_vec();
            at += n as usize;
            if let Some(crc) = crc {
                if crc32(&piece) != crc {
                    return Err("7z: CRC mismatch".into());
                }
            }
            streams_iter.push(piece);
        }
    }
    let mut streams_iter = streams_iter.into_iter();
    for (i, name) in names.into_iter().enumerate() {
        if empty.get(i).copied().unwrap_or(false) {
            // A directory or an empty file.
            continue;
        }
        let body = streams_iter.next().ok_or("7z: fewer streams than files")?;
        out.files.insert(name.replace('\\', "/"), body);
    }
    Ok(out)
}

/// CRC-32 (the IEEE 802.3 polynomial, reflected), as zip and 7z use.
pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        t
    });
    let mut crc = !0u32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_of_the_check_string() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn seven_z_numbers() {
        let mut c = Cur {
            d: &[0x7F, 0x80, 0x80, 0xC1, 0x02, 0x03],
            i: 0,
        };
        assert_eq!(c.num().unwrap(), 0x7F);
        assert_eq!(c.num().unwrap(), 0x80);
        assert_eq!(c.num().unwrap(), 0x01_0302);
    }
}
