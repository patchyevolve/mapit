// Fixture: main.rs — calls into lib.rs (cross-file call)
mod lib;
mod utils;

fn main() {
    let result = lib::compute(6);
    println!("{}", result);
}
