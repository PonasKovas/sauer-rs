use cube_string::CubeString;
use digest::Digest;
use p192::{
    AffinePoint, NistP192, ProjectivePoint, Scalar,
    elliptic_curve::{
        bigint::U192,
        ops::Reduce,
        sec1::{EncodedPoint, FromEncodedPoint, ToEncodedPoint},
    },
};

// ── Password hash ─────────────────────────────────────────────────────────────

/// Tiger("{cn} {session_id} {password}") → "{w0:x}{w1:x}{w2:x}"
///
/// Matches Sauerbraten's `hashpassword` in server.cpp.
pub fn hash_password(cn: u32, session_id: i32, password: &str) -> String {
    let input = format!("{} {} {}", cn, session_id, password);
    let result = tiger::Tiger::digest(input.as_bytes());
    let w0 = u64::from_le_bytes(result[0..8].try_into().unwrap());
    let w1 = u64::from_le_bytes(result[8..16].try_into().unwrap());
    let w2 = u64::from_le_bytes(result[16..24].try_into().unwrap());
    format!("{:x}{:x}{:x}", w0, w1, w2)
}

// ── ECC auth ──────────────────────────────────────────────────────────────────

/// Generate an ECC auth challenge for a client.
///
/// Mirrors `genchallenge` in crypto.cpp:
///   1. Tiger-hash a random seed → `scalar`
///   2. `expected = pubkey * scalar`   (client must return `expected.x`)
///   3. `secret   = G * scalar`        (sent to client as the challenge point)
///
/// Returns `(challenge_str, expected_x)` where `challenge_str` is `[+-]hex_x`
/// and `expected_x` is the 24-byte big-endian x-coordinate the client must produce.
/// Returns `None` if `pubkey_str` is not a valid Sauerbraten point.
pub fn gen_auth_challenge(pubkey_str: &CubeString) -> Option<(CubeString, [u8; 24])> {
    let pubkey = parse_sauer_point(&pubkey_str.to_string())?;

    // Tiger-hash a random seed to get the challenge scalar.
    let seed: [u8; 32] = rand::random();
    let hash_ga = tiger::Tiger::digest(&seed[..]);
    let mut hash = [0u8; 24];
    hash.copy_from_slice(&hash_ga);

    let scalar = scalar_from_le_bytes(&hash);

    // expected = pubkey * scalar  (correct answer x the client must send back)
    let expected = (ProjectivePoint::from(pubkey) * scalar).to_affine();
    let expected_x = affine_x_bytes(&expected);

    // secret = G * scalar  (challenge point sent to client)
    let secret = (ProjectivePoint::GENERATOR * scalar).to_affine();
    let challenge_str = print_sauer_point(&secret);

    Some((CubeString::from_str_lossy(&challenge_str), expected_x))
}

/// Verify a client's ECC answer against the expected x-coordinate.
///
/// `answer_str` is the client's hex-encoded x-coordinate (Sauerbraten bigint format).
/// `expected_x` is the 24-byte big-endian value stored when the challenge was generated.
pub fn verify_auth_answer(answer_str: &CubeString, expected_x: &[u8; 24]) -> bool {
    match parse_sauer_hex(&answer_str.to_string()) {
        Some(bytes) => bytes == *expected_x,
        None => false,
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Treat 24 little-endian bytes as a 192-bit integer and reduce mod curve order.
fn scalar_from_le_bytes(bytes: &[u8; 24]) -> Scalar {
    let mut be = *bytes;
    be.reverse();
    let uint = U192::from_be_slice(&be);
    <Scalar as Reduce<U192>>::reduce(uint)
}

/// Extract the 24-byte big-endian x-coordinate from an affine point.
fn affine_x_bytes(point: &AffinePoint) -> [u8; 24] {
    let encoded = point.to_encoded_point(false);
    let x = encoded.x().expect("affine point has x");
    let mut out = [0u8; 24];
    out.copy_from_slice(x.as_ref());
    out
}

/// Format an affine point as `[+-]hex_x` in Sauerbraten's bigint print format.
///
/// '+' = y is even (LSB = 0), '-' = y is odd (LSB = 1).
/// x is printed as a hex string of 16-bit groups (4 hex chars each) with leading
/// zero groups stripped, matching `ecjacobian::print` in crypto.cpp.
fn print_sauer_point(point: &AffinePoint) -> String {
    let encoded = point.to_encoded_point(false);
    let x_bytes: &[u8] = encoded.x().expect("affine point has x");
    let y_bytes: &[u8] = encoded.y().expect("affine point has y");

    // LSB of y in bigint terms = bit 0 of the last (least-significant) byte in big-endian.
    let ybit = y_bytes[y_bytes.len() - 1] & 1 != 0;
    let sign = if ybit { '-' } else { '+' };

    // Full 48-char hex of x (big-endian bytes).
    let hex: String = x_bytes.iter().map(|b| format!("{:02x}", b)).collect();

    // Strip leading zero groups (each group is 4 hex chars = one 16-bit bigint digit).
    let mut start = 0;
    while start + 4 <= hex.len() && &hex[start..start + 4] == "0000" {
        start += 4;
    }
    let hex_x = if start < hex.len() { &hex[start..] } else { "0000" };

    format!("{}{}", sign, hex_x)
}

/// Parse a Sauerbraten `[+-]hex_x` point into an affine point.
fn parse_sauer_point(s: &str) -> Option<AffinePoint> {
    if s.is_empty() {
        return None;
    }
    let (sign, hex_x) = s.split_at(1);
    let ybit = match sign {
        "+" => false,
        "-" => true,
        _ => return None,
    };

    let x_bytes = parse_sauer_hex(hex_x)?;

    // Build a SEC1 compressed point: 0x02 (even y) or 0x03 (odd y) || x.
    let prefix = if ybit { 0x03u8 } else { 0x02u8 };
    let mut buf = [0u8; 25];
    buf[0] = prefix;
    buf[1..].copy_from_slice(&x_bytes);

    let ep = EncodedPoint::<NistP192>::from_bytes(&buf).ok()?;
    Option::from(AffinePoint::from_encoded_point(&ep))
}

/// Parse a Sauerbraten bigint hex string to 24 big-endian bytes.
///
/// Sauerbraten prints bigints as groups of 4 hex chars (one 16-bit digit each),
/// most-significant first, with leading zero groups stripped. The string length
/// must be a multiple of 4 (or empty for zero).
fn parse_sauer_hex(hex: &str) -> Option<[u8; 24]> {
    if hex.len() > 48 || hex.len() % 4 != 0 {
        return None;
    }
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }

    let mut bytes = [0u8; 24];
    let num_bytes = hex.len() / 2;
    let offset = 24 - num_bytes;
    let hex_b = hex.as_bytes();

    for i in 0..num_bytes {
        let hi = hex_nibble(hex_b[i * 2])?;
        let lo = hex_nibble(hex_b[i * 2 + 1])?;
        bytes[offset + i] = (hi << 4) | lo;
    }

    Some(bytes)
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
