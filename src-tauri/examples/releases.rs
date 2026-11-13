// cargo run --example releases — the release pipeline with no window open: list what the updater
// would offer, fetch each release's hash list, download the newest image of each kind and verify it
// against the published hash. Prints what was read rather than what was expected.
use kshell_updater_lib::{catalogue_for, download_verified};

fn main() {
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
