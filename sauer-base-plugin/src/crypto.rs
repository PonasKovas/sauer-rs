use cube_string::{CubeStr, CubeString};
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

/// Tiger("{cn} {session_id} {password}") → "{w0:x}{w1:x}{w2:x}" as a CubeString.
///
/// Matches Sauerbraten's `hashpassword` in server.cpp.
pub fn hash_password(cn: u32, session_id: i32, password: &CubeStr) -> CubeString {
    // Build Tiger input as raw bytes without any UTF-8 conversion.
    let mut input = Vec::new();
    write_dec_u32(&mut input, cn);
    input.push(b' ');
    write_dec_i32(&mut input, session_id);
    input.push(b' ');
    input.extend_from_slice(password.as_bytes());

    let hash = tiger::Tiger::digest(&input);
    let w0 = u64::from_le_bytes(hash[0..8].try_into().unwrap());
    let w1 = u64::from_le_bytes(hash[8..16].try_into().unwrap());
    let w2 = u64::from_le_bytes(hash[16..24].try_into().unwrap());

    let mut out = Vec::new();
    write_hex_u64(&mut out, w0);
    write_hex_u64(&mut out, w1);
    write_hex_u64(&mut out, w2);
    CubeString::from_bytes(out)
}

// ── ECC auth ──────────────────────────────────────────────────────────────────

/// Generate an ECC auth challenge for a client.
///
/// Mirrors `genchallenge` in crypto.cpp:
///   1. Tiger-hash a random seed → `scalar`
///   2. `expected = pubkey * scalar`   (client must return `expected.x`)
///   3. `secret   = G * scalar`        (sent to client as the challenge point)
///
/// Returns `(challenge_str, expected_x_bytes)` or `None` if `pubkey_str` is invalid.
pub fn gen_auth_challenge(pubkey_str: &CubeStr) -> Option<(CubeString, [u8; 24])> {
    let pubkey = parse_sauer_point(pubkey_str.as_bytes())?;

    let seed: [u8; 32] = rand::random();
    let hash_ga = tiger::Tiger::digest(&seed[..]);
    let mut hash = [0u8; 24];
    hash.copy_from_slice(&hash_ga);

    let scalar = scalar_from_le_bytes(&hash);

    let expected = (ProjectivePoint::from(pubkey) * scalar).to_affine();
    let expected_x = affine_x_bytes(&expected);

    let secret = (ProjectivePoint::GENERATOR * scalar).to_affine();
    let challenge = print_sauer_point(&secret);

    Some((challenge, expected_x))
}

/// Verify a client's ECC answer against the expected x-coordinate.
///
/// `answer_str` is the client's hex-encoded x-coordinate (Sauerbraten bigint format).
/// `expected_x` is the 24-byte big-endian value stored when the challenge was generated.
pub fn verify_auth_answer(answer_str: &CubeStr, expected_x: &[u8; 24]) -> bool {
    match parse_sauer_hex(answer_str.as_bytes()) {
        Some(bytes) => bytes == *expected_x,
        None => false,
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn scalar_from_le_bytes(bytes: &[u8; 24]) -> Scalar {
    let mut be = *bytes;
    be.reverse();
    let uint = U192::from_be_slice(&be);
    <Scalar as Reduce<U192>>::reduce(uint)
}

fn affine_x_bytes(point: &AffinePoint) -> [u8; 24] {
    let encoded = point.to_encoded_point(false);
    let x = encoded.x().expect("affine point has x");
    let mut out = [0u8; 24];
    out.copy_from_slice(x.as_ref());
    out
}

/// Format an affine point as `[+-]hex_x` in Sauerbraten's bigint print format.
fn print_sauer_point(point: &AffinePoint) -> CubeString {
    let encoded = point.to_encoded_point(false);
    let x_bytes: &[u8] = encoded.x().expect("affine point has x");
    let y_bytes: &[u8] = encoded.y().expect("affine point has y");

    let ybit = y_bytes[y_bytes.len() - 1] & 1 != 0;

    let mut out = Vec::with_capacity(50);
    out.push(if ybit { b'-' } else { b'+' });

    // Full 48-char hex of x (big-endian bytes).
    let mut hex = Vec::with_capacity(48);
    for b in x_bytes {
        write_hex_byte(&mut hex, *b);
    }

    // Strip leading zero 16-bit groups (4 hex chars each).
    let mut start = 0;
    while start + 4 <= hex.len() && &hex[start..start + 4] == b"0000" {
        start += 4;
    }
    if start < hex.len() {
        out.extend_from_slice(&hex[start..]);
    } else {
        out.extend_from_slice(b"0000");
    }

    CubeString::from_bytes(out)
}

/// Parse a Sauerbraten `[+-]hex_x` point.
fn parse_sauer_point(s: &[u8]) -> Option<AffinePoint> {
    let (&sign, hex_x) = s.split_first()?;
    let ybit = match sign {
        b'+' => false,
        b'-' => true,
        _ => return None,
    };

    let x_bytes = parse_sauer_hex(hex_x)?;

    let prefix = if ybit { 0x03u8 } else { 0x02u8 };
    let mut buf = [0u8; 25];
    buf[0] = prefix;
    buf[1..].copy_from_slice(&x_bytes);

    let ep = EncodedPoint::<NistP192>::from_bytes(&buf).ok()?;
    Option::from(AffinePoint::from_encoded_point(&ep))
}

/// Parse Sauerbraten bigint hex to 24 big-endian bytes.
/// Length must be a multiple of 4 (each 16-bit group = 4 hex chars).
fn parse_sauer_hex(hex: &[u8]) -> Option<[u8; 24]> {
    if hex.len() > 48 || hex.len() % 4 != 0 {
        return None;
    }
    if !hex.iter().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }

    let mut bytes = [0u8; 24];
    let num_bytes = hex.len() / 2;
    let offset = 24 - num_bytes;

    for i in 0..num_bytes {
        let hi = hex_nibble(hex[i * 2])?;
        let lo = hex_nibble(hex[i * 2 + 1])?;
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

fn write_hex_byte(buf: &mut Vec<u8>, b: u8) {
    const HEX: &[u8] = b"0123456789abcdef";
    buf.push(HEX[(b >> 4) as usize]);
    buf.push(HEX[(b & 0xF) as usize]);
}

fn write_hex_u64(buf: &mut Vec<u8>, mut n: u64) {
    const HEX: &[u8] = b"0123456789abcdef";
    if n == 0 {
        buf.push(b'0');
        return;
    }
    let start = buf.len();
    while n > 0 {
        buf.push(HEX[(n & 0xF) as usize]);
        n >>= 4;
    }
    buf[start..].reverse();
}

fn write_dec_u32(buf: &mut Vec<u8>, mut n: u32) {
    if n == 0 {
        buf.push(b'0');
        return;
    }
    let start = buf.len();
    while n > 0 {
        buf.push(b'0' + (n % 10) as u8);
        n /= 10;
    }
    buf[start..].reverse();
}

fn write_dec_i32(buf: &mut Vec<u8>, n: i32) {
    if n < 0 {
        buf.push(b'-');
        write_dec_u32(buf, n.unsigned_abs());
    } else {
        write_dec_u32(buf, n as u32);
    }
}
