# starview
## Warning
> [!CAUTION]
> This project is vibecoded and not very well tested. It Works On My Machine but proceed with caution

---

A tiny Windows overlay that shows which layer your ZSA keyboard is on — but only
when you're *not* on the base layer. A semi-transparent panel appears in the
top-right corner rendering the active layer's full keymap (keycaps, labels,
hold-action hints, live highlights on physically pressed keys), and disappears
when you return to base. The layout re-syncs from Oryx every 5 minutes, so
edits in the configurator show up without restarting.

The overlay is a ghost: clicks pass straight through it, it never appears in
Alt-Tab or on the taskbar, and it never steals focus.

## Install

Grab `starview-setup.exe` from the
[latest release](https://github.com/pmaxhogan/starview/releases/latest) and
run it — per-user install (no admin), with an optional start-with-Windows
task. The app then checks for new releases daily and offers updates via the
tray menu ("Install update vX.Y.Z"), installing silently and relaunching
itself. Or build from source: `cargo build --release`.

Releases are built by the `release` GitHub Actions workflow on `v*` tags
(tag must match `Cargo.toml`'s version); each release carries the installer,
a portable exe, and SHA256SUMS.

## Usage

```
starview [--always] [layout-hash-id] [geometry]
```

A system tray icon (dark disc with a blue dot) provides runtime settings,
persisted to `%LOCALAPPDATA%\starview\settings.json`: **Pin base layer**
(keep the overlay up on the base layer too; `--always` forces this on for the
session), **Overlay corner** (which display corner to dock to; bottom corners
leave taskbar clearance), **Opacity** (100–40%), and **Auto-hide after** (fade
the overlay out after an idle interval — Never/5s/15s/30s/1m/2m/5m; a layer
change or keypress brings it back), **Rainbow key ghosts** (color the
pressed/afterglow key highlights as a rainbow that sweeps across the board),
and **Key heatmap** (tint each key by its lifetime press count — log-scaled
cold-blue to hot-red — with a running press total in the header; counts persist
to `%LOCALAPPDATA%\starview\stats.json`), plus **Quit**.

The overlay is click-through, so its on-panel controls are gated behind the
Shift key to keep them out of the way: hold **Shift** and drag the hamburger
icon (top-right of the panel) to reposition it anywhere on-screen (it won't go
off the monitor edge, and the dragged spot persists until you pick a corner
again); hold **Shift** and click the **✕** next to it to quit.

On non-base layers, unmapped (transparent) keys show the base layer's label
dimmed — matching QMK's fall-through semantics — while `KC_NO` keys stay blank.
Physically held keys highlight, and the highlight fades out over ~3s after
release.

Defaults are baked in (`jmvGw` / `moonlander`) — the hash id and geometry come
straight from your Oryx URL: `configure.zsa.io/{geometry}/layouts/{hashId}/...`.
The layout must be public in Oryx. Layer names are fetched once at startup and
cached in `%LOCALAPPDATA%\starview`, so it works offline after the first run.
If the fetch fails entirely, the overlay falls back to layer numbers.

The installer's "Start starview when Windows starts" task handles autostart
(an HKCU Run registry entry). For source builds: `Win+R` → `shell:startup` →
drop in a shortcut to `target\release\starview.exe`.

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
  a font fallback so arrows and symbol labels render. Thumb-cluster keys are
  drawn as actually-rotated polygons with rotated labels. Keydown/keyup HID
  events (matrix positions, mapped through the same keyboard.json table)
  highlight physically held keys while the overlay is visible.
- **Trackball indicator** (`src/trackball.rs`): the ZSA Navigator presents as
  a HID mouse on the keyboard's USB composite device, so a Raw Input
  message-only window (`RIDEV_INPUTSINK`) watches per-device mouse deltas,
  filtered to ZSA's vendor id (other mice are ignored). A ring between the
  thumb clusters shows a dot that deflects with ball motion and glides back
  to center; it appears after the first motion from a ZSA pointing device.
- **Overlay window** (`src/overlay.rs`): eframe/egui with the **glow** renderer
  (wgpu, the eframe default, cannot do transparent windows on Windows). The
  ghost behavior is raw Win32: `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW |
  WS_EX_LAYERED | WS_EX_TRANSPARENT` with `WS_EX_APPWINDOW` cleared. winit
  can't express these and rewrites the ex-styles wholesale on its own state
  changes, so they're re-asserted on every update tick rather than set once.
  The window itself stays permanently visible; "hidden" just means nothing is
  drawn (a truly hidden window stops presenting, so its stale last frame
  would flash on re-show).

## Roadmap

- Other ZSA boards (Voyager, Ergodox EZ): needs their geometry tables; the
  rest of the pipeline is board-agnostic (the renderer falls back to the
  name-only bubble when the key count doesn't match the geometry).
- Macro contents and underglow-color swatches on rendered keys.
