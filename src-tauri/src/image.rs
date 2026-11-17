use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Firmware,
    Database,
}

impl Kind {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "firmware" => Ok(Self::Firmware),
            "database" => Ok(Self::Database),
            other => Err(format!("unknown image kind {other}")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Firmware => "firmware",
            Self::Database => "database",
        }
    }

    pub fn published_hash(self, image: &[u8]) -> Result<String, ImageError> {
        match self {
            Self::Firmware => firmware_hash(image),
            Self::Database => database_hash(image),
        }
    }

    pub fn inspect(self, image: &[u8]) -> Result<ImageInfo, String> {
        match self {
            Self::Firmware => inspect_firmware(image),
            Self::Database => inspect_database(image),
        }
        .map_err(|e| e.to_string())
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(self.name())
    }
}

pub const MCU_PAGE_SIZE: usize = 8192;
pub const FIRMWARE_PAGE_COUNT: usize = 76;
pub const FIRMWARE_PARTITION_SIZE: usize = MCU_PAGE_SIZE * FIRMWARE_PAGE_COUNT;
pub const FIRMWARE_HASHED_PREFIX_SIZE: usize = 588;
pub const SIGNATURE_SIZE: usize = 64;
pub const FIRMWARE_VERSION_OFFSET: usize = 652;
pub const FIRMWARE_BTC_ONLY_BIT: u8 = 0x20;
pub const DATABASE_MAGIC: u16 = 0x4532;
pub const EMPTY_FLASH_BYTE: u8 = 0xff;

#[derive(Debug, Error)]
pub enum ImageError {
    #[error("too small for a firmware image: {0} bytes")]
    FirmwareTooSmall(usize),
    #[error("larger than the firmware partition: {0} bytes, max {FIRMWARE_PARTITION_SIZE}")]
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
    pub version: String,
    pub kind: &'static str,
    pub fw_variant: Option<u8>,
    pub bytes: usize,
    pub sha256: String,
    pub matches_release: Option<String>,
}

