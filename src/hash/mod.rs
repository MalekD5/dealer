//! Hashing and encoding primitives used to verify package integrity.

pub mod sha1;
pub mod sha512;

pub use sha1::Sha1;
pub use sha512::Sha512;

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encodes bytes as lowercase hexadecimal.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(hex_digit(byte >> 4));
        encoded.push(hex_digit(byte & 0x0f));
    }
    encoded
}

/// Decodes a lowercase or uppercase hexadecimal string.
pub fn from_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("hexadecimal digests must contain an even number of digits".to_string());
    }

    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_value(pair[0])?;
            let low = hex_value(pair[1])?;
            Ok(high << 4 | low)
        })
        .collect()
}

/// Encodes bytes as padded standard base64.
pub fn to_base64(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for group in bytes.chunks(3) {
        let packed = group
            .iter()
            .enumerate()
            .fold(0u32, |packed, (index, byte)| {
                packed | u32::from(*byte) << (16 - index * 8)
            });

        for index in 0..=group.len() {
            let sextet = (packed >> (18 - index * 6)) & 0x3f;
            encoded.push(char::from(BASE64_ALPHABET[sextet as usize]));
        }
        for _ in group.len()..3 {
            encoded.push('=');
        }
    }

    encoded
}

/// Decodes padded standard base64.
pub fn from_base64(value: &str) -> Result<Vec<u8>, String> {
    let trimmed = value.trim_end_matches('=');
    let mut decoded = Vec::with_capacity(trimmed.len() / 4 * 3);
    let mut packed = 0u32;
    let mut sextets = 0;

    for byte in trimmed.bytes() {
        let sextet = base64_value(byte)?;
        packed = packed << 6 | u32::from(sextet);
        sextets += 1;

        if sextets == 4 {
            decoded.extend_from_slice(&packed.to_be_bytes()[1..]);
            packed = 0;
            sextets = 0;
        }
    }

    match sextets {
        0 => {}
        2 => decoded.push((packed >> 4) as u8),
        3 => {
            decoded.push((packed >> 10) as u8);
            decoded.push((packed >> 2) as u8);
        }
        _ => return Err("base64 input has a truncated final group".to_string()),
    }

    Ok(decoded)
}

fn hex_digit(nibble: u8) -> char {
    char::from(match nibble {
        0..=9 => b'0' + nibble,
        _ => b'a' + nibble - 10,
    })
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!(
            "`{}` is not a hexadecimal digit",
            char::from(byte).escape_default()
        )),
    }
}

fn base64_value(byte: u8) -> Result<u8, String> {
    BASE64_ALPHABET
        .iter()
        .position(|candidate| *candidate == byte)
        .map(|position| position as u8)
        .ok_or_else(|| {
            format!(
                "`{}` is not a base64 character",
                char::from(byte).escape_default()
            )
        })
}
