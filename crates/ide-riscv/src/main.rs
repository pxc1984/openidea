fn main() {
    std::process::exit(ide_riscv::cli(std::env::args().skip(1).collect()));
}
