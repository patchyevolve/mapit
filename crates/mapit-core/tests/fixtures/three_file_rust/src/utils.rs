// Fixture: utils.rs — defines double(), called from lib.rs
pub fn double(x: u32) -> u32 {
    x * 2
}

// External call — calls into std (unresolvable in project scope)
pub fn print_version() {
    println!("v1.0");
}
