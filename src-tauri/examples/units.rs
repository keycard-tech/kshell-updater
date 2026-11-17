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
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri is under the repository")
            .join("tests/fixtures");
        std::fs::create_dir_all(&fixtures).expect("the fixtures directory exists");
        std::fs::write(fixtures.join("challenge.svg"), &svg).expect("the challenge svg is written");
        std::fs::write(fixtures.join("units-latest.bin"), &bytes)
            .expect("the published units list is kept as a test fixture");
        println!("qr:         {} bytes of svg", svg.len());
    });
}
