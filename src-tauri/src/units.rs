//! The device allowlist: which Shell units left the factory, and the signed answer to a challenge.
//!
//! A list is verified by the secp256k1 signature inside it, over everything before the trailing 64
//! bytes; the pinned key is the trust, not the download location. A device answers a 32-byte
//! challenge with a signature over `uid || challenge`, and the row that matches it is
//! `sha256(salt || "ShellUID" || uid || "ShellPubKey" || recovered pubkey)` - so a list holds no
//! identifier that could be matched without the device signing for it.

use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Lists are published by `keycard-tech/shell-units`; this key is what says one came from them.
/// The one asset the units release carries.
pub const LIST_ASSET_NAME: &str = "shell-verification-db.bin";

pub const UNITS_VERIFY_KEY: &str =
    "032ac68f2b2a500ebf1bc31f0c36e4645b24530ce6d1460e92c2d94e9a47ba550e";

const MAGIC: &[u8; 4] = b"SHVR";
const FORMAT: u16 = 1;
const HEADER_SIZE: usize = 64;
const SIG_SIZE: usize = 64;
const ROW_LEN: usize = 32;
const UID_LEN: usize = 16;
const SALT_LEN: usize = 16;
const FORMAT_OFFSET: usize = 4;
const UID_LEN_OFFSET: usize = 6;
const ROW_COUNT_OFFSET: usize = 8;
const SALT_OFFSET: usize = 12;
const BUILD_OFFSET: usize = 28;
const TAG_UID: &[u8] = b"ShellUID";
const TAG_PUBKEY: &[u8] = b"ShellPubKey";

/// Fields a device answer must carry and nothing else: version, uid, challenge, signature.
const ANSWER_FIELDS: [usize; 4] = [1, 2, 6, 7];

#[derive(Debug, Error)]
pub enum UnitsError {
    /// Refused by the rules of the format, whichever file or answer it was.
    #[error("{0}")]
    Refusal(String),
    /// A well-formed answer from a unit that is not on the list: not a mistake, and not authentic.
    #[error("this Shell is not on the produced units list")]
    NotRegistered { uid: String },
}

fn refuse<T>(reason: impl Into<String>) -> Result<T, UnitsError> {
    Err(UnitsError::Refusal(reason.into()))
}

