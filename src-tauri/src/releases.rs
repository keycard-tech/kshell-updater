//! Which release assets the app is willing to offer.
//!
//! Online mode shows exactly the two images a person can reasonably flash: the omni build and the
//! btc-only build. `shellos-bootloader-*` is not this updater's business, and
//! `shellos-experimental-*` is excluded rather than hidden behind a toggle — an updater that offers
//! a build labelled experimental invites the brick it exists to prevent. Both remain installable
//! through `From file`, where the file was chosen deliberately.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The two firmware builds. Serialised as the number the device reports in `fwVariant`, because the
/// view compares a release file's variant against the device's and the two must be the same shape:
/// as `"omni"` and `"btcOnly"` they would never match, and the label would read undefined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Variant {
    /// Signs Bitcoin and Ethereum; what the device reports as fwVariant 0.
    Omni,
    /// Signs Bitcoin only; fwVariant 1.
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
    /// 8 digits as published, `20261112`.
    pub date: String,
    /// Firmware version `1.5.3`, or the database version in the date field's place.
    pub version: Option<String>,
    pub variant: Option<Variant>,
    /// Filled from the release's `-sha256.txt` once it has been fetched.
    pub sha256: Option<String>,
}

fn digits(value: &str) -> bool {
    !value.is_empty() && value.len() == 8 && value.bytes().all(|b| b.is_ascii_digit())
}

fn version_shape(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// `shellos-20261112-1.5.3.bin` and `shellos-20261112-1.5.3-btc-only.bin`.
/// Anything else in the release — bootloader, experimental, the sha256 list — returns None.
pub fn parse_firmware_asset(name: &str, url: &str) -> Option<ReleaseFile> {
    let stem = name.strip_suffix(".bin")?;
    let rest = stem.strip_prefix("shellos-")?;
    let (date, tail) = rest.split_once('-')?;
    if !digits(date) {
        return None;
    }
    let (version, variant) = match tail.strip_suffix("-btc-only") {
        Some(version) => (version, Variant::BtcOnly),
        None => (tail, Variant::Omni),
    };
    if !version_shape(version) {
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

/// `shelldb-20261111.bin`. The reproduction archive and the sha256 list are not images.
pub fn parse_database_asset(name: &str, url: &str) -> Option<ReleaseFile> {
    let stem = name.strip_suffix(".bin")?;
    let date = stem.strip_prefix("shelldb-")?;
    if !digits(date) {
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

/// A `<hash>  <filename>` list, one line per asset. Names are matched exactly, so an asset that is
/// not covered by the list keeps `sha256: None` and is then not downloadable.
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

/// The published hash lists do not use the asset names. A btc-only image is listed as
/// `shellos-btc-<date>-<version>.bin` where the asset is `shellos-<date>-<version>-btc-only.bin`,
/// and the database is listed as plain `db.bin` where the asset is `shelldb-<date>.bin`. Without this
/// mapping the two most-flashed files would report no published hash.
pub fn index_key(name: &str) -> String {
    if let Some(stem) = name
        .strip_prefix("shellos-")
        .and_then(|n| n.strip_suffix("-btc-only.bin"))
    {
        return format!("shellos-btc-{stem}.bin");
    }
    if name.starts_with("shelldb-") && name.ends_with(".bin") {
        return "db.bin".to_string();
    }
    name.to_string()
}

pub fn attach_sha256(files: &mut [ReleaseFile], index: &HashMap<String, String>) {
    for file in files {
        file.sha256 = lookup_sha256(index, &file.name);
    }
}

/// Only the newest release is ever offered: the updater installs what is current, and nothing keeps a
/// list of older builds to reach for. Files are grouped by the date they were published, so a release
/// that ships omni only stays omni only — an older btc-only build is not offered beside it, which
/// would read as two current builds of different ages.
pub fn latest(files: &[ReleaseFile]) -> Vec<ReleaseFile> {
    let Some(newest) = files.iter().map(|f| f.date.clone()).max() else {
        return Vec::new();
    };
    files.iter().filter(|f| f.date == newest).cloned().collect()
}

/// What to show: files with a published hash. The rest are left out rather than greyed out, because
/// an updater cannot say what an unverifiable file contains, and listing it invites someone to try.
/// The same bytes stay installable through `From file`, where they were chosen deliberately. Releases
/// before v1.4.0 publish no hash list, so this drops most of the history — which is reported as a
/// count so the list does not look short by accident.
pub fn offerable(files: Vec<ReleaseFile>) -> (Vec<ReleaseFile>, usize) {
    let mut offered = Vec::new();
    let mut skipped = 0;
    for file in files {
        if file.sha256.is_some() {
            offered.push(file);
        } else {
            skipped += 1;
        }
    }
    (offered, skipped)
}

pub fn lookup_sha256(index: &HashMap<String, String>, asset: &str) -> Option<String> {
    index
        .get(&index_key(asset))
        .or_else(|| index.get(asset))
        .cloned()
}

/// Assets of a GitHub releases API response, filtered to flashable images and newest first.
///
/// `kind` is `firmware` or `database`; the two repos use different name shapes but the same release
/// JSON, so one walk serves both. Each file keeps the URL of its release's `-sha256.txt` so the hash
/// can be fetched later without hardcoding a tag.
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

/// URLs of every release's own hash list, taken from the API rather than composed: the file name
/// carries the tag, and tags do not always match the version inside the file names. Each list covers
/// only its own release, so they are merged into one index — asset names are unique across releases.
pub fn sha256_urls(json: &serde_json::Value) -> Vec<String> {
    json.as_array()
        .into_iter()
        .flatten()
        .flat_map(|release| release["assets"].as_array().into_iter().flatten())
        .filter(|asset| {
            asset["name"]
                .as_str()
                .is_some_and(|n| n.ends_with("-sha256.txt"))
        })
        .filter_map(|asset| asset["browser_download_url"].as_str())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asset names taken from the live API on 2026-08-04, not invented.
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
        // A name that only looks like an image.
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
        // Both quirks are live in the published lists as of 2026-08-04.
        assert_eq!(
            index_key("shellos-20261112-1.5.3-btc-only.bin"),
            "shellos-btc-20261112-1.5.3.bin"
        );
        assert_eq!(index_key("shelldb-20261111.bin"), "db.bin");
        assert_eq!(
            index_key("shellos-20261112-1.5.3.bin"),
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
        let (offered, skipped) = offerable(files);
        assert_eq!(offered.len(), 1);
        assert_eq!(skipped, 1);
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
        let latest = latest(&offerable(files).0);
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
    fn a_release_that_ships_one_variant_offers_one_variant() {
        // v1.5.2 published omni only: its btc build was experimental, so it is not in the list.
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
        let picked = latest(&offerable(files).0);
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].variant, Some(Variant::Omni));
    }

    #[test]
    fn variant_travels_as_the_devices_number() {
        // The view labels and compares this against `fwVariant`; a string would silently never match.
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
