# starview

A tiny Windows overlay that shows which layer your ZSA keyboard is on — but only
when you're *not* on the base layer. A semi-transparent panel appears in the
top-right corner rendering the active layer's full keymap (keycaps, labels,
hold-action hints), and disappears when you return to base.

The overlay is a ghost: clicks pass straight through it, it never appears in
Alt-Tab or on the taskbar, and it never steals focus.

## Usage

```
starview [layout-hash-id] [geometry]
```

Defaults are baked in (`jmvGw` / `moonlander`) — the hash id and geometry come
straight from your Oryx URL: `configure.zsa.io/{geometry}/layouts/{hashId}/...`.
The layout must be public in Oryx. Layer names are fetched once at startup and
cached in `%LOCALAPPDATA%\starview`, so it works offline after the first run.
If the fetch fails entirely, the overlay falls back to layer numbers.

To start it with Windows: `Win+R` → `shell:startup` → drop in a shortcut to
`target\release\starview.exe` (build with `cargo build --release`; the release
build has no console window).

For testing/styling, set `STARVIEW_FORCE_LAYER=<n>` to pin the overlay to a
layer regardless of what the keyboard reports.

## How it works

- **Layer detection** (`src/hid.rs`): stock ZSA/Oryx firmware speaks a small
  protocol over the QMK raw-HID collection (usage page `0xFF60`, usage `0x61`,
  32-byte reports). Sending `PAIRING_INIT` (`0x01`) makes the keyboard push an
  event on every layer change: `[0x05, layer_index, 0xFE, ...]`, with the
  current layer re-emitted immediately on pairing. This is the same channel
  Keymapp's live training uses; Windows fans HID input reports out to every
  open handle, so starview and Keymapp coexist fine, and neither needs the
  other running. The keyboard's paired flag lives in its RAM, so the watcher
  re-pairs after reconnects and periodically when idle (idempotent, doubles as
  a resync).
- **Layer names + key data** (`src/oryx.rs`): one GraphQL query against the
  unofficial `https://oryx.zsa.io/graphql` endpoint (`layout(hashId,
  revisionId: "latest") { revision { layers { title position keys } } }`),
  parsed leniently because the stored key JSON has drifted over the years.
- **Keymap render** (`src/geometry.rs`, `src/keycodes.rs`, `src/overlay.rs`):
  key positions are transcribed from QMK's `moonlander/keyboard.json` with
  Oryx's ±30° thumb-cluster rotation, indexed in Oryx key order (left half
  0–35, right 36–71; rows top-to-bottom, then big-red, then piano keys —
  validated against a live layout). Labels compose from Oryx custom labels,
  a ~390-entry QMK keycode table, layer-switch targets ("MO 2"), and
  modifier wrappers ("G+1", "C+Bksp"). Windows' Segoe UI Symbol is loaded as
  a font fallback so arrows and symbol labels render.
- **Overlay window** (`src/overlay.rs`): eframe/egui with the **glow** renderer
  (wgpu, the eframe default, cannot do transparent windows on Windows). The
  ghost behavior is raw Win32: `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW |
  WS_EX_LAYERED | WS_EX_TRANSPARENT` with `WS_EX_APPWINDOW` cleared, and
  show/hide via `ShowWindow(SW_SHOWNA/SW_HIDE)`. winit can't express these and
  rewrites the ex-styles wholesale on its own state changes (and re-shows the
  window after the first frame), so both the styles and the visibility are
  re-asserted on every update tick rather than set once.

## Roadmap

- Other ZSA boards (Voyager, Ergodox EZ): needs their geometry tables; the
  rest of the pipeline is board-agnostic (the renderer falls back to the
  name-only bubble when the key count doesn't match the geometry).
- Macro contents and underglow-color swatches on rendered keys.
