#![forbid(unsafe_code)]

fn main() {
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let exit = ba_cli::run(std::env::args_os(), &mut stdout, &mut stderr);
    std::process::exit(exit);
}
