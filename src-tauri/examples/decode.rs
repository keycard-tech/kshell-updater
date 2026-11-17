use kshell_updater_lib::scan::{code_of, picture_of, square};

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: decode <png> | decode --yuyv <width> <height> <bytes>";
    let (path, packed) = match argv.as_slice() {
        [path] => (path.clone(), None),
        [flag, width, height, path] if flag == "--yuyv" => (
            path.clone(),
            Some((
                width.parse().unwrap_or_default(),
                height.parse().unwrap_or_default(),
            )),
        ),
        _ => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(why) => {
            eprintln!("{path}: {why}");
            std::process::exit(2);
        }
    };
    let picture = match packed {
        Some((width, height)) => match picture_of(width, height, bytes) {
            Ok(picture) => picture,
            Err(why) => {
                eprintln!("{path}: {why}");
                std::process::exit(2);
            }
        },
        None => image::open(&path)
            .unwrap_or_else(|why| {
                eprintln!("{path} is not a picture this app can read: {why}");
                std::process::exit(2);
            })
            .to_rgb8(),
    };
    match code_of(&square(&picture)) {
        Some(code) => println!("{code}"),
        None => {
            eprintln!("nothing decoded from {path}");
            std::process::exit(1);
        }
    }
}
