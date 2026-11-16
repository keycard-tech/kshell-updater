fn main() {
    match hidapi::HidApi::new() {
        Ok(api) => {
            let shells: Vec<_> = api
                .device_list()
                .filter(|d| d.vendor_id() == 0x1209)
                .collect();
            println!(
                "hidapi initialised, {} HID devices, {} with vendor 0x1209",
                api.device_list().count(),
                shells.len()
            );
            for d in shells {
                println!(
                    "  {:04x}:{:04x} {}",
                    d.vendor_id(),
                    d.product_id(),
                    d.product_string().unwrap_or("?")
                );
            }
        }
        Err(e) => println!("hidapi failed to initialise: {e}"),
    }
}
