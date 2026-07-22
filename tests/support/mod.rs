//! Helpers for building the package tarballs the tests install from.

#![allow(dead_code)]

pub mod provider;
pub mod registry;

use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;

const BLOCK_LENGTH: usize = 512;

pub const REGULAR_FILE: u8 = b'0';
pub const HARD_LINK: u8 = b'1';
pub const SYMLINK: u8 = b'2';
pub const CHARACTER_DEVICE: u8 = b'3';
pub const DIRECTORY: u8 = b'5';
pub const GNU_LONG_NAME: u8 = b'L';
pub const PAX_HEADER: u8 = b'x';

/// Builds tar archives, including deliberately malformed ones.
#[derive(Default)]
pub struct TarballBuilder {
    blocks: Vec<u8>,
}

impl TarballBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a regular file rooted at the conventional `package/` directory.
    pub fn file(mut self, path: &str, contents: &str) -> Self {
        self.push(&format!("package/{path}"), contents.as_bytes(), REGULAR_FILE, 0o644, "");
        self
    }

    pub fn executable(mut self, path: &str, contents: &str) -> Self {
        self.push(&format!("package/{path}"), contents.as_bytes(), REGULAR_FILE, 0o755, "");
        self
    }

    pub fn directory(mut self, path: &str) -> Self {
        self.push(&format!("package/{path}"), &[], DIRECTORY, 0o755, "");
        self
    }

    pub fn symlink(mut self, path: &str, target: &str) -> Self {
        self.push(&format!("package/{path}"), &[], SYMLINK, 0o777, target);
        self
    }

    pub fn hard_link(mut self, path: &str, target: &str) -> Self {
        self.push(&format!("package/{path}"), &[], HARD_LINK, 0o644, target);
        self
    }

    /// Adds an entry with the path written exactly as given, bypassing the
    /// `package/` convention.
    pub fn raw(mut self, path: &str, contents: &str, type_flag: u8) -> Self {
        self.push(path, contents.as_bytes(), type_flag, 0o644, "");
        self
    }

    pub fn raw_link(mut self, path: &str, target: &str, type_flag: u8) -> Self {
        self.push(path, &[], type_flag, 0o777, target);
        self
    }

    /// Writes a GNU long name record ahead of an entry whose header name is
    /// truncated.
    pub fn long_named_file(mut self, path: &str, contents: &str) -> Self {
        let path = format!("package/{path}");
        let mut name = path.clone().into_bytes();
        name.push(0);

        self.push("././@LongLink", &name, GNU_LONG_NAME, 0o644, "");
        self.push(&path, contents.as_bytes(), REGULAR_FILE, 0o644, "");
        self
    }

    /// Writes a PAX extended header that overrides the following entry's path.
    pub fn pax_named_file(mut self, path: &str, contents: &str) -> Self {
        let path = format!("package/{path}");
        let record = pax_record("path", &path);

        self.push("PaxHeaders/entry", record.as_bytes(), PAX_HEADER, 0o644, "");
        self.push("package/placeholder", contents.as_bytes(), REGULAR_FILE, 0o644, "");
        self
    }

    /// Serialises the archive and compresses it the way `npm pack` does.
    pub fn build(self) -> Vec<u8> {
        gzip(&self.into_tar())
    }

    /// Serialises the archive without compressing it.
    pub fn into_tar(mut self) -> Vec<u8> {
        // Archives end with two zero filled blocks.
        self.blocks.extend(std::iter::repeat_n(0, BLOCK_LENGTH * 2));
        self.blocks
    }

    fn push(&mut self, path: &str, contents: &[u8], type_flag: u8, mode: u32, link: &str) {
        self.blocks
            .extend_from_slice(&header(path, contents.len(), type_flag, mode, link));
        self.blocks.extend_from_slice(contents);

        let padding = contents.len().next_multiple_of(BLOCK_LENGTH) - contents.len();
        self.blocks.extend(std::iter::repeat_n(0, padding));
    }
}

/// Compresses bytes as a gzip stream.
pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data).expect("gzip encoding should succeed");
    encoder.finish().expect("gzip stream should finish")
}

/// A minimal npm style `package.json` for a fixture package.
pub fn manifest(name: &str, version: &str) -> String {
    manifest_with(name, version, serde_json::json!({}))
}

/// A fixture `package.json` with extra fields such as `dependencies`, `bin` or
/// `scripts` merged in.
pub fn manifest_with(name: &str, version: &str, extra: serde_json::Value) -> String {
    let mut document = serde_json::json!({ "name": name, "version": version });

    if let (Some(target), Some(extra)) = (document.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            target.insert(key.clone(), value.clone());
        }
    }

    document.to_string()
}

fn header(path: &str, size: usize, type_flag: u8, mode: u32, link: &str) -> [u8; BLOCK_LENGTH] {
    let mut block = [0u8; BLOCK_LENGTH];

    write_field(&mut block, 0, 100, path.as_bytes());
    write_octal(&mut block, 100, 8, u64::from(mode));
    write_octal(&mut block, 108, 8, 0);
    write_octal(&mut block, 116, 8, 0);
    write_octal(&mut block, 124, 12, size as u64);
    write_octal(&mut block, 136, 12, 0);
    block[156] = type_flag;
    write_field(&mut block, 157, 100, link.as_bytes());
    write_field(&mut block, 257, 6, b"ustar\0");
    write_field(&mut block, 263, 2, b"00");

    // The checksum is computed with its own field blanked out.
    block[148..156].fill(b' ');
    let checksum: u64 = block.iter().map(|byte| u64::from(*byte)).sum();
    let encoded = format!("{checksum:06o}\0 ");
    block[148..156].copy_from_slice(encoded.as_bytes());

    block
}

/// Corrupts the checksum of the first header in a tar stream.
pub fn corrupt_checksum(mut tar: Vec<u8>) -> Vec<u8> {
    tar[148..156].copy_from_slice(b"000000\0 ");
    tar
}

fn write_field(block: &mut [u8], offset: usize, length: usize, value: &[u8]) {
    let written = value.len().min(length);
    block[offset..offset + written].copy_from_slice(&value[..written]);
}

fn write_octal(block: &mut [u8], offset: usize, length: usize, value: u64) {
    let encoded = format!("{value:0width$o}\0", width = length - 1);
    write_field(block, offset, length, encoded.as_bytes());
}

fn pax_record(key: &str, value: &str) -> String {
    let payload = format!(" {key}={value}\n");

    // The record length is written into the record, so it has to account for
    // its own digits.
    let mut length = payload.len() + 1;
    loop {
        let candidate = format!("{length}{payload}");
        if candidate.len() == length {
            return candidate;
        }
        length = candidate.len();
    }
}
