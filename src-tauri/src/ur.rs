//! UR (Blockchain Commons) as the Shell spells it: `UR:DEV-AUTH/<bytewords>`, single part, minimal
//! style - two letters per byte over a trailing CRC-32. Base32 bodies and multi-part sequences are
//! what other wallets use; a device that sent one would not be answering this app.

use crate::bytewords::BYTEWORDS;
use thiserror::Error;

/// The one UR type this app writes and reads.
pub const DEV_AUTH_TYPE: &str = "DEV-AUTH";

#[derive(Debug, Error)]
#[error("{0}")]
pub struct UrError(String);

fn refuse<T>(reason: impl Into<String>) -> Result<T, UrError> {
    Err(UrError(reason.into()))
}

/// Two letters per byte, then the CRC-32 of the payload spelled the same way.
pub fn encode_minimal(payload: &[u8]) -> String {
    let mut out = String::with_capacity((payload.len() + 4) * 2);
    let crc = crc32(payload);
    for byte in payload.iter().chain(crc.to_be_bytes().iter()) {
        let word = BYTEWORDS[usize::from(*byte)];
        out.push_str(&word[0..1]);
        out.push_str(&word[3..4]);
    }
    out
}

/// The payload, once the spelling and its CRC-32 have been checked. Lowercase, as a scanner writes.
pub fn decode_minimal(text: &str) -> Result<Vec<u8>, UrError> {
    if !text.is_ascii() {
        return refuse("bytewords contain characters outside ASCII");
    }
    if text.len() % 2 != 0 {
        return refuse("bytewords have an odd number of letters");
    }
    let pairs: Vec<u8> = text
        .as_bytes()
        .chunks(2)
        .map(|pair| minimal_index(pair[0], pair[1]))
        .collect::<Option<Vec<u8>>>()
        .ok_or_else(|| UrError("bytewords include a code that does not exist".into()))?;
    if pairs.len() < 4 {
        return refuse("bytewords are too short to carry a checksum");
    }
    let split = pairs.len() - 4;
    let (payload, checksum) = pairs.split_at(split);
    let given = u32::from_be_bytes([checksum[0], checksum[1], checksum[2], checksum[3]]);
    if crc32(payload) != given {
        return refuse("bytewords checksum does not match its content");
    }
    Ok(payload.to_vec())
}

fn minimal_index(first: u8, last: u8) -> Option<u8> {
    // First and last letters are a unique pair per word, so the words are the lookup table.
    BYTEWORDS
        .iter()
        .position(|word| word.as_bytes()[0] == first && word.as_bytes()[3] == last)
        .map(|index| index as u8)
}

/// The text of the challenge QR, and of a scanned answer.
pub fn ur_encode(payload: &[u8]) -> String {
    format!("UR:{DEV_AUTH_TYPE}/{}", encode_minimal(payload))
}

/// The payload of a scanned `UR:DEV-AUTH/...`, lower-cased first because a scanner folds case.
pub fn ur_decode(text: &str) -> Result<Vec<u8>, UrError> {
    let lowered = text.trim().to_lowercase();
    let Some(rest) = lowered.strip_prefix("ur:") else {
        return refuse("the scanned code is not a UR");
    };
    let Some((kind, body)) = rest.split_once('/') else {
        return refuse("the scanned code names a type but carries no payload");
    };
    if body.contains('/') {
        return refuse("the scanned code is split into parts, which this exchange never sends");
    }
    if kind != DEV_AUTH_TYPE.to_lowercase() {
        return refuse(format!(
            "the scanned code is a {kind} code, not a device authentication code"
        ));
    }
    if body.is_empty() || !body.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return refuse("the scanned code carries something other than bytewords");
    }
    decode_minimal(body)
}

/// CRC-32 (IEEE 802.3): the checksum Bytewords carries, not the one zlib's `adler32` would give.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_is_the_ieee_one() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn minimal_codes_are_the_devices_table() {
        // Spelled out by the vendor bundle the firmware ships, so a table edit shows up here.
        assert_eq!(
            [0x00u8, 0x01, 0x02, 0x03]
                .iter()
                .map(|byte| {
                    let word = BYTEWORDS[usize::from(*byte)];
                    format!("{}{}", &word[0..1], &word[3..4])
                })
                .collect::<String>(),
            "aeadaoax"
        );
        assert_eq!(
            [0xfcu8, 0xfd, 0xfe, 0xff]
                .iter()
                .map(|byte| {
                    let word = BYTEWORDS[usize::from(*byte)];
                    format!("{}{}", &word[0..1], &word[3..4])
                })
                .collect::<String>(),
            "ztzczezm"
        );
        let mut pairs: Vec<String> = BYTEWORDS
            .iter()
            .map(|word| format!("{}{}", &word[0..1], &word[3..4]))
            .collect();
        pairs.sort();
        pairs.dedup();
        assert_eq!(pairs.len(), 256, "two bytes would spell the same code");
    }

    #[test]
    fn payload_round_trips_through_a_scanners_case() {
        let payload = vec![0xa2, 0x01, 0x01, 0x06, 0x58, 0x20, 0x07, 0xff];
        let text = ur_encode(&payload);
        assert!(text.starts_with("UR:DEV-AUTH/"));
        assert_eq!(ur_decode(&text).unwrap(), payload);
        assert_eq!(ur_decode(&text.to_lowercase()).unwrap(), payload);
    }

    #[test]
    fn two_letters_per_byte_plus_the_checksum() {
        let payload = vec![0u8; 38];
        assert_eq!(
            ur_encode(&payload).split_once('/').unwrap().1.len(),
            (payload.len() + 4) * 2
        );
    }

    #[test]
    fn a_body_that_is_not_bytewords_is_refused() {
        // Base32, as another wallet would write it.
        assert!(ur_decode("UR:DEV-AUTH/IBSGAYTBOJSXG42PNRSGK5DBN5ZGK5DUPBSXG2DB").is_err());
        assert!(ur_decode("UR:DEV-AUTH/1-3/oeadadamhdcx").is_err());
        assert!(ur_decode("UR:crypto-account/aeadaoax").is_err());
        assert!(ur_decode("not a code at all").is_err());
        assert!(
            ur_decode("UR:DEV-AUTH/oeadadamhdcx").is_err(),
            "checksum missing"
        );
    }

    #[test]
    fn a_flipped_letter_breaks_the_checksum() {
        let text = ur_encode(&[0xa2, 0x01, 0x01, 0x06, 0x58, 0x20, 0x07]);
        let tail = text.len() - 2;
        let swapped = format!("{}lk", &text[..tail]);
        assert!(ur_decode(&swapped).is_err());
    }
}
