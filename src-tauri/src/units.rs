use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const LIST_ASSET_NAME: &str = "shell-verification-db.bin";

pub const UNITS_VERIFY_KEY: &str =
    "032ac68f2b2a500ebf1bc31f0c36e4645b24530ce6d1460e92c2d94e9a47ba550e";

const MAGIC: &[u8; 4] = b"SHVR";
const LIST_FORMAT_VERSION: u16 = 1;
const HEADER_SIZE: usize = 64;
const LIST_SIGNATURE_SIZE: usize = 64;
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

const FIELD_STEP: usize = 1;
const FIELD_UID: usize = 2;
const FIELD_CHALLENGE: usize = 6;
const FIELD_SIGNATURE: usize = 7;
const ANSWER_FIELDS: [usize; 4] = [FIELD_STEP, FIELD_UID, FIELD_CHALLENGE, FIELD_SIGNATURE];
const TAG_POSITIVE_BIGNUM: usize = 37;
const STEP_ANSWERED: usize = 2;

#[derive(Debug, Error)]
pub enum UnitsError {
    #[error("{0}")]
    Refusal(String),
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
        &self.bytes[HEADER_SIZE..self.bytes.len() - LIST_SIGNATURE_SIZE]
    }

    fn row_holding(&self, hash: &[u8]) -> Option<usize> {
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

fn signs(sig: &Signature, digest: &[u8], key: &VerifyingKey) -> bool {
    let halves = [Some(*sig), sig.normalize_s()];
    halves.iter().flatten().any(|candidate| {
        (0..2)
            .filter_map(|id| recover(candidate, digest, id).ok())
            .any(|recovered| recovered == *key)
    })
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

pub fn load_list(bytes: &[u8]) -> Result<UnitList, UnitsError> {
    load_list_with_key(bytes, UNITS_VERIFY_KEY)
}

pub fn load_list_with_key(bytes: &[u8], verify_key: &str) -> Result<UnitList, UnitsError> {
    if bytes.len() < HEADER_SIZE + LIST_SIGNATURE_SIZE {
        return refuse("file too short to be a units list");
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return refuse("not a units list");
    }
    if u16le(bytes, FORMAT_OFFSET) != LIST_FORMAT_VERSION {
        return refuse(format!(
            "units list format is not version {LIST_FORMAT_VERSION}"
        ));
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
    let expected = HEADER_SIZE + row_count * ROW_LEN + LIST_SIGNATURE_SIZE;
    if bytes.len() != expected {
        return refuse(format!(
            "units list is {} bytes, its header describes {expected}",
            bytes.len()
        ));
    }

    let (prefix, signature) = bytes.split_at(bytes.len() - LIST_SIGNATURE_SIZE);
    let key = VerifyingKey::from_sec1_bytes(&unhex(verify_key)?)
        .map_err(|err| UnitsError::Refusal(format!("pinned key could not be read: {err}")))?;
    let sig = Signature::from_slice(signature)
        .map_err(|err| UnitsError::Refusal(format!("signature could not be read: {err}")))?;
    let digest = Sha256::digest(prefix);
    if !signs(&sig, &digest, &key) {
        return refuse("signature does not match the pinned key");
    }

    let rows = &bytes[HEADER_SIZE..bytes.len() - LIST_SIGNATURE_SIZE];
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

    fn dev_auth(&mut self) -> Result<(), UnitsError> {
        let count = self.head(5)?;
        for _ in 0..count {
            let key = self.head(0)?;
            if self.keys.contains(&key) {
                return refuse(format!("device answer repeats field {key}"));
            }
            self.keys.push(key);
            match key {
                FIELD_STEP => self.step = Some(self.head(0)?),
                FIELD_UID => {
                    if self.bytes.get(self.at).map(|head| head >> 5) != Some(6) {
                        return refuse(format!(
                            "the unit id is not a tagged value, expected tag {TAG_POSITIVE_BIGNUM}"
                        ));
                    }
                    let tag = self.head(6)?;
                    if tag != TAG_POSITIVE_BIGNUM {
                        return refuse(format!(
                            "device answer tags its unit id {tag}, expected {TAG_POSITIVE_BIGNUM} (a positive bignum)"
                        ));
                    }
                    self.uid = Some(self.byte_string()?);
                }
                3..=5 => {
                    self.head(0)?;
                }
                FIELD_CHALLENGE => self.challenge = Some(self.byte_string()?),
                FIELD_SIGNATURE => self.signature = Some(self.byte_string()?),
                other => return refuse(format!("device answer has an unexpected field {other}")),
            }
        }
        if self.at != self.bytes.len() {
            return refuse("device answer has bytes after its last field");
        }
        Ok(())
    }
}

pub fn encode_init(challenge: &[u8; 32]) -> Vec<u8> {
    let mut out = vec![0xa2, 0x01, 0x01, 0x06, 0x58, 0x20];
    out.extend_from_slice(challenge);
    out
}

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
        Some(STEP_ANSWERED) => {}
        Some(other) => {
            return refuse(format!(
                "device answered step {other}, expected {STEP_ANSWERED}"
            ))
        }
        None => return refuse("device answer has no step"),
    }
    let uid = answer
        .uid
        .ok_or_else(|| UnitsError::Refusal("no unit id".into()))?;
    let signature = answer
        .signature
        .ok_or_else(|| UnitsError::Refusal("no signature".into()))?;
    // Field 6 of the answer is device randomness and is checked against nothing (shell-units
    // docs/format.md). The answer is bound to this session by sha256(uid || challenge): an answer
    // made for another challenge recovers a point no row holds.
    if uid.len() != UID_LEN {
        return refuse(format!(
            "unit id is {} bytes, the list expects {UID_LEN}",
            uid.len()
        ));
    }
    if signature.len() != LIST_SIGNATURE_SIZE {
        return refuse(format!(
            "signature is {} bytes, expected {LIST_SIGNATURE_SIZE}",
            signature.len()
        ));
    }

    let sig = Signature::from_slice(&signature)
        .map_err(|err| UnitsError::Refusal(format!("signature could not be read: {err}")))?;
    let sig = sig.normalize_s().unwrap_or(sig);
    let digest = Sha256::digest([uid.as_slice(), challenge.as_slice()].concat());

    let mut recovered_any = false;
    for id in 0..2 {
        let Ok(key) = recover(&sig, &digest, id) else {
            continue;
        };
        recovered_any = true;
        let point = key.to_encoded_point(true);
        if let Some(row) = list.row_holding(&row_hash(list.salt(), &uid, point.as_bytes())) {
            return Ok(row);
        }
    }
    if !recovered_any {
        return refuse("the signature recovers no public key");
    }

    Err(UnitsError::NotRegistered { uid: hex(&uid) })
}

pub fn new_challenge() -> [u8; 32] {
    let mut bytes = [0u8; 32];
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
    crate::image::hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_the_two_field_map() {
        let encoded = encode_init(&[0x07; 32]);
        assert_eq!(hex(&encoded), format!("a20101065820{}", "07".repeat(32)));
    }

    fn published_list() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("src-tauri is under the repository")
                .join("tests/fixtures/units-latest.bin"),
        )
        .expect("run `cargo run --example units` to write tests/fixtures/units-latest.bin")
    }

    #[test]
    fn a_list_signed_on_the_other_half_of_the_curve_still_loads() {
        use k256::elliptic_curve::PrimeField;
        let mut bytes = published_list();
        let at = bytes.len() - LIST_SIGNATURE_SIZE;
        let signature: [u8; LIST_SIGNATURE_SIZE] = bytes[at..].try_into().unwrap();
        let half: [u8; 32] = signature[32..].try_into().unwrap();
        let mut flipped = signature;
        flipped[32..].copy_from_slice(
            &k256::Scalar::from_repr(half.into())
                .unwrap()
                .negate()
                .to_bytes(),
        );
        assert_ne!(flipped, signature, "S has to have moved");
        bytes[at..].copy_from_slice(&flipped);
        assert!(
            load_list(&bytes).is_ok(),
            "the same list with the other S is the same signature"
        );
    }

    #[test]
    fn a_list_signed_by_nobody_is_refused() {
        let mut bytes = published_list();
        bytes[0] ^= 0xff;
        assert!(load_list(&bytes).is_err());
    }
}
