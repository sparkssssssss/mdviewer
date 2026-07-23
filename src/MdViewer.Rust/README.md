# MdViewer.Rust

Experimental Rust native shell for MdViewer.

## Goal

Keep the existing .NET version intact while building a Rust-based native shell that reuses the current MdViewer frontend assets and aims for faster startup with fewer runtime dependencies.

## Current status

Implemented in the Rust shell:

- Native Windows window
- WebView2 hosting through `webview2-com`
- Shared frontend asset loading via WebView2 virtual host mapping
- Command-line Markdown opening: `MdViewerRust.exe README.md`
- In-window **Open** button
- File menu: Open, Reload, Exit
- View menu: Toggle Theme, Zoom In, Zoom Out, Reset Zoom
- Markdown rendering through the existing `viewer.html`
- Local relative images via `mdviewer-files.local` virtual host mapping
- Drag-and-drop file opening
- External link interception and system-browser opening
- Edit source action using VS Code or Notepad fallback
- Auto-refresh when the current Markdown file changes
- Per-user file association commands: `--associate` / `--unassociate`

Still experimental / needs Windows runtime testing:

- Polish around installer packaging and icon behavior
- More exhaustive parity testing against the .NET version

## Build on Windows

From the repository root:

```powershell
cargo build --manifest-path src/MdViewer.Rust/Cargo.toml --release --locked
```

Package manually:

```powershell
$out = 'publish/rust-win-x64'
New-Item -ItemType Directory -Force -Path $out | Out-Null
Copy-Item src/MdViewer.Rust/target/release/mdviewer-rust.exe "$out/MdViewerRust.exe"
Copy-Item -Recurse src/MdViewer/assets "$out/assets"
Copy-Item src/MdViewer/app.ico "$out/app.ico"
Copy-Item README.md "$out/README.md"
Copy-Item LICENSE "$out/LICENSE"
Copy-Item examples/sample.md "$out/sample.md"
```

Run:

```powershell
publish/rust-win-x64/MdViewerRust.exe examples/sample.md
```

Associate Markdown file types for the current user:

```powershell
publish/rust-win-x64/MdViewerRust.exe --associate
```

Undo the association:

```powershell
publish/rust-win-x64/MdViewerRust.exe --unassociate
```

## Notes

The Rust shell still requires Microsoft Edge WebView2 Runtime, same as the .NET version. The frontend renderer remains shared with `src/MdViewer/assets`.
