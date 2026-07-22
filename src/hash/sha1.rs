//! A minimal SHA-1 implementation (FIPS 180-4).
//!
//! SHA-1 is only used to check the legacy `dist.shasum` field published by the
//! npm registry; it is never used to derive store keys.

const BLOCK_LENGTH: usize = 64;

const INITIAL_STATE: [u32; 5] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];

/// A streaming SHA-1 hasher.
pub struct Sha1 {
    state: [u32; 5],
    buffer: [u8; BLOCK_LENGTH],
    buffered: usize,
    length: u64,
}

impl Sha1 {
    pub fn new() -> Self {
        Self {
            state: INITIAL_STATE,
            buffer: [0; BLOCK_LENGTH],
            buffered: 0,
            length: 0,
        }
    }

    /// Hashes `bytes` in one shot.
    pub fn digest(bytes: &[u8]) -> [u8; 20] {
        let mut hasher = Self::new();
        hasher.update(bytes);
        hasher.finish()
    }

    pub fn update(&mut self, mut bytes: &[u8]) {
        self.length += bytes.len() as u64;

        if self.buffered > 0 {
            let wanted = (BLOCK_LENGTH - self.buffered).min(bytes.len());
            self.buffer[self.buffered..self.buffered + wanted].copy_from_slice(&bytes[..wanted]);
            self.buffered += wanted;
            bytes = &bytes[wanted..];

            if self.buffered < BLOCK_LENGTH {
                return;
            }

            let block = self.buffer;
            self.compress(&block);
            self.buffered = 0;
        }

        let mut chunks = bytes.chunks_exact(BLOCK_LENGTH);
        for block in &mut chunks {
            self.compress(block);
        }

        let remainder = chunks.remainder();
        self.buffer[..remainder.len()].copy_from_slice(remainder);
        self.buffered = remainder.len();
    }

    pub fn finish(mut self) -> [u8; 20] {
        let bit_length = self.length * 8;

        self.update_without_counting(&[0x80]);
        while self.buffered != BLOCK_LENGTH - 8 {
            self.update_without_counting(&[0x00]);
        }
        self.update_without_counting(&bit_length.to_be_bytes());

        let mut digest = [0; 20];
        for (target, word) in digest.chunks_exact_mut(4).zip(self.state) {
            target.copy_from_slice(&word.to_be_bytes());
        }
        digest
    }

    fn update_without_counting(&mut self, bytes: &[u8]) {
        let length = self.length;
        self.update(bytes);
        self.length = length;
    }

    fn compress(&mut self, block: &[u8]) {
        let mut schedule = [0u32; 80];
        for (word, source) in schedule.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_be_bytes(source.try_into().expect("chunk is four bytes"));
        }
        for index in 16..80 {
            schedule[index] = (schedule[index - 3]
                ^ schedule[index - 8]
                ^ schedule[index - 14]
                ^ schedule[index - 16])
                .rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = self.state;

        for (index, word) in schedule.iter().enumerate() {
            let (mixed, constant) = match index {
                0..=19 => ((b & c) | (!b & d), 0x5a827999),
                20..=39 => (b ^ c ^ d, 0x6ed9eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1bbcdc),
                _ => (b ^ c ^ d, 0xca62c1d6),
            };

            let temporary = a
                .rotate_left(5)
                .wrapping_add(mixed)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);

            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temporary;
        }

        for (current, added) in self.state.iter_mut().zip([a, b, c, d, e]) {
            *current = current.wrapping_add(added);
        }
    }
}

impl Default for Sha1 {
    fn default() -> Self {
        Self::new()
    }
}
