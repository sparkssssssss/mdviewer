# MdViewer

MdViewer is a lightweight Markdown viewer for Windows. It focuses on reading local Markdown files and deliberately avoids editor, note database, and Electron-style heavy features.

## Features

- Windows desktop app based on WinForms + WebView2
- Markdown rendering via `markdown-it`
- Mermaid diagram rendering
- KaTeX math formulas
- Syntax highlighting via `highlight.js`
- Local image preview for relative Markdown image paths
- Drag and drop Markdown files into the window
- Command-line opening: `MdViewer.exe README.md`
- Light/dark theme toggle
- GitHub Actions Windows builds

## Screenshots / sample

A sample file is included at [`examples/sample.md`](examples/sample.md).

## Runtime requirements

For the small `framework-dependent` package:

1. Windows 10/11
2. [.NET 8 Desktop Runtime](https://dotnet.microsoft.com/download/dotnet/8.0)
3. [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)

Most Windows 10/11 systems already include WebView2.

For the `self-contained` package, .NET Runtime is bundled, but WebView2 Runtime is still required.

## Download builds

This repository is configured to build packages with GitHub Actions.

The workflow is manual only. It will not run automatically on push, pull request, or tag creation.

To build packages:

1. Open the repository on GitHub.
2. Go to **Actions**.
3. Select **Build Windows**.
4. Click **Run workflow**.

If you want to upload build artifacts to a GitHub Release, create/select the release tag first, run the workflow on that tag, and enable the `upload_release` option.

Generated packages:

- `MdViewer-win-x64-framework-dependent.zip` - smaller, requires .NET 8 Desktop Runtime
- `MdViewer-win-x64-self-contained.zip` - larger, includes .NET runtime
- `MdViewer-win-arm64-framework-dependent.zip` - for Windows on ARM64

## Local development

Install:

- .NET 8 SDK
- WebView2 Runtime
- PowerShell 7 or Windows PowerShell

Fetch local web assets:

```powershell
./scripts/fetch-assets.ps1
```

Run:

```powershell
dotnet run --project src/MdViewer/MdViewer.csproj -- examples/sample.md
```

Publish a small framework-dependent build:

```powershell
dotnet publish src/MdViewer/MdViewer.csproj -c Release -r win-x64 --self-contained false -p:PublishSingleFile=true -o publish/win-x64
```

Publish a self-contained build:

```powershell
dotnet publish src/MdViewer/MdViewer.csproj -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true -p:EnableCompressionInSingleFile=true -o publish/win-x64-self-contained
```

## Project layout

```text
.
├─ .github/workflows/build-windows.yml
├─ examples/sample.md
├─ scripts/fetch-assets.ps1
├─ src/MdViewer/
│  ├─ MainForm.cs
│  ├─ Program.cs
│  ├─ MdViewer.csproj
│  └─ assets/
│     ├─ viewer.html
│     ├─ viewer.css
│     └─ vendor/          # downloaded by scripts/fetch-assets.ps1
├─ MdViewer.sln
├─ LICENSE
└─ README.md
```

## Security notes

MdViewer is a local file viewer. To reduce risk when opening untrusted Markdown files:

- Raw HTML in Markdown is disabled.
- Mermaid is initialized with `securityLevel: 'strict'`.
- Links open externally through the system browser.

## License

MIT
