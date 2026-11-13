//! The verifier checked against vectors produced by the site's own TypeScript implementation, so the
//! two agree on the same bytes rather than on each author's reading of the format.
//! Regenerate with `gen-vectors.mts` in a checkout of keycard-tech/shell-update-site.

use kshell_updater_lib::units::{self, UnitsError};
use kshell_updater_lib::ur;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vectors {
    verify_key: String,
    list: String,
    build: u32,
    rows: usize,
    case_challenge: String,
    challenge_text: String,
    challenge_payload: String,
    cases: Vec<Case>,
    bad_lists: Vec<BadList>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    challenge: String,
    response: String,
    outcome: String,
    #[serde(default)]
    reason: String,
}

#[derive(Deserialize)]
struct BadList {
    name: String,
    bytes: String,
    reason: String,
}

fn vectors() -> Vectors {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/verify-vectors.json"
    ))
    .expect("the vectors file is part of the repository");
    serde_json::from_str(&text).expect("the vectors file is the JSON the generator writes")
}

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|at| u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).unwrap())
        .collect()
}

fn challenge(hex: &str) -> [u8; 32] {
    bytes(hex).try_into().expect("a challenge is 32 bytes")
}

#[test]
fn the_list_from_the_site_loads() {
    let vectors = vectors();
    let list = units::load_list_with_key(&bytes(&vectors.list), &vectors.verify_key).unwrap();
    assert_eq!(list.build, vectors.build);
    // Rows are padded to a power of two, so the count is a bound rather than a device total.
    assert!(list.row_count >= vectors.rows);
    assert_eq!(list.row_count, list.row_count.next_power_of_two());
}

#[test]
fn every_answer_is_read_the_way_the_site_reads_it() {
    let vectors = vectors();
    let list = units::load_list_with_key(&bytes(&vectors.list), &vectors.verify_key).unwrap();

    for case in &vectors.cases {
        let answer =
            units::check_answer(&list, &challenge(&case.challenge), &bytes(&case.response));
        let problem = match &case.outcome[..] {
            "row" => {
                assert!(
                    answer.is_ok(),
                    "{}: the site accepts this answer, the Rust refused it as {:?}",
                    case.name,
                    answer.err()
                );
                continue;
            }
            "not-registered" => match answer {
                Err(UnitsError::NotRegistered { .. }) => continue,
                other => format!(
                    "{}: expected an unregistered unit, got {:?}",
                    case.name, other
                ),
            },
            "refusal" => match answer {
                Err(UnitsError::Refusal(message)) => {
                    assert!(
                        message.contains(&case.reason),
                        "{}: refused, but says {message} rather than mentioning {}",
                        case.name,
                        case.reason
                    );
                    continue;
                }
                other => format!(
                    "{}: expected a refusal mentioning {}, got {:?}",
                    case.name, case.reason, other
                ),
            },
            other => panic!("{}: unknown outcome {other}", case.name),
        };
        panic!("{problem}");
    }
}

#[test]
fn every_refusal_of_a_list_is_refusal_here_too() {
    let vectors = vectors();

    for bad in &vectors.bad_lists {
        let error = units::load_list_with_key(&bytes(&bad.bytes), &vectors.verify_key)
            .err()
            .unwrap_or_else(|| panic!("{}: the Rust accepted a list the site refuses", bad.name));
        assert!(
            error.to_string().contains(&bad.reason),
            "{}: refused, but says {error} rather than mentioning {}",
            bad.name,
            bad.reason
        );
    }
}

#[test]
fn the_challenge_text_is_the_ones_the_site_writes() {
    let vectors = vectors();
    let payload = bytes(&vectors.challenge_payload);

    assert_eq!(
        ur::ur_encode(&units::encode_init(&challenge(&vectors.case_challenge))),
        vectors.challenge_text
    );
    assert_eq!(ur::ur_decode(&vectors.challenge_text).unwrap(), payload);
}

#[test]
fn the_list_keycard_publishes_verifies() {
    // The file the factory published, kept out of the repository because it is 131 KB: run
    // `curl -L -o ~/scratch/units.bin <release asset>` to have this check do anything.
    let Ok(bytes) = std::fs::read(format!(
        "{}/scratch/units-20261112.bin",
        std::env::var("HOME").unwrap_or_default()
    )) else {
        return;
    };
    let list =
        units::load_list(&bytes).expect("the published list must verify against the pinned key");
    assert_eq!(list.build, 20261112);
    assert_eq!(list.row_count, 4096);
}