pub fn firmware_hash(image: &[u8]) -> Result<String, ImageError> {
    if image.len() < FIRMWARE_VERSION_OFFSET + 3 {
        return Err(ImageError::FirmwareTooSmall(image.len()));
    }
    if image.len() > FIRMWARE_PARTITION_SIZE {
        return Err(ImageError::FirmwareTooLarge(image.len()));
    }
    let mut hasher = Sha256::new();
    hasher.update(&image[..FIRMWARE_HASHED_PREFIX_SIZE]);
    hasher.update(&image[FIRMWARE_HASHED_PREFIX_SIZE + SIGNATURE_SIZE..]);
    for _ in image.len()..FIRMWARE_PARTITION_SIZE {
        hasher.update([EMPTY_FLASH_BYTE]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn database_hash(image: &[u8]) -> Result<String, ImageError> {
    if image.len() <= SIGNATURE_SIZE {
        return Err(ImageError::DatabaseTooSmall(image.len()));
    }
    let mut hasher = Sha256::new();
    hasher.update(&image[..image.len() - SIGNATURE_SIZE]);
    Ok(hex(&hasher.finalize()))
}

fn is_a_release_version(major: u8, minor: u8, patch: u8) -> bool {
    major != 0 && major < EMPTY_FLASH_BYTE && minor < EMPTY_FLASH_BYTE && patch < EMPTY_FLASH_BYTE
}

fn version_looks_like_a_date(version: u32) -> bool {
    (20_000_000..40_000_000).contains(&version)
}

pub fn inspect_firmware(image: &[u8]) -> Result<ImageInfo, ImageError> {
    let sha256 = firmware_hash(image)?;
    let (major, minor, patch) = (
        image[FIRMWARE_VERSION_OFFSET],
        image[FIRMWARE_VERSION_OFFSET + 1],
        image[FIRMWARE_VERSION_OFFSET + 2],
    );
    if !is_a_release_version(major, minor, patch) {
        return Err(ImageError::FirmwareVersion {
            major,
            minor,
            patch,
        });
    }
    let btc_only = image[FIRMWARE_VERSION_OFFSET + 3] & FIRMWARE_BTC_ONLY_BIT != 0;
    Ok(ImageInfo {
        version: format!("{major}.{minor}.{patch}"),
        kind: "firmware",
        fw_variant: Some(if btc_only { 1 } else { 0 }),
        bytes: image.len(),
        sha256,
        matches_release: None,
    })
}

pub fn inspect_database(image: &[u8]) -> Result<ImageInfo, ImageError> {
    if image.len() < 8 + SIGNATURE_SIZE {
        return Err(ImageError::DatabaseTooSmall(image.len()));
    }
    if u16::from_le_bytes([image[0], image[1]]) != DATABASE_MAGIC {
        return Err(ImageError::DatabaseMagic);
    }
    let version = u32::from_le_bytes([image[4], image[5], image[6], image[7]]);
    if !version_looks_like_a_date(version) {
        return Err(ImageError::DatabaseVersion(version));
    }
    Ok(ImageInfo {
        version: version.to_string(),
        kind: "database",
        fw_variant: None,
        bytes: image.len(),
        sha256: database_hash(image)?,
        matches_release: None,
    })
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filler(len: usize) -> Vec<u8> {
        vec![0xff; len]
    }

    #[test]
    fn firmware_hash_skips_the_signature_and_pads_the_image() {
        let mut image = filler(600);
        assert!(matches!(
            firmware_hash(&image),
            Err(ImageError::FirmwareTooSmall(600))
        ));
        image = filler(FIRMWARE_VERSION_OFFSET + 3);
        assert!(firmware_hash(&image).is_ok());
        image = filler(FIRMWARE_PARTITION_SIZE);
        assert!(firmware_hash(&image).is_ok());
        image.push(0);
        assert!(matches!(
            firmware_hash(&image),
            Err(ImageError::FirmwareTooLarge(_))
        ));
    }

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
        let base = filler(FIRMWARE_VERSION_OFFSET + 4);

        let mut trailing = base.clone();
        trailing.extend([0xff, 0xff, 0xff]);
        assert_eq!(
            firmware_hash(&base).unwrap(),
            firmware_hash(&trailing).unwrap()
        );

        let mut in_signature = base.clone();
        in_signature[FIRMWARE_HASHED_PREFIX_SIZE] ^= 0xff;
        assert_eq!(
            firmware_hash(&base).unwrap(),
            firmware_hash(&in_signature).unwrap()
        );

        let mut outside = base.clone();
        outside[FIRMWARE_HASHED_PREFIX_SIZE - 1] ^= 0xff;
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
            database_hash(&[0u8; SIGNATURE_SIZE]),
            Err(ImageError::DatabaseTooSmall(64))
        ));
    }

    #[test]
    fn a_real_database_header_is_accepted() {
        let mut image = vec![0u8; 8 + SIGNATURE_SIZE];
        image[0..2].copy_from_slice(&DATABASE_MAGIC.to_le_bytes());
        image[4..8].copy_from_slice(&20261111u32.to_le_bytes());
        let info = inspect_database(&image).unwrap();
        assert_eq!(info.version, "20261111");
        assert_eq!(info.kind, "database");
        assert_eq!(info.fw_variant, None);
    }

    #[test]
    fn anything_without_the_database_magic_is_refused() {
        let mut image = vec![0u8; 8 + SIGNATURE_SIZE];
        image[4..8].copy_from_slice(&20261111u32.to_le_bytes());
        assert!(matches!(
            inspect_database(&image),
            Err(ImageError::DatabaseMagic)
        ));
    }

    #[test]
    fn an_ordinary_binary_is_not_a_database() {
        let mut image = b"PK\x03\x04not really a db image".to_vec();
        image.resize(8 + SIGNATURE_SIZE, 0);
        assert!(matches!(
            inspect_database(&image),
            Err(ImageError::DatabaseMagic)
        ));
    }

    #[test]
    fn database_version_must_look_like_a_date() {
        let mut image = vec![0u8; 8 + SIGNATURE_SIZE];
        image[0..2].copy_from_slice(&DATABASE_MAGIC.to_le_bytes());
        image[4..8].copy_from_slice(&12345u32.to_le_bytes());
        assert!(matches!(
            inspect_database(&image),
            Err(ImageError::DatabaseVersion(12345))
        ));
    }

    fn firmware_with(version: [u8; 3], flags: u8) -> Vec<u8> {
        let mut image = filler(FIRMWARE_VERSION_OFFSET + 4);
        image[FIRMWARE_VERSION_OFFSET..FIRMWARE_VERSION_OFFSET + 3].copy_from_slice(&version);
        image[FIRMWARE_VERSION_OFFSET + 3] = flags;
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
        assert_eq!(
            inspect_firmware(&firmware_with([1, 5, 3], 0x00))
                .unwrap()
                .fw_variant,
            Some(0)
        );
        assert_eq!(
            inspect_firmware(&firmware_with([1, 5, 3], FIRMWARE_BTC_ONLY_BIT))
                .unwrap()
                .fw_variant,
            Some(1)
        );
    }

    #[test]
    fn versions_that_are_not_releases_are_refused() {
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
