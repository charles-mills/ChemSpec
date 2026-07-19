# ChemSpec application icon

`app-icon.png` is the canonical 1024×1024 packaging master derived from the
supplied ChemSpec logo. The artwork is inset on its original black background
to keep important details inside platform icon masks.

The PNG size variants are used by Linux packages and by `cargo-packager` when
it builds the macOS ICNS icon. `app-icon.ico` contains 16, 24, 32, 48, 64, 128,
and 256 pixel Windows variants. `128x128.rgba` is the exact raw pixel buffer
embedded into the Iced window so development builds also show the logo in
title bars, task switchers, and docks where the platform supports it.
