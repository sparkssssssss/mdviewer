# MdViewer.Rust

Experimental Rust native shell skeleton for MdViewer.

## Goal

Keep the existing .NET version intact while preparing a Rust-based native shell that can eventually reproduce the same behavior with better startup characteristics.

## Planned responsibilities

- Create the native Windows window
- Host WebView2
- Load the existing `src/MdViewer/assets/viewer.html`
- Forward Markdown content into the frontend renderer
- Support local images, drag and drop, menus, theme toggles, file association, and edit actions

## Current status

Skeleton only.

## Next step

Implement the Windows/WebView2 host and point it at the shared MdViewer frontend assets.
