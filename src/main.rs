fn main() {
    if let Err(error) = heeupscale::run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
