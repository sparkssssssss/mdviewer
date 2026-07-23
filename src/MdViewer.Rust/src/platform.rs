#[cfg(target_os = "windows")]
mod windows;

#[cfg(not(target_os = "windows"))]
mod stub;

#[cfg(target_os = "windows")]
pub use self::windows::{run, show_error};

#[cfg(not(target_os = "windows"))]
pub use self::stub::{run, show_error};
