use std::fmt;

use crate::hash::{Sha1, Sha512, from_base64, from_hex, to_base64, to_hex};

/// A hash function dealer can verify a tarball against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HashAlgorithm {
    /// Only published through the legacy `dist.shasum` field.
    Sha1,
    /// The algorithm used by modern `dist.integrity` metadata.
    Sha512,
}

impl HashAlgorithm {
    fn name(self) -> &'static str {
        match self {
            HashAlgorithm::Sha1 => "sha1",
            HashAlgorithm::Sha512 => "sha512",
        }
    }

    fn digest_length(self) -> usize {
        match self {
            HashAlgorithm::Sha1 => 20,
            HashAlgorithm::Sha512 => 64,
        }
    }

    fn digest(self, bytes: &[u8]) -> Vec<u8> {
        match self {
            HashAlgorithm::Sha1 => Sha1::digest(bytes).to_vec(),
            HashAlgorithm::Sha512 => Sha512::digest(bytes).to_vec(),
        }
    }
}

/// A checksum a package tarball is expected to hash to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Integrity {
    algorithm: HashAlgorithm,
    digest: Vec<u8>,
}

impl Integrity {
    pub fn new(algorithm: HashAlgorithm, digest: Vec<u8>) -> Result<Self, String> {
        if digest.len() != algorithm.digest_length() {
            return Err(format!(
                "{} digests must be {} bytes, got {}",
                algorithm.name(),
                algorithm.digest_length(),
                digest.len()
            ));
        }

        Ok(Self { algorithm, digest })
    }

    /// Parses a Subresource Integrity string such as `sha512-<base64>`.
    ///
    /// Registries may publish several space separated hashes; the strongest
    /// supported one wins and unknown algorithms are ignored.
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut strongest: Option<Integrity> = None;
        let mut last_error = None;

        for entry in value.split_whitespace() {
            let Some((algorithm, encoded)) = entry.split_once('-') else {
                last_error = Some(format!("`{entry}` is not an `<algorithm>-<digest>` pair"));
                continue;
            };

            let algorithm = match algorithm {
                "sha1" => HashAlgorithm::Sha1,
                "sha512" => HashAlgorithm::Sha512,
                _ => continue,
            };

            match from_base64(encoded).and_then(|digest| Integrity::new(algorithm, digest)) {
                Ok(candidate) => {
                    if strongest
                        .as_ref()
                        .is_none_or(|current| candidate.algorithm > current.algorithm)
                    {
                        strongest = Some(candidate);
                    }
                }
                Err(error) => last_error = Some(error),
            }
        }

        strongest.ok_or_else(|| {
            last_error.unwrap_or_else(|| format!("`{value}` contains no supported hash algorithm"))
        })
    }

    /// Parses the legacy hexadecimal `dist.shasum` field.
    pub fn from_shasum(value: &str) -> Result<Self, String> {
        Integrity::new(HashAlgorithm::Sha1, from_hex(value)?)
    }

    pub fn algorithm(&self) -> HashAlgorithm {
        self.algorithm
    }

    pub fn digest(&self) -> &[u8] {
        &self.digest
    }

    /// Confirms `bytes` hashes to this checksum.
    pub fn verify(&self, bytes: &[u8]) -> Result<(), String> {
        let actual = self.algorithm.digest(bytes);
        if actual == self.digest {
            return Ok(());
        }

        Err(format!(
            "integrity check failed: expected {self}, got {}-{}",
            self.algorithm.name(),
            to_base64(&actual)
        ))
    }

    /// The hexadecimal form of the digest, used for store and cache keys.
    pub fn to_hex(&self) -> String {
        to_hex(&self.digest)
    }
}

impl fmt::Display for Integrity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}-{}",
            self.algorithm.name(),
            to_base64(&self.digest)
        )
    }
}
