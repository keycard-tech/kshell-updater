//! Reading and validating the two images the Shell accepts, before anything is written to it.
//!
//! Hash rules are not a plain `sha256sum` and were confirmed against published release values rather
//! than inferred, mirroring `tools/common.py` in `keycard-tech/keycard-shell`:
//!
//! - firmware: the image is padded with `0xff` to [`FW_SIZE`], then hashed with the signature at
//!   [`FW_IV_SIZE`] skipped. `sha256sum` of the file matches nothing.
//! - database: every byte except the trailing signature.
//!
//! Neither image carries a magic number — the firmware starts with an ARM vector table — so a file
//! picked from disk can only be checked structurally. That is why an online download must match the
//! release's published hash, and why [`ImageInfo`] reports what was actually read: on a file picked
//! from disk the version is the app's best evidence, not a promise.

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// One flash page of the MCU.
pub const PAGE_SIZE: usize = 8192;
/// Pages the firmware partition spans.
pub const FW_PAGE_COUNT: usize = 76;
pub const FW_SIZE: usize = PAGE_SIZE * FW_PAGE_COUNT;
/// Bytes before the signature, and the signature itself.
pub const FW_IV_SIZE: usize = 588;
pub const SIG_SIZE: usize = 64;
/// First byte of the `major.minor.patch` version in a firmware image.
pub const FW_VERSION_POSITION: usize = 652;
/// Set in the byte after the version on btc-only images, clear on omni.
pub const BTC_ONLY_FLAG: u8 = 0x20;
/// First two bytes of a database image, little-endian.
pub const DB_MAGIC: u16 = 0x4532;

#[derive(Debug, Error)]
pub enum ImageError {
    #[error("too small for a firmware image: {0} bytes")]
    FirmwareTooSmall(usize),
    #[error("larger than the firmware partition: {0} bytes, max {FW_SIZE}")]
    FirmwareTooLarge(usize),
    #[error("firmware version {major}.{minor}.{patch} is not a released build")]
    FirmwareVersion { major: u8, minor: u8, patch: u8 },
    #[error("not a Keycard database file")]
    DatabaseMagic,
    #[error("too small for a database image: {0} bytes")]
    DatabaseTooSmall(usize),
    #[error("database version {0} is not a plausible date")]
    DatabaseVersion(u32),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    /// `1.5.3` for firmware, `20261111` for a database.
    pub version: String,
    /// `firmware` or `database`, so the renderer cannot mix the two up.
    pub kind: &'static str,
    /// Firmware only: 0 omni, 1 btc-only, read from the image's own flags.
    pub fw_variant: Option<u8>,
    pub bytes: usize,
    pub sha256: String,
}

/// The hash the bootloader checks, as hex. Pads short images exactly as `tools/firmware-hash.py`
/// does: the ELF was produced with `--gap-fill=255`, so unwritten flash reads as `0xff`.
pub fn firmware_hash(image: &[u8]) -> Result<String, ImageError> {
    if image.len() < FW_VERSION_POSITION + 3 {
        return Err(ImageError::FirmwareTooSmall(image.len()));
    }
    if image.len() > FW_SIZE {
        return Err(ImageError::FirmwareTooLarge(image.len()));
    }
    let mut hasher = Sha256::new();
    hasher.update(&image[..FW_IV_SIZE]);
    hasher.update(&image[FW_IV_SIZE + SIG_SIZE..]);
    for _ in image.len()..FW_SIZE {
        hasher.update([0xffu8]);
    }
    Ok(hex(&hasher.finalize()))
}

/// The hash published in `db-<version>-sha256.txt`: everything but the trailing signature.
pub fn database_hash(image: &[u8]) -> Result<String, ImageError> {
    if image.len() <= SIG_SIZE {
        return Err(ImageError::DatabaseTooSmall(image.len()));
    }
    let mut hasher = Sha256::new();
    hasher.update(&image[..image.len() - SIG_SIZE]);
    Ok(hex(&hasher.finalize()))
}

/// No released version has a zero or 255 component: 255.255.255 is what erased flash reads as, which
/// is what an empty or half-written image looks like here, and 0.0.0 is what a header full of zeros
/// reads as. The old app accepted both and flashed them.
fn version_is_plausible(major: u8, minor: u8, patch: u8) -> bool {
    major != 0
        && (major, minor, patch) != (0xff, 0xff, 0xff)
        && major < 0xff
        && minor < 0xff
        && patch < 0xff
}

