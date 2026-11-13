// cargo run --example units — the verify flow's half that needs no device and no window: fetch the
// newest produced units list, check it against the pinned key, and write the challenge the app would
// put in the QR code. What it prints is what keycard-tech/shell-units published, not a fixture.
use kshell_updater_lib::ur::ur_decode;
use kshell_updater_lib::{units, ur, verify};

fn main() {
    tauri::async_runtime::block_on(async {
        let (bytes, list) = verify::newest_list().await.expect("a published units list");
        println!(
            "units list: {} bytes, {} rows, build {}",
            bytes.len(),
            list.row_count,
            list.build
        );
        println!("pinned key: {}", units::UNITS_VERIFY_KEY);

        let challenge = units::new_challenge();
        let text = ur::ur_encode(&units::encode_init(&challenge));
        println!("challenge:  {text}");
        assert_eq!(
            ur_decode(&text).expect("the code the app writes must read back"),
            units::encode_init(&challenge),
        );

        let svg = verify::challenge_svg(&text).expect("a QR code for the challenge");
        // One home for fixtures, and it is the one shots/frontend-check.mjs already reads from.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri is under the repository")
            .join("tests/fixtures/challenge.svg");
        std::fs::create_dir_all(path.parent().expect("the fixtures directory"))
            .expect("the fixtures directory exists");
        std::fs::write(&path, &svg).expect("the challenge svg is written");
        println!(
            "qr:         {} bytes of svg at {}",
            svg.len(),
            path.display()
        );
    });
}
