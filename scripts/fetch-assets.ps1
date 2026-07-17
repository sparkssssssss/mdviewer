$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$vendor = Join-Path $root 'src/MdViewer/assets/vendor'
New-Item -ItemType Directory -Force -Path $vendor | Out-Null

function Get-Asset($Url, $OutFile) {
    $dir = Split-Path -Parent $OutFile
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    if (Test-Path $OutFile) {
        Write-Host "exists $OutFile"
        return
    }
    Write-Host "download $Url"
    Invoke-WebRequest -Uri $Url -OutFile $OutFile
}

Get-Asset 'https://cdn.jsdelivr.net/npm/markdown-it@14.1.0/dist/markdown-it.min.js' "$vendor/markdown-it/markdown-it.min.js"
Get-Asset 'https://cdn.jsdelivr.net/npm/markdown-it-anchor@9.2.0/dist/markdownItAnchor.umd.js' "$vendor/markdown-it-anchor/markdown-it-anchor.min.js"
Get-Asset 'https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.min.js' "$vendor/mermaid/mermaid.min.js"
Get-Asset 'https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css' "$vendor/katex/katex.min.css"
Get-Asset 'https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.js' "$vendor/katex/katex.min.js"
Get-Asset 'https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/contrib/auto-render.min.js' "$vendor/katex/auto-render.min.js"
Get-Asset 'https://cdn.jsdelivr.net/npm/highlight.js@11.10.0/lib/common.min.js' "$vendor/highlight/highlight.min.js"
Get-Asset 'https://cdn.jsdelivr.net/npm/highlight.js@11.10.0/styles/github.min.css' "$vendor/highlight/github.min.css"
Get-Asset 'https://cdn.jsdelivr.net/npm/highlight.js@11.10.0/styles/github-dark.min.css' "$vendor/highlight/github-dark.min.css"

# KaTeX CSS references font files. Download the font directory used by common formulas.
$fonts = @(
  'KaTeX_AMS-Regular.woff2',
  'KaTeX_Caligraphic-Bold.woff2',
  'KaTeX_Caligraphic-Regular.woff2',
  'KaTeX_Fraktur-Bold.woff2',
  'KaTeX_Fraktur-Regular.woff2',
  'KaTeX_Main-Bold.woff2',
  'KaTeX_Main-BoldItalic.woff2',
  'KaTeX_Main-Italic.woff2',
  'KaTeX_Main-Regular.woff2',
  'KaTeX_Math-BoldItalic.woff2',
  'KaTeX_Math-Italic.woff2',
  'KaTeX_SansSerif-Bold.woff2',
  'KaTeX_SansSerif-Italic.woff2',
  'KaTeX_SansSerif-Regular.woff2',
  'KaTeX_Script-Regular.woff2',
  'KaTeX_Size1-Regular.woff2',
  'KaTeX_Size2-Regular.woff2',
  'KaTeX_Size3-Regular.woff2',
  'KaTeX_Size4-Regular.woff2',
  'KaTeX_Typewriter-Regular.woff2'
)
foreach ($font in $fonts) {
    Get-Asset "https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/fonts/$font" "$vendor/katex/fonts/$font"
}
