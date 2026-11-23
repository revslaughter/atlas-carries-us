//! Prints the version of the lending rules.

/// Prints the version and exits.
fn main() {
    println!("{}", lending::log_line!("rules {}", lending::rules::VERSION));
}
