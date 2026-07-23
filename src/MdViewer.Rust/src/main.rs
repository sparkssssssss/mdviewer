#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod platform;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(error) = platform::run(&args) {
        platform::show_error("MdViewer Rust", &error.to_string());
    }
}
