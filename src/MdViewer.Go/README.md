# MdViewer.Go

Experimental native Go shell for MdViewer.

Goal: verify whether a smaller native WebView shell can open Markdown files faster than the current .NET WinForms shell while reusing the existing `src/MdViewer/assets` frontend.

## Status

MVP / experimental.

Implemented:

- Starts a native WebView window
- Serves existing `viewer.html`, `viewer.css`, and vendor assets from a local loopback server
- Opens a Markdown file passed on the command line
- Sends Markdown content to the existing frontend renderer
- Supports local images relative to the current Markdown file
- Supports the existing bottom-right edit action via Go binding

Not yet implemented:

- Windows file association
- Drag and drop
- Installer packaging
- Window state persistence
- PDF/export menu

## Build on Windows

```powershell
cd src/MdViewer.Go
go mod tidy
go build -ldflags="-H windowsgui -s -w" -o ../../publish/go/MdViewerGo.exe .
Copy-Item -Recurse ../MdViewer/assets ../../publish/go/assets
```

Run:

```powershell
../../publish/go/MdViewerGo.exe ../../examples/sample.md
```

The dependency is `github.com/webview/webview_go`, which uses the platform WebView. On Windows it requires WebView2 Runtime.
