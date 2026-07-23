pub fn run(_args: &[String]) -> Result<(), String> {
    println!("MdViewer Rust skeleton");
    println!("This non-Windows build is a compile-safe stub.");
    println!("The real native shell is implemented only on Windows.");
    Ok(())
}

pub fn show_error(title: &str, message: &str) {
    eprintln!("{title}: {message}");
}
