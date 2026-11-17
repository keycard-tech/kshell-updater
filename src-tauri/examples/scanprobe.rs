use kshell_updater_lib::scan::probe;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init()
        .ok();
    match probe(std::env::args().nth(1)) {
        Ok(answer) => println!("{answer}"),
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(1);
        }
    }
}
