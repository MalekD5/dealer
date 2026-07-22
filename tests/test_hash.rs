use dealer::hash::{Sha1, Sha512, from_base64, from_hex, to_base64, to_hex};

/// The two block message from the FIPS 180-4 examples.
const TWO_BLOCK_MESSAGE: &str = "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmno\
                                 ijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";

#[test]
fn sha512_matches_published_vectors() {
    assert_eq!(
        to_hex(&Sha512::digest(b"")),
        "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
         47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
    );
    assert_eq!(
        to_hex(&Sha512::digest(b"abc")),
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
         2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
    assert_eq!(
        to_hex(&Sha512::digest(TWO_BLOCK_MESSAGE.as_bytes())),
        "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018\
         501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909"
    );
}

#[test]
fn sha1_matches_published_vectors() {
    assert_eq!(
        to_hex(&Sha1::digest(b"")),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );
    assert_eq!(
        to_hex(&Sha1::digest(b"abc")),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        to_hex(&Sha1::digest(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
    );
}

/// Streaming in awkward chunk sizes must agree with hashing in one shot.
#[test]
fn streaming_updates_match_a_single_call() {
    let message: Vec<u8> = (0..1000u32).map(|value| value as u8).collect();

    for chunk_size in [1, 7, 63, 64, 65, 127, 128, 129] {
        let mut sha512 = Sha512::new();
        let mut sha1 = Sha1::new();
        for chunk in message.chunks(chunk_size) {
            sha512.update(chunk);
            sha1.update(chunk);
        }

        assert_eq!(sha512.finish(), Sha512::digest(&message), "chunk size {chunk_size}");
        assert_eq!(sha1.finish(), Sha1::digest(&message), "chunk size {chunk_size}");
    }
}

#[test]
fn base64_round_trips_every_padding_length() {
    let cases = [
        (&b""[..], ""),
        (b"f", "Zg=="),
        (b"fo", "Zm8="),
        (b"foo", "Zm9v"),
        (b"foob", "Zm9vYg=="),
        (b"fooba", "Zm9vYmE="),
        (b"foobar", "Zm9vYmFy"),
    ];

    for (bytes, encoded) in cases {
        assert_eq!(to_base64(bytes), encoded);
        assert_eq!(from_base64(encoded).unwrap(), bytes);
    }

    assert!(from_base64("Zm9v!!").is_err());
}

#[test]
fn hex_round_trips_and_rejects_bad_input() {
    assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "000fff");
    assert_eq!(from_hex("000FFF").unwrap(), vec![0x00, 0x0f, 0xff]);
    assert!(from_hex("abc").is_err());
    assert!(from_hex("zz").is_err());
}
