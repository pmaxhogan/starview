# Changelog

All notable changes to starview, newest first. Generated from the git history
by [git-cliff](https://git-cliff.org) — edit commit messages, not this file.

## [v0.19.0](https://github.com/pmaxhogan/starview/releases/tag/v0.19.0) — 2026-07-31

### Changed

- Port starview to macOS

## [v0.18.2](https://github.com/pmaxhogan/starview/releases/tag/v0.18.2) — 2026-06-18

### Added

- Add an auto-generated changelog and wire it into releases

## [v0.18.1](https://github.com/pmaxhogan/starview/releases/tag/v0.18.1) — 2026-06-18

### Changed

- Re-scan monitors when the tray menu opens

## [v0.18.0](https://github.com/pmaxhogan/starview/releases/tag/v0.18.0) — 2026-06-17

### Added

- Add fullscreen "display" mode

## [v0.17.0](https://github.com/pmaxhogan/starview/releases/tag/v0.17.0) — 2026-06-17

### Changed

- Remember the layout across runs

## [v0.16.0](https://github.com/pmaxhogan/starview/releases/tag/v0.16.0) — 2026-06-17

### Added

- Track substitution-pair typos (typed -> meant)

## [v0.15.0](https://github.com/pmaxhogan/starview/releases/tag/v0.15.0) — 2026-06-17

### Added

- Add "Export stats" — write a full report and open it

## [v0.14.0](https://github.com/pmaxhogan/starview/releases/tag/v0.14.0) — 2026-06-17

### Added

- Add time-windowed press heatmap (all-time / today / week)

## [v0.13.0](https://github.com/pmaxhogan/starview/releases/tag/v0.13.0) — 2026-06-17

### Added

- Add daily press counts and a streak

## [v0.12.0](https://github.com/pmaxhogan/starview/releases/tag/v0.12.0) — 2026-06-17

### Added

- Track and show top bigrams

## [v0.11.0](https://github.com/pmaxhogan/starview/releases/tag/v0.11.0) — 2026-06-17

### Added

- Add a per-finger load chart

## [v0.10.0](https://github.com/pmaxhogan/starview/releases/tag/v0.10.0) — 2026-06-17

### Added

- Add a live WPM readout

## [v0.9.1](https://github.com/pmaxhogan/starview/releases/tag/v0.9.1) — 2026-06-17

### Changed

- Group coloring modes into a submenu; separate Reset key stats

## [v0.9.0](https://github.com/pmaxhogan/starview/releases/tag/v0.9.0) — 2026-06-17

### Added

- Add multi-monitor placement

## [v0.8.1](https://github.com/pmaxhogan/starview/releases/tag/v0.8.1) — 2026-06-17

### Changed

- Make the three board-coloring modes mutually exclusive

## [v0.8.0](https://github.com/pmaxhogan/starview/releases/tag/v0.8.0) — 2026-06-17

### Added

- Add a global show/hide hotkey (Ctrl+Alt+O)

## [v0.7.0](https://github.com/pmaxhogan/starview/releases/tag/v0.7.0) — 2026-06-17

### Added

- Add a fading recent-layer breadcrumb

## [v0.6.0](https://github.com/pmaxhogan/starview/releases/tag/v0.6.0) — 2026-06-17

### Added

- Add accent color themes

## [v0.5.0](https://github.com/pmaxhogan/starview/releases/tag/v0.5.0) — 2026-06-17

### Added

- Add an adjustable overlay size (tray submenu)

## [v0.4.0](https://github.com/pmaxhogan/starview/releases/tag/v0.4.0) — 2026-06-17

### Added

- Add feature roadmap
- Add a "Reset key stats" tray item

## [v0.3.9](https://github.com/pmaxhogan/starview/releases/tag/v0.3.9) — 2026-06-17

### Changed

- Auto-compensate overlay opacity under HDR

## [v0.3.8](https://github.com/pmaxhogan/starview/releases/tag/v0.3.8) — 2026-06-17

### Changed

- Void typo tracking on mouse use or arrow keys

## [v0.3.7](https://github.com/pmaxhogan/starview/releases/tag/v0.3.7) — 2026-06-17

### Added

- Show version in the tray menu; make "Up to date" a manual check

## [v0.3.6](https://github.com/pmaxhogan/starview/releases/tag/v0.3.6) — 2026-06-16

### Added

- Add a typo heatmap from backspace usage

## [v0.3.5](https://github.com/pmaxhogan/starview/releases/tag/v0.3.5) — 2026-06-16

### Added

- Count key presses; add a tray-toggleable usage heatmap

## [v0.3.4](https://github.com/pmaxhogan/starview/releases/tag/v0.3.4) — 2026-06-14

### Changed

- Rainbow: one hue per press-moment, HSL for uniform brightness

## [v0.3.3](https://github.com/pmaxhogan/starview/releases/tag/v0.3.3) — 2026-06-14

### Added

- Add tray-toggleable rainbow mode for key ghosts

### Changed

- Lengthen key-ghost afterglow to ~3s

## [v0.3.2](https://github.com/pmaxhogan/starview/releases/tag/v0.3.2) — 2026-06-14

### Added

- Add a motion trail to the trackball visualizer

## [v0.3.1](https://github.com/pmaxhogan/starview/releases/tag/v0.3.1) — 2026-06-13

### Changed

- Make the panel fully opaque at 100% opacity

## [v0.3.0](https://github.com/pmaxhogan/starview/releases/tag/v0.3.0) — 2026-06-13

### Added

- Add an X close button to the overlay top bar

### Changed

- Update README.md
- Shift-drag hamburger handle to reposition the overlay
- Configurable overlay opacity from the tray menu
- Afterglow: released keys fade out instead of snapping off
- Enlarge shift/control glyphs and space/escape/enter labels
- Auto-hide: fade the overlay away after a configurable idle time
- Clamp overlay drag to monitor bounds
- Document tray opacity/auto-hide and the Shift-gated panel controls

## [v0.2.0](https://github.com/pmaxhogan/starview/releases/tag/v0.2.0) — 2026-06-11

### Added

- Add --always mode and base-layer fall-through labels
- Trackball motion indicator for the ZSA Navigator

### Changed

- Initial commit: layer overlay MVP for ZSA Moonlander
- v2: render the active layer's full keymap
- Strip invisible emoji plumbing from custom key labels
- Pressed-key highlights, rotated thumb clusters, periodic layout refresh
- Smooth the trackball dot: EMA toward motion, time-based decay
- Bigger trackball dot amplitude
- Tray icon with pin/corner settings, numbered layer header
- Pad the tray menu bottom for auto-hiding taskbars
- Trim tray menu padding to two blank rows
- Glow-color key borders and proper key symbols
- Slightly larger single letters and digits on keycaps
- Enlarge thumb cluster keys ~8%
- Release infrastructure: installer, CI releases, auto-updater
- High-contrast palette when the primary monitor is in HDR mode

### Fixed

- Fix stale-frame flash when a layer key is pressed

