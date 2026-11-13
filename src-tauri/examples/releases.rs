// cargo run --example releases — the release pipeline with no window open: list what the updater
// would offer, fetch each release's hash list, download the newest image of each kind and verify it
// against the published hash. Prints what was read rather than what was expected.
use kshell_updater_lib::{build_catalogue, catalogue_for, download_verified, image};

fn main() {
    // `cargo run --example releases -- --fixtures ../tests/fixtures/releases.json` writes what the
    // renderer really gets, so shots/frontend-check.mjs can stub the bridge with it.
    let args: Vec<String> = std::env::args().collect();
    if let Some(at) = args.iter().position(|a| a == "--fixtures") {
        let path = args.get(at + 1).expect("--fixtures wants a path");
        tauri::async_runtime::block_on(async {
            let both = serde_json::json!({
                "firmware": build_catalogue("firmware").await.expect("firmware listing"),
                "database": build_catalogue("database").await.expect("database listing"),
                // A sample of what `inspect` answers, so the frontend harness can build stubs with the
                // same field names instead of guessing them. It guessed once and a label shipped as
                // "undefined-btc from file".
                "imageInfo": serde_json::to_value(image::ImageInfo {
                    version: "0".to_string(),
                    kind: "firmware",
                    fw_variant: Some(0),
                    bytes: 0,
                    sha256: String::new(),
                    matches_release: None,
                })
                .expect("serialise image info"),
            });
            std::fs::write(
                path,
                format!("{}\n", serde_json::to_string_pretty(&both).unwrap()),
            )
            .expect("write fixtures");
            println!("wrote {path}");
        });
        return;
    }

    tauri::async_runtime::block_on(async {
        for kind in ["firmware", "database"] {
            let (files, unofferable, index, failures) = match catalogue_for(kind).await {
                Ok(ok) => ok,
                Err(error) => {
                    println!("{kind}: listing failed — {error}");
                    continue;
                }
            };
            println!(
                "{kind}: {} offerable, {} without a published hash, {} published hashes, {} hash lists unreachable",
                files.len(),
                unofferable,
                index.len(),
                failures.len()
            );
            for file in &files {
                let hash = file.sha256.clone().unwrap_or_default();
                println!(
                    "  {:<44} {:<10} sha256 {}…",
                    file.name,
                    file.version.as_deref().unwrap_or(&file.date),
                    &hash[..12]
                );
            }
            for file in files.iter().take(2) {
                match download_verified(kind, &file.url, &index).await {
                    Ok((info, bytes)) => println!(
                        "  verified {}: {} bytes, version {}, variant {:?}, sha256 {}…",
                        file.name,
                        bytes.len(),
                        info.version,
                        info.fw_variant,
                        &info.sha256[..12]
                    ),
                    Err(error) => println!("  {} FAILED — {error}", file.name),
                }
            }
        }
    });
}
