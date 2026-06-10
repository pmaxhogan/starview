//! Physical key geometry for the ZSA Moonlander, indexed by ZSA Oryx's
//! `layers[].keys` array order.
//!
//! # Provenance
//!
//! * Key coordinates (`x`, `y`, `w`, `h`) are transcribed from QMK firmware,
//!   `keyboards/zsa/moonlander/keyboard.json` (`layouts.LAYOUT.layout[]`),
//!   branch `firmware24` of <https://github.com/zsa/qmk_firmware> (identical
//!   in qmk/qmk_firmware master). These are factual measurements of the
//!   physical board, not creative expression; no QMK code is copied.
//! * The Oryx-index -> physical-position mapping was derived empirically by
//!   diffing the Oryx GraphQL `layers[].keys` array against the `keymap.c`
//!   that Oryx itself generates for the same layout (layout `jmvGw`,
//!   `https://oryx.zsa.io/jmvGw/latest/source`), whose `LAYOUT_moonlander`
//!   arguments follow `keyboard.json` order. Verified against the Workman
//!   alphabet of that layout (Q D R W B / J F U P ; etc.).
//! * Oryx serializes keys per half: left half first (indices 0-35), then
//!   right half (36-71). Within a half: rows top-to-bottom, each row
//!   left-to-right (7, 7, 7, 6, 5 keys), then the big red thumb key, then
//!   the 3 piano thumb keys left-to-right.
//! * QMK draws the thumb clusters flat (no rotation data). The +/-30 degree
//!   cluster rotation matches Oryx's own renderer
//!   (`.moonlander .clusters .left-cluster { transform: rotate(30deg) }`,
//!   right cluster -30deg, in configure.zsa.io CSS). Each cluster is
//!   additionally translated +0.5u in `y` relative to QMK's flat layout so
//!   the rotated cluster clears the main key grid.
//! * Conventions (1u key units, top-left-corner positions, clockwise
//!   rotation) follow the MIT-licensed oryx-bench project
//!   (<https://github.com/Enriquefft/oryx-bench>, Copyright (c) 2026 Enrique
//!   Flores, MIT License); oryx-bench has no Moonlander table, so no
//!   geometry data was copied from it.

/// Position/size in key units (1u = one standard key). `x`,`y` are the key's
/// top-left corner BEFORE rotation; `rot_deg` rotates around (`rot_x`,`rot_y`).
///
/// Coordinates are screen-style: `x` grows right, `y` grows down. Positive
/// `rot_deg` is a clockwise rotation (CSS/QMK convention).
#[derive(Clone, Copy, Debug)]
pub struct KeyGeom {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rot_deg: f32,
    pub rot_x: f32,
    pub rot_y: f32,
}

