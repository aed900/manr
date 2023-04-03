fn main() {
    if let Err(e) = manr::Config::new() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}