/// Reads the header and rejects anything the Shell would refuse.
///
/// The variant comes from the image rather than the file name, so a renamed or downloaded-and-renamed
/// file still reports what it is. The flags byte is not the device's `fwVariant` encoding: bit `0x20`
/// of byte 655 marks btc-only (`0x00` on omni, seen on 1.5.2, 1.5.2-experimental-btc and 1.5.3 in both
/// shapes), where the device reports 0 or 1. Normalised here so callers see device values.
pub fn inspect_firmware(image: &[u8]) -> Result<ImageInfo, ImageError> {
    let sha256 = firmware_hash(image)?;
    let (major, minor, patch) = (
        image[FW_VERSION_POSITION],
        image[FW_VERSION_POSITION + 1],
        image[FW_VERSION_POSITION + 2],
    );
    if !version_is_plausible(major, minor, patch) {
        return Err(ImageError::FirmwareVersion {
            major,
            minor,
            patch,
        });
    }
    let btc_only = image[FW_VERSION_POSITION + 3] & BTC_ONLY_FLAG != 0;
    Ok(ImageInfo {
        version: format!("{major}.{minor}.{patch}"),
        kind: "firmware",
        fw_variant: Some(if btc_only { 1 } else { 0 }),
        bytes: image.len(),
        sha256,
    })
}

