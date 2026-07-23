use std::{env, fs, path::PathBuf};

fn main() {
    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    println!("cargo:rerun-if-changed=../MdViewer/app.ico");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let rc_path = out_dir.join("mdviewer-rust.rc");
    let icon_path = PathBuf::from("../MdViewer/app.ico")
        .canonicalize()
        .expect("app.ico exists");
    let icon_path = icon_path.to_string_lossy().replace('\\', "\\\\");

    fs::write(&rc_path, format!("1 ICON \"{icon_path}\"\n")).expect("write rc file");
    embed_resource::compile(&rc_path, embed_resource::NONE);
}