fn u16le(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32le(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// A list this key signed. One buffer, so `rows` cannot outlive the signature that authorised it.
pub struct UnitList {
    bytes: Vec<u8>,
    pub build: u32,
    pub row_count: usize,
}

impl UnitList {
    fn salt(&self) -> &[u8] {
        &self.bytes[SALT_OFFSET..SALT_OFFSET + SALT_LEN]
    }

    fn rows(&self) -> &[u8] {
        &self.bytes[HEADER_SIZE..self.bytes.len() - SIG_SIZE]
    }

    /// The row whose digest matches `hash`, or none. Rows are sorted, so this is a binary search.
    fn find(&self, hash: &[u8]) -> Option<usize> {
        let rows = self.rows();
        let (mut lo, mut hi) = (0, self.row_count);
        while lo < hi {
            let mid = (lo + hi) / 2;
            let at = &rows[mid * ROW_LEN..mid * ROW_LEN + ROW_LEN];
            match at.cmp(hash) {
                std::cmp::Ordering::Equal => return Some(mid),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        None
    }
}

fn recover(sig: &Signature, digest: &[u8], id: u8) -> Result<VerifyingKey, UnitsError> {
    VerifyingKey::recover_from_prehash(digest, sig, RecoveryId::new(id == 1, false))
        .map_err(|_| UnitsError::Refusal("signature does not carry a recoverable point".into()))
}

fn row_hash(salt: &[u8], uid: &[u8], pubkey: &[u8]) -> [u8; ROW_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(TAG_UID);
    hasher.update(uid);
    hasher.update(TAG_PUBKEY);
    hasher.update(pubkey);
    hasher.finalize().into()
}

/// Read a list and decide whether this key signed it, refusing what must not be reinterpreted.
pub fn load_list(bytes: &[u8]) -> Result<UnitList, UnitsError> {
    load_list_with_key(bytes, UNITS_VERIFY_KEY)
}

/// `verify_key` is a parameter so a test can sign its own list and exercise the rest of the rules.
pub fn load_list_with_key(bytes: &[u8], verify_key: &str) -> Result<UnitList, UnitsError> {
    if bytes.len() < HEADER_SIZE + SIG_SIZE {
        return refuse("file too short to be a units list");
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return refuse("not a units list");
    }
    if u16le(bytes, FORMAT_OFFSET) != FORMAT {
        return refuse(format!("units list format is not version {FORMAT}"));
    }
    let uid_len = bytes[UID_LEN_OFFSET] as usize;
    if uid_len != UID_LEN {
        return refuse(format!(
            "units list expects {uid_len} byte unit ids, this app reads {UID_LEN}"
        ));
    }
    if bytes[UID_LEN_OFFSET + 1] != 0
        || bytes[BUILD_OFFSET + 4..HEADER_SIZE].iter().any(|b| *b != 0)
    {
        return refuse("reserved bytes in the units list header are not zero");
    }

    let row_count = u32le(bytes, ROW_COUNT_OFFSET) as usize;
    if row_count == 0 {
        return refuse("units list has no rows");
    }
    let expected = HEADER_SIZE + row_count * ROW_LEN + SIG_SIZE;
    if bytes.len() != expected {
        return refuse(format!(
            "units list is {} bytes, its header describes {expected}",
            bytes.len()
        ));
    }

    let (prefix, signature) = bytes.split_at(bytes.len() - SIG_SIZE);
    let key = VerifyingKey::from_sec1_bytes(&unhex(verify_key)?)
        .map_err(|err| UnitsError::Refusal(format!("pinned key could not be read: {err}")))?;
    let sig = Signature::from_slice(signature)
        .map_err(|err| UnitsError::Refusal(format!("signature could not be read: {err}")))?;
    // Recovery is the same check with one API, and it takes a high-s signature as it comes.
    let digest = Sha256::digest(prefix);
    if !(0..2)
        .filter_map(|id| recover(&sig, &digest, id).ok())
        .any(|recovered| recovered == key)
    {
        return refuse("signature does not match the pinned key");
    }

    // Sorted, with repeats allowed: the padding rows are copies of one row.
    let rows = &bytes[HEADER_SIZE..bytes.len() - SIG_SIZE];
    for row in 1..row_count {
        let prev = &rows[(row - 1) * ROW_LEN..row * ROW_LEN];
        let next = &rows[row * ROW_LEN..(row + 1) * ROW_LEN];
        if prev > next {
            return refuse(format!("units list rows are not sorted at {row}"));
        }
    }

    Ok(UnitList {
        bytes: bytes.to_vec(),
        build: u32le(bytes, BUILD_OFFSET),
        row_count,
    })
}

struct Cbor<'a> {
    bytes: &'a [u8],
    at: usize,
    /// Which fields the map carried, in the order they came.
    keys: Vec<usize>,
    step: Option<usize>,
    uid: Option<Vec<u8>>,
    challenge: Option<Vec<u8>>,
    signature: Option<Vec<u8>>,
}

impl Cbor<'_> {
    fn byte(&mut self) -> Result<u8, UnitsError> {
        let byte = *self.bytes.get(self.at).ok_or_else(|| {
            UnitsError::Refusal("device answer ended in the middle of a value".into())
        })?;
        self.at += 1;
        Ok(byte)
    }

    /// Head of `major`: a number, or the length after it. Canonical only, so one answer has one spelling.
    fn head(&mut self, major: u8) -> Result<usize, UnitsError> {
        let byte = self.byte()?;
        if byte >> 5 != major {
            return refuse(format!(
                "device answer has a value where major type {major} belonged"
            ));
        }
        match byte & 0x1f {
            31 => refuse("device answer is not canonical CBOR"),
            short @ 0..=23 => Ok(usize::from(short)),
            extra @ 24..=27 => {
                let width = 1usize << (extra - 24);
                let mut value: u64 = 0;
                for _ in 0..width {
                    value = value * 256 + u64::from(self.byte()?);
                }
                if value < 1u64 << (8 * (width - 1)) {
                    return refuse("device answer uses a longer length than it needed");
                }
                usize::try_from(value)
                    .map_err(|_| UnitsError::Refusal("device answer claims a huge length".into()))
            }
            _ => refuse("device answer uses a reserved length"),
        }
    }

    fn byte_string(&mut self) -> Result<Vec<u8>, UnitsError> {
        let len = self.head(2)?;
        if self.at + len > self.bytes.len() {
            return refuse("device answer ended in the middle of a byte string");
        }
        let out = self.bytes[self.at..self.at + len].to_vec();
        self.at += len;
        Ok(out)
    }

    /// `{1: step, 2: #6.37(uid), 3..5: other steps, 6: challenge, 7: signature}`.
    fn dev_auth(&mut self) -> Result<(), UnitsError> {
        let count = self.head(5)?;
        for _ in 0..count {
            let key = self.head(0)?;
            if self.keys.contains(&key) {
                return refuse(format!("device answer repeats field {key}"));
            }
            self.keys.push(key);
            match key {
                1 => self.step = Some(self.head(0)?),
                2 => {
                    if self.bytes.get(self.at).map(|head| head >> 5) != Some(6) {
                        return refuse("the unit id is not a tagged value, expected tag 37");
                    }
                    let tag = self.head(6)?;
                    if tag != 37 {
                        return refuse(format!(
                            "device answer tags its unit id {tag}, expected 37 (a positive bignum)"
                        ));
                    }
                    self.uid = Some(self.byte_string()?);
                }
                3..=5 => {
                    self.head(0)?;
                }
                6 => self.challenge = Some(self.byte_string()?),
                7 => self.signature = Some(self.byte_string()?),
                other => return refuse(format!("device answer has an unexpected field {other}")),
            }
        }
        if self.at != self.bytes.len() {
            return refuse("device answer has bytes after its last field");
        }
        Ok(())
    }
}

/// The init the challenge QR carries: `{1: 1, 6: challenge}` with the challenge as a byte string.
pub fn encode_init(challenge: &[u8; 32]) -> Vec<u8> {
    let mut out = vec![0xa2, 0x01, 0x01, 0x06, 0x58, 0x20];
    out.extend_from_slice(challenge);
    out
}

/// The row of the list this answer matches, or why it proves nothing.
pub fn check_answer(
    list: &UnitList,
    challenge: &[u8; 32],
    payload: &[u8],
) -> Result<usize, UnitsError> {
    let mut answer = Cbor {
        bytes: payload,
        at: 0,
        keys: Vec::new(),
        step: None,
        uid: None,
        challenge: None,
        signature: None,
    };
    answer.dev_auth()?;

    let mut keys = answer.keys.clone();
    keys.sort_unstable();
    if keys != ANSWER_FIELDS {
        return refuse(format!(
            "device answer must carry fields 1, 2, 6 and 7 - it carried {}",
            keys.iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    match answer.step {
        Some(2) => {}
        Some(other) => return refuse(format!("device answered step {other}, expected 2")),
        None => return refuse("device answer has no step"),
    }
    let uid = answer
        .uid
        .ok_or_else(|| UnitsError::Refusal("no unit id".into()))?;
    let signature = answer
        .signature
        .ok_or_else(|| UnitsError::Refusal("no signature".into()))?;
    if answer.challenge.as_deref() != Some(challenge.as_slice()) {
        return refuse("device signed a different challenge than this session asked for");
    }
    if uid.len() != UID_LEN {
        return refuse(format!(
            "unit id is {} bytes, the list expects {UID_LEN}",
            uid.len()
        ));
    }
    if signature.len() != SIG_SIZE {
        return refuse(format!(
            "signature is {} bytes, expected {SIG_SIZE}",
            signature.len()
        ));
    }

    let sig = Signature::from_slice(&signature)
        .map_err(|err| UnitsError::Refusal(format!("signature could not be read: {err}")))?;
    // A high-s signature says the same thing; the row digest is built from one spelling of it.
    let sig = sig.normalize_s().unwrap_or(sig);
    let digest = Sha256::digest([uid.as_slice(), challenge.as_slice()].concat());

    // RecIds 2 and 3 need x above the curve order, about 2^-128 of them: both parities is the set.
    for id in 0..2 {
        let Ok(key) = recover(&sig, &digest, id) else {
            continue;
        };
        let point = key.to_encoded_point(true);
        if let Some(row) = list.find(&row_hash(list.salt(), &uid, point.as_bytes())) {
            return Ok(row);
        }
    }

    Err(UnitsError::NotRegistered { uid: hex(&uid) })
}

/// A 32-byte challenge for one verify session.
pub fn new_challenge() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    // A challenge of zeros would verify a device that never signed anything, so this cannot continue.
    getrandom::fill(&mut bytes).expect("the operating system offered no random bytes");
    bytes
}

fn unhex(text: &str) -> Result<Vec<u8>, UnitsError> {
    let out = (0..text.len() / 2)
        .map(|at| {
            u8::from_str_radix(&text[at * 2..at * 2 + 2], 16)
                .map_err(|_| UnitsError::Refusal("expected lowercase hex".into()))
        })
        .collect::<Result<Vec<u8>, UnitsError>>()?;
    if out.len() * 2 != text.len() {
        return refuse("expected lowercase hex of even length");
    }
    Ok(out)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_the_two_field_map() {
        let encoded = encode_init(&[0x07; 32]);
        assert_eq!(hex(&encoded), format!("a20101065820{}", "07".repeat(32)));
    }
}
