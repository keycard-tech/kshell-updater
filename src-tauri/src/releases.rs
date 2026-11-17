use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const FIRMWARE_ASSET_PREFIX: &str = "shellos-";
pub const DATABASE_ASSET_PREFIX: &str = "shelldb-";
pub const BTC_ONLY_SUFFIX: &str = "-btc-only";
pub const HASH_LIST_SUFFIX: &str = "-sha256.txt";
pub const BIN_SUFFIX: &str = ".bin";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Variant {
    Omni,
    BtcOnly,
}

impl Serialize for Variant {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(u8::from(*self))
    }
}

impl From<Variant> for u8 {
    fn from(variant: Variant) -> u8 {
        match variant {
            Variant::Omni => 0,
            Variant::BtcOnly => 1,
        }
    }
}

impl Variant {
    pub fn from_fw_variant(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Omni),
            1 => Some(Self::BtcOnly),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Omni => "omni",
            Self::BtcOnly => "btc-only",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseFile {
    pub name: String,
    pub url: String,
    pub date: String,
    pub version: Option<String>,
    pub variant: Option<Variant>,
    pub sha256: Option<String>,
}

fn is_a_date(value: &str) -> bool {
    !value.is_empty() && value.len() == 8 && value.bytes().all(|b| b.is_ascii_digit())
}

fn is_a_dotted_version(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

pub fn parse_firmware_asset(name: &str, url: &str) -> Option<ReleaseFile> {
    let stem = name.strip_suffix(BIN_SUFFIX)?;
    let rest = stem.strip_prefix(FIRMWARE_ASSET_PREFIX)?;
    let (date, tail) = rest.split_once('-')?;
    if !is_a_date(date) {
        return None;
    }
    let (version, variant) = match tail.strip_suffix(BTC_ONLY_SUFFIX) {
        Some(version) => (version, Variant::BtcOnly),
        None => (tail, Variant::Omni),
    };
    if !is_a_dotted_version(version) {
        return None;
    }
    Some(ReleaseFile {
        name: name.to_string(),
        url: url.to_string(),
        date: date.to_string(),
        version: Some(version.to_string()),
        variant: Some(variant),
        sha256: None,
    })
}

pub fn parse_database_asset(name: &str, url: &str) -> Option<ReleaseFile> {
    let stem = name.strip_suffix(BIN_SUFFIX)?;
    let date = stem.strip_prefix(DATABASE_ASSET_PREFIX)?;
    if !is_a_date(date) {
        return None;
    }
    Some(ReleaseFile {
        name: name.to_string(),
        url: url.to_string(),
        date: date.to_string(),
        version: None,
        variant: None,
        sha256: None,
    })
}

pub fn parse_sha256_index(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            if hash.len() != 64 {
                return None;
            }
            if !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            Some((name.trim().to_string(), hash.to_ascii_lowercase()))
        })
        .collect()
}

pub fn hash_list_name(name: &str) -> String {
    if let Some(stem) = name
        .strip_prefix(FIRMWARE_ASSET_PREFIX)
        .and_then(|n| n.strip_suffix(&format!("{BTC_ONLY_SUFFIX}{BIN_SUFFIX}")))
    {
        return format!("shellos-btc-{stem}{BIN_SUFFIX}");
    }
    if name.starts_with(DATABASE_ASSET_PREFIX) && name.ends_with(BIN_SUFFIX) {
        return "db.bin".to_string();
    }
    name.to_string()
}

pub fn attach_sha256(files: &mut [ReleaseFile], index: &HashMap<String, String>) {
    for file in files {
        file.sha256 = lookup_sha256(index, &file.name);
    }
}

pub fn newest_date(files: &[ReleaseFile]) -> Option<String> {
    files.iter().map(|f| f.date.clone()).max()
}

pub fn latest(files: &[ReleaseFile]) -> Vec<ReleaseFile> {
    let Some(newest) = newest_date(files) else {
        return Vec::new();
    };
    files.iter().filter(|f| f.date == newest).cloned().collect()
}

pub struct Offered {
    pub files: Vec<ReleaseFile>,
    pub dropped_without_hash: usize,
}

pub fn offerable(files: Vec<ReleaseFile>) -> Offered {
    let (offered, dropped): (Vec<_>, Vec<_>) =
        files.into_iter().partition(|file| file.sha256.is_some());
    Offered {
        files: offered,
        dropped_without_hash: dropped.len(),
    }
}

pub fn lookup_sha256(index: &HashMap<String, String>, asset: &str) -> Option<String> {
    index
        .get(&hash_list_name(asset))
        .or_else(|| index.get(asset))
        .cloned()
}

pub fn release_files(json: &serde_json::Value, kind: &str) -> Vec<ReleaseFile> {
    let mut files: Vec<ReleaseFile> = json
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|release| release["assets"].as_array().into_iter().flatten())
        .filter_map(|asset| {
            let name = asset["name"].as_str()?;
            let url = asset["browser_download_url"].as_str()?;
            match kind {
                "firmware" => parse_firmware_asset(name, url),
                "database" => parse_database_asset(name, url),
                _ => None,
            }
        })
        .collect();
    files.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| a.variant.map(|v| v as u8).cmp(&b.variant.map(|v| v as u8)))
            .then_with(|| b.version.cmp(&a.version))
    });
    files
}

