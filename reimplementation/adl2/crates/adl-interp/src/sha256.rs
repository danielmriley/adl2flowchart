//! A self-contained SHA-256 (FIPS 180-4) — vendored rather than pulled as
//! a dependency, the same discipline `rootfile` uses for its streamer
//! blobs: the algorithm is fixed and small, and provenance hashing
//! (SPEC_EVENT_PIPELINE §6) must be byte-deterministic with zero external
//! surface. Verified against the FIPS test vectors in the module tests.

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

const H0: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// Lowercase hex SHA-256 digest of `data` (one-shot).
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex(h.finalize())
}

/// An incremental SHA-256 hasher: feed bytes with [`Sha256::update`] in any
/// chunking, then [`Sha256::finalize`]. Holds only the 8-word state and a
/// single 64-byte block buffer, so hashing a multi-gigabyte stream uses
/// **O(1)** memory (SPEC_EVENT_PIPELINE §5/§6 — the input identity hashes a
/// 1M-event file without buffering it).
#[derive(Debug, Clone)]
pub struct Sha256 {
    h: [u32; 8],
    /// Bytes seen but not yet absorbed (always `< 64`).
    buf: [u8; 64],
    buf_len: usize,
    total_len: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    #[must_use]
    pub fn new() -> Self {
        Self {
            h: H0,
            buf: [0; 64],
            buf_len: 0,
            total_len: 0,
        }
    }

    /// Absorb more input. Any chunking yields the same digest.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u64);
        // Top up a partial buffer to a full block first.
        if self.buf_len > 0 {
            let take = (64 - self.buf_len).min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                compress(&mut self.h, &block);
                self.buf_len = 0;
            } else {
                // The buffer is still partial and all input is consumed;
                // leave `buf_len` intact (the remainder step below would
                // otherwise clobber it to zero).
                debug_assert!(data.is_empty());
                return;
            }
        }
        // Absorb whole blocks straight from the input.
        let mut chunks = data.chunks_exact(64);
        for block in &mut chunks {
            compress(&mut self.h, block.try_into().expect("64-byte block"));
        }
        let rem = chunks.remainder();
        self.buf[..rem.len()].copy_from_slice(rem);
        self.buf_len = rem.len();
    }

    /// Finish and return the lowercase hex digest, consuming the hasher.
    #[must_use]
    pub fn finalize_hex(self) -> String {
        hex(self.finalize())
    }

    /// Finish and return the 32-byte digest, consuming the hasher.
    #[must_use]
    pub fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len.wrapping_mul(8);
        // Padding: 0x80, then zeros to leave 8 bytes, then the bit length.
        let mut pad = [0u8; 72];
        pad[0] = 0x80;
        let zeros = if self.buf_len < 56 {
            56 - self.buf_len
        } else {
            120 - self.buf_len
        };
        let tail = zeros + 8;
        pad[zeros..tail].copy_from_slice(&bit_len.to_be_bytes());
        self.update(&pad[..tail]);
        debug_assert_eq!(self.buf_len, 0, "padding closes the final block");
        let mut out = [0u8; 32];
        for (i, word) in self.h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

fn hex(digest: [u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push(char::from_digit(u32::from(b >> 4), 16).expect("nibble"));
        out.push(char::from_digit(u32::from(b & 0x0f), 16).expect("nibble"));
    }
    out
}

/// One 64-byte block of the SHA-256 compression function.
fn compress(h: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (i, word) in w.iter_mut().take(16).enumerate() {
        let j = i * 4;
        *word = u32::from_be_bytes([block[j], block[j + 1], block[j + 2], block[j + 3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }

    for (hv, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *hv = hv.wrapping_add(v);
    }
}

#[cfg(test)]
mod tests {
    use super::{Sha256, hex, sha256_hex};

    #[test]
    fn fips_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn multi_block_and_length_edges() {
        // 1,000,000 'a' bytes — the canonical long FIPS vector, exercising
        // many blocks and the bit-length encoding.
        let million = vec![b'a'; 1_000_000];
        assert_eq!(
            sha256_hex(&million),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        // 55 bytes: the last block that still fits its own padding byte;
        // 56 bytes: forces a second padding block.
        assert_eq!(sha256_hex(&[b'x'; 55]).len(), 64, "55-byte input hashes");
        assert_eq!(
            sha256_hex(&[b'x'; 56]).len(),
            64,
            "56-byte input hashes (extra block)"
        );
    }

    #[test]
    fn incremental_matches_one_shot() {
        // Any chunking of any input must agree with the one-shot digest —
        // this is what lets `run` hash a streamed file without buffering it.
        let data: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        for chunk in [1usize, 7, 56, 63, 64, 65, 128, 1000] {
            let mut h = Sha256::new();
            for piece in data.chunks(chunk) {
                h.update(piece);
            }
            assert_eq!(
                hex(h.finalize()),
                sha256_hex(&data),
                "chunked-by-{chunk} digest must equal one-shot"
            );
        }
        // The 1M-vector through the streaming path.
        let million = vec![b'a'; 1_000_000];
        let mut h = Sha256::new();
        for piece in million.chunks(997) {
            h.update(piece);
        }
        assert_eq!(
            hex(h.finalize()),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}
