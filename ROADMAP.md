# starview roadmap

Selected features to build, one at a time. Each: implement → build + test →
commit → bump version (minor for a feature, patch for a small tweak) → tag →
push (no CI wait) → next.

Verification note: starview reads the keyboard's **raw HID matrix** (Oryx
protocol), not OS keystrokes, so injected input can't exercise typing logic —
keypress-driven features are covered by unit tests on pure logic + a launch
smoke test, plus a seeded-data screenshot for anything visual.

Order is easy/low-risk first, then the data-model and harder ones.

## Overlay / UX

- [x] **Reset stats** (v0.4.0) — tray item that clears all accumulated counts. Tray →
  `TrayEvent::ResetStats` → `AppEvent::ResetStats`; overlay sets
  `stats = Stats::default()` and saves. (`Stats::default()` clears every field,
  so this keeps working as new stat kinds are added.)
- [x] **Adjustable size/scale** (v0.5.0) — tray submenu (75/100/125/150/200%)
  sets the UI zoom factor and resizes the window to match; persisted.
- [x] **Color theme options** (v0.6.0) — "Accent color" tray submenu
  (Blue/Green/Purple/Amber/Pink); accent drives key ghosts, trackball, and UI
  highlights; persisted.
- [x] **Recent-layer breadcrumb** (v0.7.0) — fading "2 › 5 › 1" trail of the
  last few layers, centered at the panel top for ~2s after a switch.
- [x] **Global hotkey toggle** (v0.8.0) — Ctrl+Alt+O (`RegisterHotKey` on the
  tray thread) toggles a master visibility off-switch.
- [x] **Multi-monitor placement** (v0.9.0) — "Overlay monitor" tray submenu
  (shown when >1 display) docks to a chosen monitor's corner using its desktop
  offset; persisted. Also fixed right/bottom corners at non-100% zoom.

## Typing analytics (keypress-driven; pure logic unit-tested)

- [x] **Live WPM readout** (v0.10.0) — rolling words-per-minute (chars/5 over a
  5s window, decays when idle), shown after the title; tray toggle.
- [x] **Per-hand / per-finger load** (v0.11.0) — geometry-based finger slots; a
  10-bar load chart (left accent / right grey, P R M I T labels) in the center
  gap; tray toggle.
- [ ] **Bigram frequency** — count consecutive key pairs; expose top digraphs.
- [ ] **Daily counts & streaks** — per-day press totals + streak, keyed by local
  date (Win32 local time); persisted in stats.
- [ ] **Time-windowed views** — today / this week / all-time toggle for the
  heatmap + counters, from per-day buckets.
- [ ] **Export / stats window** — an egui window (or file export) summarizing
  the accumulated stats.
- [ ] **Substitution-pair typos** — when a backspaced char is replaced by a
  different one (same window, no mouse), record the (typed→meant) pair; show top
  confusions. Fuzzy by nature.

## Hardware / layout

- [ ] **Auto-detect layout** — discover the connected board's layout instead of
  the hardcoded hash. Best-feasible: persist the chosen layout id in settings
  and remember it across runs; investigate reading it from the device.

_Not selected: support for other ZSA boards (Voyager/Ergodox)._