pub fn sha256_urls(json: &serde_json::Value) -> Vec<String> {
    json.as_array()
        .into_iter()
        .flatten()
        .flat_map(|release| release["assets"].as_array().into_iter().flatten())
        .filter(|asset| {
            asset["name"]
                .as_str()
                .is_some_and(|name| name.ends_with(HASH_LIST_SUFFIX))
        })
        .filter_map(|asset| asset["browser_download_url"].as_str())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHELL_ASSETS: [(&str, bool); 8] = [
        ("shellos-20261112-1.5.3.bin", true),
        ("shellos-20261112-1.5.3-btc-only.bin", true),
        ("shellos-bootloader-20261112-1.5.3.bin", false),
        ("shellos-v1.5.3-sha256.txt", false),
        ("shellos-20261031-1.5.2.bin", true),
        ("shellos-experimental-20261031-1.5.2-btc-only.bin", false),
        ("shellos-v1.5.2-sha256.txt", false),
        ("README.md", false),
    ];

    #[test]
    fn only_omni_and_btc_only_images_are_offered() {
        let accepted: Vec<&str> = SHELL_ASSETS
            .iter()
            .filter(|(name, _)| parse_firmware_asset(name, "u").is_some())
            .map(|(name, _)| *name)
            .collect();
        let expected: Vec<&str> = SHELL_ASSETS
            .iter()
            .filter(|(_, keep)| *keep)
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(accepted, expected);
    }

    #[test]
    fn variants_are_read_from_the_name() {
        let omni = parse_firmware_asset("shellos-20261112-1.5.3.bin", "u").unwrap();
        assert_eq!(omni.variant, Some(Variant::Omni));
        assert_eq!(omni.version.as_deref(), Some("1.5.3"));
        assert_eq!(omni.date, "20261112");

        let btc = parse_firmware_asset("shellos-20261112-1.5.3-btc-only.bin", "u").unwrap();
        assert_eq!(btc.variant, Some(Variant::BtcOnly));
        assert_eq!(btc.version.as_deref(), Some("1.5.3"));
    }

    #[test]
    fn experimental_and_bootloader_names_are_rejected() {
        assert!(
            parse_firmware_asset("shellos-experimental-20261031-1.5.2-btc-only.bin", "u").is_none()
        );
        assert!(parse_firmware_asset("shellos-bootloader-20261112-1.5.3.bin", "u").is_none());
        assert!(parse_firmware_asset("shellos-20261112-1.5.bin", "u").is_none());
        assert!(parse_firmware_asset("shellos-2026111-1.5.3.bin", "u").is_none());
    }

    #[test]
    fn database_images_only() {
        let db = parse_database_asset("shelldb-20261111.bin", "u").unwrap();
        assert_eq!(db.date, "20261111");
        assert_eq!(db.variant, None);
        assert!(parse_database_asset("db-20261111-sha256.txt", "u").is_none());
        assert!(parse_database_asset("db-20261111-reproduction.zip", "u").is_none());
    }

    #[test]
    fn release_listing_is_filtered_and_newest_first() {
        let json = serde_json::json!([
            {"tag_name": "v1.5.3", "assets": [
                {"name": "shellos-20261112-1.5.3.bin", "browser_download_url": "https://x/a.bin"},
                {"name": "shellos-20261112-1.5.3-btc-only.bin", "browser_download_url": "https://x/b.bin"},
                {"name": "shellos-bootloader-20261112-1.5.3.bin", "browser_download_url": "https://x/c.bin"},
                {"name": "shellos-v1.5.3-sha256.txt", "browser_download_url": "https://x/v.txt"}
            ]},
            {"tag_name": "v1.5.2", "assets": [
                {"name": "shellos-20261031-1.5.2.bin", "browser_download_url": "https://x/d.bin"},
                {"name": "shellos-experimental-20261031-1.5.2-btc-only.bin", "browser_download_url": "https://x/e.bin"}
            ]}
        ]);
        let files = release_files(&json, "firmware");
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "shellos-20261112-1.5.3.bin",
                "shellos-20261112-1.5.3-btc-only.bin",
                "shellos-20261031-1.5.2.bin",
            ],
            "newest first, omni before btc-only, nothing else"
        );
        assert_eq!(sha256_urls(&json), vec!["https://x/v.txt"]);
    }

    #[test]
    fn index_names_that_differ_from_asset_names_still_resolve() {
        assert_eq!(
            hash_list_name("shellos-20261112-1.5.3-btc-only.bin"),
            "shellos-btc-20261112-1.5.3.bin"
        );
        assert_eq!(hash_list_name("shelldb-20261111.bin"), "db.bin");
        assert_eq!(
            hash_list_name("shellos-20261112-1.5.3.bin"),
            "shellos-20261112-1.5.3.bin"
        );
    }

    #[test]
    fn sha_index_matches_by_exact_name() {
        let index = parse_sha256_index(
            "a3b5  shellos-20261112-1.5.3.bin\n\
             00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff  shellos-20261112-1.5.3-btc-only.bin\n\
             malformed line\n",
        );
        assert_eq!(index.len(), 1, "the 4-char hash is not a sha256");
        let mut files = vec![
            parse_firmware_asset("shellos-20261112-1.5.3-btc-only.bin", "u").unwrap(),
            parse_firmware_asset("shellos-20261112-1.5.3.bin", "u").unwrap(),
        ];
        attach_sha256(&mut files, &index);
        assert_eq!(
            files[0].sha256.as_deref(),
            Some("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
        );
        assert_eq!(
            files[1].sha256, None,
            "an uncovered asset must not look verified"
        );
    }

    #[test]
    fn only_hashed_files_are_offered_and_the_rest_are_counted() {
        let mut without = parse_firmware_asset("shellos-20260409-1.2.1.bin", "u").unwrap();
        without.sha256 = None;
        let with = parse_firmware_asset("shellos-20261112-1.5.3.bin", "u").unwrap();
        let index = HashMap::from([("shellos-20261112-1.5.3.bin".to_string(), "b".repeat(64))]);
        let mut files = vec![without, with];
        attach_sha256(&mut files, &index);
        let offered = offerable(files);
        assert_eq!(offered.files.len(), 1);
        assert_eq!(offered.dropped_without_hash, 1);
    }

    #[test]
    fn latest_keeps_one_date_and_both_variants_of_it() {
        let index = HashMap::from([
            ("shellos-20261112-1.5.3.bin".to_string(), "a".repeat(64)),
            ("shellos-btc-20261112-1.5.3.bin".to_string(), "b".repeat(64)),
            ("shellos-20261031-1.5.2.bin".to_string(), "c".repeat(64)),
        ]);
        let mut files = vec![
            parse_firmware_asset("shellos-20261112-1.5.3.bin", "u").unwrap(),
            parse_firmware_asset("shellos-20261112-1.5.3-btc-only.bin", "u").unwrap(),
            parse_firmware_asset("shellos-20261031-1.5.2.bin", "u").unwrap(),
        ];
        attach_sha256(&mut files, &index);
        let latest = latest(&offerable(files).files);
        assert_eq!(
            latest.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            vec![
                "shellos-20261112-1.5.3.bin",
                "shellos-20261112-1.5.3-btc-only.bin"
            ],
            "both variants of the newest date, nothing older"
        );
    }

    #[test]
    fn the_newest_release_without_a_hash_is_missed_rather_than_hidden() {
        let index = HashMap::from([("shellos-20261031-1.5.2.bin".to_string(), "c".repeat(64))]);
        let mut files = vec![
            parse_firmware_asset("shellos-20261112-1.5.3.bin", "u").unwrap(),
            parse_firmware_asset("shellos-20261031-1.5.2.bin", "u").unwrap(),
        ];
        attach_sha256(&mut files, &index);
        let published = newest_date(&files);
        let offered = latest(&offerable(files).files);
        assert_eq!(
            newest_date(&offered).as_deref(),
            Some("20261031"),
            "the list goes down to what has a hash"
        );
        assert_ne!(
            published,
            newest_date(&offered),
            "and the person has to be told the list stops early"
        );
    }

    #[test]
    fn a_release_that_ships_one_variant_offers_one_variant() {
        let index = HashMap::from([("shellos-20261031-1.5.2.bin".to_string(), "c".repeat(64))]);
        let mut files = vec![
            parse_firmware_asset("shellos-20261031-1.5.2.bin", "u").unwrap(),
            parse_firmware_asset("shellos-experimental-20261031-1.5.2-btc-only.bin", "u")
                .unwrap_or_else(|| {
                    parse_firmware_asset("shellos-20261031-1.5.2.bin", "u").unwrap()
                }),
        ];
        files.truncate(1);
        attach_sha256(&mut files, &index);
        let picked = latest(&offerable(files).files);
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].variant, Some(Variant::Omni));
    }

    #[test]
    fn variant_travels_as_the_devices_number() {
        let file = parse_firmware_asset("shellos-20261112-1.5.3-btc-only.bin", "u").unwrap();
        let json = serde_json::to_value(&file).unwrap();
        assert_eq!(json["variant"], serde_json::json!(1));
        let omni = parse_firmware_asset("shellos-20261112-1.5.3.bin", "u").unwrap();
        assert_eq!(
            serde_json::to_value(omni).unwrap()["variant"],
            serde_json::json!(0)
        );
    }

    #[test]
    fn device_variant_matches_the_asset_variant() {
        assert_eq!(Variant::from_fw_variant(0), Some(Variant::Omni));
        assert_eq!(Variant::from_fw_variant(1), Some(Variant::BtcOnly));
        assert_eq!(Variant::from_fw_variant(32), None);
    }
}