/// One entry per Oryx key index.
pub const MOONLANDER_KEYS: &[KeyGeom] = &[
    // 0: L row 0 col 0
    KeyGeom { x: 0.0, y: 0.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 1: L row 0 col 1
    KeyGeom { x: 1.0, y: 0.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 2: L row 0 col 2
    KeyGeom { x: 2.0, y: 0.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 3: L row 0 col 3
    KeyGeom { x: 3.0, y: 0.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 4: L row 0 col 4
    KeyGeom { x: 4.0, y: 0.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 5: L row 0 col 5
    KeyGeom { x: 5.0, y: 0.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 6: L row 0 col 6
    KeyGeom { x: 6.0, y: 0.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 7: L row 1 col 0
    KeyGeom { x: 0.0, y: 1.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 8: L row 1 col 1
    KeyGeom { x: 1.0, y: 1.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 9: L row 1 col 2
    KeyGeom { x: 2.0, y: 1.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 10: L row 1 col 3
    KeyGeom { x: 3.0, y: 1.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 11: L row 1 col 4
    KeyGeom { x: 4.0, y: 1.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 12: L row 1 col 5
    KeyGeom { x: 5.0, y: 1.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 13: L row 1 col 6
    KeyGeom { x: 6.0, y: 1.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 14: L row 2 col 0
    KeyGeom { x: 0.0, y: 2.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 15: L row 2 col 1
    KeyGeom { x: 1.0, y: 2.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 16: L row 2 col 2
    KeyGeom { x: 2.0, y: 2.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 17: L row 2 col 3
    KeyGeom { x: 3.0, y: 2.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 18: L row 2 col 4
    KeyGeom { x: 4.0, y: 2.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 19: L row 2 col 5
    KeyGeom { x: 5.0, y: 2.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 20: L row 2 col 6
    KeyGeom { x: 6.0, y: 2.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 21: L row 3 col 0
    KeyGeom { x: 0.0, y: 3.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 22: L row 3 col 1
    KeyGeom { x: 1.0, y: 3.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 23: L row 3 col 2
    KeyGeom { x: 2.0, y: 3.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 24: L row 3 col 3
    KeyGeom { x: 3.0, y: 3.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 25: L row 3 col 4
    KeyGeom { x: 4.0, y: 3.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 26: L row 3 col 5
    KeyGeom { x: 5.0, y: 3.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 27: L row 4 col 0
    KeyGeom { x: 0.0, y: 4.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 28: L row 4 col 1
    KeyGeom { x: 1.0, y: 4.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 29: L row 4 col 2
    KeyGeom { x: 2.0, y: 4.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 30: L row 4 col 3
    KeyGeom { x: 3.0, y: 4.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 31: L row 4 col 4
    KeyGeom { x: 4.0, y: 4.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 32: L thumb: big red key
    KeyGeom { x: 5.0, y: 5.0, w: 2.0, h: 1.0, rot_deg: 30.0, rot_x: 6.5, rot_y: 6.25 },
    // 33: L thumb: piano key 0
    KeyGeom { x: 5.0, y: 6.0, w: 1.0, h: 1.5, rot_deg: 30.0, rot_x: 6.5, rot_y: 6.25 },
    // 34: L thumb: piano key 1
    KeyGeom { x: 6.0, y: 6.0, w: 1.0, h: 1.5, rot_deg: 30.0, rot_x: 6.5, rot_y: 6.25 },
    // 35: L thumb: piano key 2
    KeyGeom { x: 7.0, y: 6.0, w: 1.0, h: 1.5, rot_deg: 30.0, rot_x: 6.5, rot_y: 6.25 },
    // 36: R row 0 col 0
    KeyGeom { x: 10.0, y: 0.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 37: R row 0 col 1
    KeyGeom { x: 11.0, y: 0.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 38: R row 0 col 2
    KeyGeom { x: 12.0, y: 0.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 39: R row 0 col 3
    KeyGeom { x: 13.0, y: 0.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 40: R row 0 col 4
    KeyGeom { x: 14.0, y: 0.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 41: R row 0 col 5
    KeyGeom { x: 15.0, y: 0.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 42: R row 0 col 6
    KeyGeom { x: 16.0, y: 0.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 43: R row 1 col 0
    KeyGeom { x: 10.0, y: 1.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 44: R row 1 col 1
    KeyGeom { x: 11.0, y: 1.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 45: R row 1 col 2
    KeyGeom { x: 12.0, y: 1.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 46: R row 1 col 3
    KeyGeom { x: 13.0, y: 1.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 47: R row 1 col 4
    KeyGeom { x: 14.0, y: 1.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 48: R row 1 col 5
    KeyGeom { x: 15.0, y: 1.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 49: R row 1 col 6
    KeyGeom { x: 16.0, y: 1.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 50: R row 2 col 0
    KeyGeom { x: 10.0, y: 2.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 51: R row 2 col 1
    KeyGeom { x: 11.0, y: 2.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 52: R row 2 col 2
    KeyGeom { x: 12.0, y: 2.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 53: R row 2 col 3
    KeyGeom { x: 13.0, y: 2.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 54: R row 2 col 4
    KeyGeom { x: 14.0, y: 2.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 55: R row 2 col 5
    KeyGeom { x: 15.0, y: 2.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 56: R row 2 col 6
    KeyGeom { x: 16.0, y: 2.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 57: R row 3 col 0
    KeyGeom { x: 11.0, y: 3.25, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 58: R row 3 col 1
    KeyGeom { x: 12.0, y: 3.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 59: R row 3 col 2
    KeyGeom { x: 13.0, y: 3.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 60: R row 3 col 3
    KeyGeom { x: 14.0, y: 3.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 61: R row 3 col 4
    KeyGeom { x: 15.0, y: 3.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 62: R row 3 col 5
    KeyGeom { x: 16.0, y: 3.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 63: R row 4 col 0
    KeyGeom { x: 12.0, y: 4.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 64: R row 4 col 1
    KeyGeom { x: 13.0, y: 4.0, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 65: R row 4 col 2
    KeyGeom { x: 14.0, y: 4.125, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 66: R row 4 col 3
    KeyGeom { x: 15.0, y: 4.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 67: R row 4 col 4
    KeyGeom { x: 16.0, y: 4.375, w: 1.0, h: 1.0, rot_deg: 0.0, rot_x: 0.0, rot_y: 0.0 },
    // 68: R thumb: big red key
    KeyGeom { x: 10.0, y: 5.0, w: 2.0, h: 1.0, rot_deg: -30.0, rot_x: 10.5, rot_y: 6.25 },
    // 69: R thumb: piano key 0
    KeyGeom { x: 9.0, y: 6.0, w: 1.0, h: 1.5, rot_deg: -30.0, rot_x: 10.5, rot_y: 6.25 },
    // 70: R thumb: piano key 1
    KeyGeom { x: 10.0, y: 6.0, w: 1.0, h: 1.5, rot_deg: -30.0, rot_x: 10.5, rot_y: 6.25 },
    // 71: R thumb: piano key 2
    KeyGeom { x: 11.0, y: 6.0, w: 1.0, h: 1.5, rot_deg: -30.0, rot_x: 10.5, rot_y: 6.25 },
];

/// Bounding box of the whole board in key units (including rotated keys).
///
/// Width is set by the main grid: right half outer column at x = 16, 1u wide.
pub const BOARD_WIDTH_U: f32 = 17.0;
/// Height is set by the rotated piano keys: each cluster's bottom corner
/// reaches pivot_y + 1.5 * sin(30) + 1.25 * cos(30)
/// = 6.25 + 0.75 + 1.0825318 = 8.0825318; rounded up slightly.
pub const BOARD_HEIGHT_U: f32 = 8.0826;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_72_keys() {
        assert_eq!(MOONLANDER_KEYS.len(), 72);
    }

    #[test]
    fn all_keys_fit_in_board_box() {
        for k in MOONLANDER_KEYS {
            let corners = [
                (k.x, k.y),
                (k.x + k.w, k.y),
                (k.x, k.y + k.h),
                (k.x + k.w, k.y + k.h),
            ];
            let t = k.rot_deg.to_radians();
            let (s, c) = t.sin_cos();
            for (px, py) in corners {
                let (dx, dy) = (px - k.rot_x, py - k.rot_y);
                let (rx, ry) = if k.rot_deg == 0.0 {
                    (px, py)
                } else {
                    (k.rot_x + dx * c - dy * s, k.rot_y + dx * s + dy * c)
                };
                assert!(rx >= -1e-4 && rx <= BOARD_WIDTH_U + 1e-4, "x out of box: {rx}");
                assert!(ry >= -1e-4 && ry <= BOARD_HEIGHT_U + 1e-4, "y out of box: {ry}");
            }
        }
    }
}