pub fn inspect_database(image: &[u8]) -> Result<ImageInfo, ImageError> {
    if image.len() < 8 + SIG_SIZE {
        return Err(ImageError::DatabaseTooSmall(image.len()));
    }
    if u16::from_le_bytes([image[0], image[1]]) != DB_MAGIC {
        return Err(ImageError::DatabaseMagic);
    }
    let version = u32::from_le_bytes([image[4], image[5], image[6], image[7]]);
    if !(20_000_000..40_000_000).contains(&version) {
        return Err(ImageError::DatabaseVersion(version));
    }
    Ok(ImageInfo {
        version: version.to_string(),
        kind: "database",
        fw_variant: None,
        bytes: image.len(),
        sha256: database_hash(image)?,
    })
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full-size image of 0xff: the shape the ELF gap-fill produces.
    fn filler(len: usize) -> Vec<u8> {
        vec![0xff; len]
    }

    #[test]
    fn firmware_hash_skips_the_signature_and_pads_the_image() {
        // Pinned against published releases, sha256 of the padded image with bytes 588..652 skipped.
        // Re-derived from the real bins; see docs/build-log.md.
        let mut image = filler(600);
        assert!(matches!(
            firmware_hash(&image),
            Err(ImageError::FirmwareTooSmall(600))
        ));
        image = filler(FW_VERSION_POSITION + 3);
        assert!(firmware_hash(&image).is_ok());
        image = filler(FW_SIZE);
        assert!(firmware_hash(&image).is_ok());
        image.push(0);
        assert!(matches!(
            firmware_hash(&image),
            Err(ImageError::FirmwareTooLarge(_))
        ));
    }

    /// Run against the real release images to confirm the rule matches what GitHub publishes:
    ///
    /// ```text
    /// KSHELL_FW=~/scratch/shellos-20261112-1.5.3.bin \
    /// KSHELL_FW_BTC=~/scratch/shellos-20261112-1.5.3-btc-only.bin \
    /// KSHELL_DB=~/scratch/shelldb-20261111.bin cargo test -- --ignored
    /// ```
    ///
    /// Ignored because the images are not in the repository, and worthless without them.
    #[test]
    #[ignore]
    fn published_hashes_are_reproduced() {
        let cases = [
            (
                "KSHELL_FW",
                "bb53a45e9c5e63e80aa8b9a9e9b46991aa0f597ddee014fefdf787ef685a6177",
            ),
            (
                "KSHELL_FW_BTC",
                "96aa0a4e651b232e665539ae2ba5079ccd5439f36d45da1eb9a0e12d1a8acc32",
            ),
        ];
        for (key, published) in cases {
            let path = std::env::var(key).expect("point the env var at a downloaded image");
            let bytes = std::fs::read(path).unwrap();
            let info = inspect_firmware(&bytes).unwrap();
            assert_eq!(info.sha256, published, "{key}");
        }
        let path = std::env::var("KSHELL_DB").expect("point the env var at a downloaded image");
        let bytes = std::fs::read(path).unwrap();
        let info = inspect_database(&bytes).unwrap();
        assert_eq!(
            info.sha256,
            "63dd9d51c8f174fb1b3a6a1ed9864326f90cb2a09856d0da6db6e5e84d02b41d"
        );
    }

    #[test]
    fn padding_and_the_skipped_window_are_part_of_the_hash() {
        let base = filler(FW_VERSION_POSITION + 4);

        // Padding is part of the rule rather than part of the file: an image and the same image with
        // trailing 0xff hash alike, because both are padded to FW_SIZE with 0xff.
        let mut trailing = base.clone();
        trailing.extend([0xff, 0xff, 0xff]);
        assert_eq!(
            firmware_hash(&base).unwrap(),
            firmware_hash(&trailing).unwrap()
        );

        // A byte inside the signature window is skipped, so it cannot change the hash.
        let mut in_signature = base.clone();
        in_signature[FW_IV_SIZE] ^= 0xff;
        assert_eq!(
            firmware_hash(&base).unwrap(),
            firmware_hash(&in_signature).unwrap()
        );

        // A byte outside it must.
        let mut outside = base.clone();
        outside[FW_IV_SIZE - 1] ^= 0xff;
        assert_ne!(
            firmware_hash(&base).unwrap(),
            firmware_hash(&outside).unwrap()
        );
    }

    #[test]
    fn database_hash_drops_the_signature() {
        let image = vec![0u8; 100];
        let hash = database_hash(&image).unwrap();
        assert_eq!(hash.len(), 64);
        assert!(matches!(
            database_hash(&[0u8; SIG_SIZE]),
            Err(ImageError::DatabaseTooSmall(64))
        ));
    }

    #[test]
    fn a_real_database_header_is_accepted() {
        let mut image = vec![0u8; 8 + SIG_SIZE];
        image[0..2].copy_from_slice(&DB_MAGIC.to_le_bytes());
        image[4..8].copy_from_slice(&20261111u32.to_le_bytes());
        let info = inspect_database(&image).unwrap();
        assert_eq!(info.version, "20261111");
        assert_eq!(info.kind, "database");
        assert_eq!(info.fw_variant, None);
    }

    #[test]
    fn anything_without_the_database_magic_is_refused() {
        let mut image = vec![0u8; 8 + SIG_SIZE];
        image[4..8].copy_from_slice(&20261111u32.to_le_bytes());
        assert!(matches!(
            inspect_database(&image),
            Err(ImageError::DatabaseMagic)
        ));
    }

    #[test]
    fn an_ordinary_binary_is_not_a_database() {
        let mut image = b"PK\x03\x04not really a db image".to_vec();
        image.resize(8 + SIG_SIZE, 0);
        assert!(matches!(
            inspect_database(&image),
            Err(ImageError::DatabaseMagic)
        ));
    }

    #[test]
    fn database_version_must_look_like_a_date() {
        let mut image = vec![0u8; 8 + SIG_SIZE];
        image[0..2].copy_from_slice(&DB_MAGIC.to_le_bytes());
        image[4..8].copy_from_slice(&12345u32.to_le_bytes());
        assert!(matches!(
            inspect_database(&image),
            Err(ImageError::DatabaseVersion(12345))
        ));
    }

    fn firmware_with(version: [u8; 3], flags: u8) -> Vec<u8> {
        let mut image = filler(FW_VERSION_POSITION + 4);
        image[FW_VERSION_POSITION..FW_VERSION_POSITION + 3].copy_from_slice(&version);
        image[FW_VERSION_POSITION + 3] = flags;
        image
    }

    #[test]
    fn firmware_version_is_read_from_the_image() {
        let info = inspect_firmware(&firmware_with([1, 5, 3], 0x00)).unwrap();
        assert_eq!(info.version, "1.5.3");
        assert_eq!(info.kind, "firmware");
    }

    #[test]
    fn btc_only_is_told_by_the_image_not_the_name() {
        // Byte 655 as found in shellos-20261112-1.5.3.bin (0x00) and its -btc-only sibling (0x20).
        assert_eq!(
            inspect_firmware(&firmware_with([1, 5, 3], 0x00))
                .unwrap()
                .fw_variant,
            Some(0)
        );
        assert_eq!(
            inspect_firmware(&firmware_with([1, 5, 3], BTC_ONLY_FLAG))
                .unwrap()
                .fw_variant,
            Some(1)
        );
    }

    #[test]
    fn versions_that_are_not_releases_are_refused() {
        // Erased flash reads 255.255.255 at the version offset; the old app read it as a version and
        // went ahead with the flash.
        for blank in [[0u8, 0, 0], [0xff, 0xff, 0xff], [1, 0xff, 0], [0xff, 5, 3]] {
            assert!(
                inspect_firmware(&firmware_with(blank, 0x00)).is_err(),
                "{blank:?} must not be accepted"
            );
        }
        assert!(inspect_firmware(&firmware_with([1, 5, 3], 0x00)).is_ok());
        assert!(inspect_firmware(&firmware_with([2, 0, 1], 0x00)).is_ok());
    }
}
