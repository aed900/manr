fn main() {
    env_logger::init();

    if let Err(e) = manr::get_args() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
