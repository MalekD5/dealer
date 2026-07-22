//! A reader for the subset of the tar format npm publishes.
//!
//! Supports USTAR headers plus the GNU long name and PAX extended header
//! records that `npm pack` can emit for deeply nested files.

use crate::error::{Result, error};

const BLOCK_LENGTH: usize = 512;

const NAME: std::ops::Range<usize> = 0..100;
const MODE: std::ops::Range<usize> = 100..108;
const SIZE: std::ops::Range<usize> = 124..136;
const CHECKSUM: std::ops::Range<usize> = 148..156;
const TYPE_FLAG: usize = 156;
const LINK_NAME: std::ops::Range<usize> = 157..257;
const PREFIX: std::ops::Range<usize> = 345..500;

/// What a tar entry represents on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    HardLink,
    /// Devices, FIFOs and anything else that has no place in a package.
    Unsupported(u8),
}

/// A single archive member, borrowing its contents from the archive buffer.
#[derive(Debug)]
pub struct Entry<'a> {
    pub path: String,
    pub kind: EntryKind,
    pub mode: u32,
    /// The target of a symlink or hard link entry.
    pub link_target: Option<String>,
    pub data: &'a [u8],
}

impl Entry<'_> {
    /// Whether the file carries any executable permission bit.
    pub fn is_executable(&self) -> bool {
        self.mode & 0o111 != 0
    }
}

/// Walks the members of an uncompressed tar archive held in memory.
pub struct ArchiveReader<'a> {
    data: &'a [u8],
    offset: usize,
    finished: bool,
    /// Overrides contributed by a preceding GNU long name or PAX header.
    pending_path: Option<String>,
    pending_link: Option<String>,
}

impl<'a> ArchiveReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            offset: 0,
            finished: false,
            pending_path: None,
            pending_link: None,
        }
    }

    /// Reads the next member, or `None` once the archive ends.
    pub fn next_entry(&mut self) -> Result<Option<Entry<'a>>> {
        loop {
            if self.finished {
                return Ok(None);
            }

            let header = self.take_block()?;
            if header.iter().all(|byte| *byte == 0) {
                self.finished = true;
                return Ok(None);
            }
            verify_checksum(header)?;

            let size = parse_number(&header[SIZE])
                .ok_or_else(|| error("tar entry has an unreadable size field"))?;
            let data = self.take_data(size)?;
            let type_flag = header[TYPE_FLAG];

            match type_flag {
                // GNU long name and long link name records describe the entry
                // that follows them.
                b'L' => {
                    self.pending_path = Some(read_string(data));
                    continue;
                }
                b'K' => {
                    self.pending_link = Some(read_string(data));
                    continue;
                }
                b'x' => {
                    self.read_pax_records(data)?;
                    continue;
                }
                // Global PAX headers carry defaults dealer has no use for.
                b'g' => continue,
                _ => {}
            }

            let path = self
                .pending_path
                .take()
                .unwrap_or_else(|| join_prefixed_name(&header[PREFIX], &header[NAME]));
            let link_target = self
                .pending_link
                .take()
                .or_else(|| Some(read_string(&header[LINK_NAME])).filter(|link| !link.is_empty()));

            return Ok(Some(Entry {
                path,
                kind: entry_kind(type_flag),
                mode: parse_number(&header[MODE]).unwrap_or(0o644) as u32,
                link_target,
                data,
            }));
        }
    }

    fn take_block(&mut self) -> Result<&'a [u8]> {
        let end = self.offset + BLOCK_LENGTH;
        let block = self
            .data
            .get(self.offset..end)
            .ok_or_else(|| error("tar archive ends in the middle of a header"))?;
        self.offset = end;
        Ok(block)
    }

    fn take_data(&mut self, size: u64) -> Result<&'a [u8]> {
        let size = usize::try_from(size)
            .map_err(|_| error("tar entry is larger than this platform can address"))?;
        let end = self
            .offset
            .checked_add(size)
            .ok_or_else(|| error("tar entry declares an implausible size"))?;
        let data = self
            .data
            .get(self.offset..end)
            .ok_or_else(|| error("tar archive ends in the middle of an entry"))?;

        // Entry contents are padded out to a whole number of blocks.
        self.offset = end.next_multiple_of(BLOCK_LENGTH);
        Ok(data)
    }

    /// PAX records are `"<length> <key>=<value>\n"` runs.
    fn read_pax_records(&mut self, data: &[u8]) -> Result<()> {
        let mut rest = data;

        while !rest.is_empty() {
            let space = rest
                .iter()
                .position(|byte| *byte == b' ')
                .ok_or_else(|| error("pax header record has no length separator"))?;
            let length: usize = std::str::from_utf8(&rest[..space])
                .ok()
                .and_then(|text| text.parse().ok())
                .ok_or_else(|| error("pax header record has an unreadable length"))?;

            let record = rest
                .get(space + 1..length)
                .ok_or_else(|| error("pax header record runs past the end of the header"))?;
            rest = &rest[length..];

            let record = String::from_utf8_lossy(record);
            let Some((key, value)) = record.trim_end_matches('\n').split_once('=') else {
                continue;
            };

            match key {
                "path" => self.pending_path = Some(value.to_string()),
                "linkpath" => self.pending_link = Some(value.to_string()),
                _ => {}
            }
        }

        Ok(())
    }
}

fn entry_kind(type_flag: u8) -> EntryKind {
    match type_flag {
        b'0' | b'7' | 0 => EntryKind::File,
        b'1' => EntryKind::HardLink,
        b'2' => EntryKind::Symlink,
        b'5' => EntryKind::Directory,
        other => EntryKind::Unsupported(other),
    }
}

/// USTAR splits long paths across a prefix field and the name field.
fn join_prefixed_name(prefix: &[u8], name: &[u8]) -> String {
    let prefix = read_string(prefix);
    let name = read_string(name);

    if prefix.is_empty() {
        name
    } else {
        format!("{prefix}/{name}")
    }
}

fn read_string(field: &[u8]) -> String {
    let end = field.iter().position(|byte| *byte == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

/// Header numbers are NUL or space terminated octal, or GNU base-256 for
/// values too large to fit.
fn parse_number(field: &[u8]) -> Option<u64> {
    if field.first().is_some_and(|byte| byte & 0x80 != 0) {
        return field[1..].iter().try_fold(u64::from(field[0] & 0x7f), |value, byte| {
            value.checked_mul(256)?.checked_add(u64::from(*byte))
        });
    }

    let digits: Vec<u8> = field
        .iter()
        .take_while(|byte| **byte != 0)
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if digits.is_empty() {
        return Some(0);
    }

    u64::from_str_radix(std::str::from_utf8(&digits).ok()?, 8).ok()
}

/// The stored checksum is computed with the checksum field itself blanked out.
fn verify_checksum(header: &[u8]) -> Result<()> {
    let stored = parse_number(&header[CHECKSUM])
        .ok_or_else(|| error("tar entry has an unreadable checksum field"))?;

    let mut unsigned: u64 = 0;
    let mut signed: i64 = 0;
    for (index, byte) in header.iter().enumerate() {
        let byte = if CHECKSUM.contains(&index) { b' ' } else { *byte };
        unsigned += u64::from(byte);
        signed += i64::from(byte as i8);
    }

    if stored == unsigned || stored as i64 == signed {
        return Ok(());
    }

    Err(error(
        "tar entry failed its header checksum; the archive is corrupt",
    ))
}
