//! Short display labels for QMK keycodes as emitted by ZSA Oryx.
//!
//! Oryx mixes QMK long-form and short-form names (e.g. it emits `KC_PAGE_UP`
//! but `KC_PGDN`, `KC_COMMA` but `KC_DOT`), so both alias spellings are
//! accepted throughout.
//!
//! Layer-switch keys arrive as a bare code (`"MO"`, `"TG"`, `"TO"`, `"TT"`,
//! `"OSL"`, `"DF"`, `"LT"`) with the target layer in the separate `layer`
//! field of the action object — the renderer should append the layer number
//! to the label returned here (e.g. `"MO"` + `2` -> "MO 2").
//!
//! Modifier-wrapped keys (e.g. LGUI(KC_1)) arrive as the inner code plus a
//! `modifiers` object of booleans — the renderer should prefix the inner
//! label with the active modifiers (e.g. "G+1", "C+Bksp").
//!
//! The Oryx `"RGB"` code sets the underglow to a fixed color carried in the
//! action's `color` field; the renderer may show a color swatch next to the
//! "RGB" label.

/// Returns a short label for a QMK keycode string, or None if unknown.
/// KC_TRANSPARENT and KC_NO return Some("") (render as blank).
pub fn key_label(code: &str) -> Option<&'static str> {
    Some(match code {
        // ── blanks ───────────────────────────────────────────────────────
        "KC_TRANSPARENT" | "KC_TRNS" => "",
        "KC_NO" | "KC_NONE" => "",

        // ── letters ──────────────────────────────────────────────────────
        "KC_A" => "A",
        "KC_B" => "B",
        "KC_C" => "C",
        "KC_D" => "D",
        "KC_E" => "E",
        "KC_F" => "F",
        "KC_G" => "G",
        "KC_H" => "H",
        "KC_I" => "I",
        "KC_J" => "J",
        "KC_K" => "K",
        "KC_L" => "L",
        "KC_M" => "M",
        "KC_N" => "N",
        "KC_O" => "O",
        "KC_P" => "P",
        "KC_Q" => "Q",
        "KC_R" => "R",
        "KC_S" => "S",
        "KC_T" => "T",
        "KC_U" => "U",
        "KC_V" => "V",
        "KC_W" => "W",
        "KC_X" => "X",
        "KC_Y" => "Y",
        "KC_Z" => "Z",

        // ── digits ───────────────────────────────────────────────────────
        "KC_0" => "0",
        "KC_1" => "1",
        "KC_2" => "2",
        "KC_3" => "3",
        "KC_4" => "4",
        "KC_5" => "5",
        "KC_6" => "6",
        "KC_7" => "7",
        "KC_8" => "8",
        "KC_9" => "9",

        // ── function keys ────────────────────────────────────────────────
        "KC_F1" => "F1",
        "KC_F2" => "F2",
        "KC_F3" => "F3",
        "KC_F4" => "F4",
        "KC_F5" => "F5",
        "KC_F6" => "F6",
        "KC_F7" => "F7",
        "KC_F8" => "F8",
        "KC_F9" => "F9",
        "KC_F10" => "F10",
        "KC_F11" => "F11",
        "KC_F12" => "F12",
        "KC_F13" => "F13",
        "KC_F14" => "F14",
        "KC_F15" => "F15",
        "KC_F16" => "F16",
        "KC_F17" => "F17",
        "KC_F18" => "F18",
        "KC_F19" => "F19",
        "KC_F20" => "F20",
        "KC_F21" => "F21",
        "KC_F22" => "F22",
        "KC_F23" => "F23",
        "KC_F24" => "F24",

        // ── punctuation ──────────────────────────────────────────────────
        "KC_MINUS" | "KC_MINS" => "-",
        "KC_EQUAL" | "KC_EQL" => "=",
        "KC_LEFT_BRACKET" | "KC_LBRACKET" | "KC_LBRC" => "[",
        "KC_RIGHT_BRACKET" | "KC_RBRACKET" | "KC_RBRC" => "]",
        "KC_BACKSLASH" | "KC_BSLASH" | "KC_BSLS" => "\\",
        "KC_SEMICOLON" | "KC_SCOLON" | "KC_SCLN" => ";",
        "KC_QUOTE" | "KC_QUOT" => "'",
        "KC_GRAVE" | "KC_GRV" => "`",
        "KC_COMMA" | "KC_COMM" => ",",
        "KC_DOT" => ".",
        "KC_SLASH" | "KC_SLSH" => "/",
        "KC_NONUS_HASH" | "KC_NUHS" => "#",
        "KC_NONUS_BACKSLASH" | "KC_NONUS_BSLASH" | "KC_NUBS" => "\\",

        // ── shifted symbols ──────────────────────────────────────────────
        "KC_EXCLAIM" | "KC_EXLM" => "!",
        "KC_AT" => "@",
        "KC_HASH" => "#",
        "KC_DOLLAR" | "KC_DLR" => "$",
        "KC_PERCENT" | "KC_PERC" => "%",
        "KC_CIRCUMFLEX" | "KC_CIRC" => "^",
        "KC_AMPERSAND" | "KC_AMPR" => "&",
        "KC_ASTERISK" | "KC_ASTR" => "*",
        "KC_LEFT_PAREN" | "KC_LPRN" => "(",
        "KC_RIGHT_PAREN" | "KC_RPRN" => ")",
        "KC_UNDERSCORE" | "KC_UNDS" => "_",
        "KC_PLUS" => "+",
        "KC_LEFT_CURLY_BRACE" | "KC_LCBR" => "{",
        "KC_RIGHT_CURLY_BRACE" | "KC_RCBR" => "}",
        "KC_PIPE" => "|",
        "KC_COLON" | "KC_COLN" => ":",
        "KC_DOUBLE_QUOTE" | "KC_DQUO" | "KC_DQT" => "\"",
        "KC_LEFT_ANGLE_BRACKET" | "KC_LABK" | "KC_LT" => "<",
        "KC_RIGHT_ANGLE_BRACKET" | "KC_RABK" | "KC_GT" => ">",
        "KC_QUESTION" | "KC_QUES" => "?",
        "KC_TILDE" | "KC_TILD" => "~",

        // ── whitespace / editing ─────────────────────────────────────────
        "KC_SPACE" | "KC_SPC" => "Spc",
        "KC_ENTER" | "KC_ENT" => "Ent",
        "KC_BACKSPACE" | "KC_BSPACE" | "KC_BSPC" => "Bksp",
        "KC_DELETE" | "KC_DEL" => "Del",
        "KC_TAB" => "Tab",
        "KC_ESCAPE" | "KC_ESC" => "Esc",
        "KC_INSERT" | "KC_INS" => "Ins",
        "KC_CAPS_LOCK" | "KC_CAPSLOCK" | "KC_CAPS" => "Caps",
        "CW_TOGG" | "QK_CAPS_WORD_TOGGLE" => "CWord",
        "KC_UNDO" => "Undo",
        "KC_AGAIN" | "KC_AGIN" => "Redo",
        "KC_CUT" => "Cut",
        "KC_COPY" => "Copy",
        "KC_PASTE" | "KC_PSTE" => "Paste",
        "KC_FIND" => "Find",

        // ── modifiers ────────────────────────────────────────────────────
        "KC_LEFT_SHIFT" | "KC_LSHIFT" | "KC_LSFT" => "Sft",
        "KC_LEFT_CTRL" | "KC_LCTRL" | "KC_LCTL" => "Ctl",
        "KC_LEFT_ALT" | "KC_LALT" => "Alt",
        "KC_LEFT_GUI" | "KC_LGUI" | "KC_LCMD" | "KC_LWIN" => "Gui",
        "KC_RIGHT_SHIFT" | "KC_RSHIFT" | "KC_RSFT" => "RSft",
        "KC_RIGHT_CTRL" | "KC_RCTRL" | "KC_RCTL" => "RCtl",
        "KC_RIGHT_ALT" | "KC_RALT" | "KC_ALGR" => "RAlt",
        "KC_RIGHT_GUI" | "KC_RGUI" | "KC_RCMD" | "KC_RWIN" => "RGui",
        "KC_APPLICATION" | "KC_APP" => "Menu",
        "KC_MENU" => "Menu",
        "KC_HYPR" | "ALL_T" => "Hypr",
        "KC_MEH" | "MEH_T" => "Meh",
        "OSM" => "OSM", // one-shot modifier; renderer appends the `modifier` field

        // ── layer switching (renderer appends the `layer` field) ─────────
        "MO" => "MO",
        "TG" => "TG",
        "TO" => "TO",
        "TT" => "TT",
        "OSL" => "OSL",
        "DF" => "DF",
        "PDF" => "PDF",
        "LT" => "LT",
        "LLOCK" | "QK_LLCK" | "QK_LAYER_LOCK" => "LLock",

        // ── navigation ───────────────────────────────────────────────────
        "KC_LEFT" => "\u{2190}",  // ←
        "KC_UP" => "\u{2191}",    // ↑
        "KC_RIGHT" => "\u{2192}", // →
        "KC_DOWN" => "\u{2193}",  // ↓
        "KC_HOME" => "Home",
        "KC_END" => "End",
        "KC_PAGE_UP" | "KC_PGUP" => "PgUp",
        "KC_PAGE_DOWN" | "KC_PGDOWN" | "KC_PGDN" => "PgDn",
        "KC_PRINT_SCREEN" | "KC_PSCREEN" | "KC_PSCR" => "PrSc",
        "KC_SCROLL_LOCK" | "KC_SCROLLLOCK" | "KC_SCRL" | "KC_SLCK" => "ScrLk",
        "KC_PAUSE" | "KC_PAUS" | "KC_BRK" => "Pause",
        "KC_NUM_LOCK" | "KC_NUMLOCK" | "KC_NUM" | "KC_NLCK" => "Num",

        // ── media / audio ────────────────────────────────────────────────
        "KC_AUDIO_VOL_UP" | "KC_KB_VOLUME_UP" | "KC_VOLU" => "Vol+",
        "KC_AUDIO_VOL_DOWN" | "KC_KB_VOLUME_DOWN" | "KC_VOLD" => "Vol-",
        "KC_AUDIO_MUTE" | "KC_KB_MUTE" | "KC_MUTE" => "Mute",
        "KC_MEDIA_PLAY_PAUSE" | "KC_MPLY" => "Play",
        "KC_MEDIA_NEXT_TRACK" | "KC_MNXT" => "Next",
        "KC_MEDIA_PREV_TRACK" | "KC_MPRV" => "Prev",
        "KC_MEDIA_STOP" | "KC_MSTP" => "Stop",
        "KC_MEDIA_SELECT" | "KC_MSEL" => "Media",
        "KC_MEDIA_EJECT" | "KC_EJCT" => "Eject",
        "KC_MEDIA_FAST_FORWARD" | "KC_MFFD" => "FFwd",
        "KC_MEDIA_REWIND" | "KC_MRWD" => "Rwd",
        "KC_BRIGHTNESS_UP" | "KC_BRIU" => "Bri+",
        "KC_BRIGHTNESS_DOWN" | "KC_BRID" => "Bri-",

        // ── browser / desktop / system ───────────────────────────────────
        "KC_WWW_BACK" | "KC_WBAK" => "Back",
        "KC_WWW_FORWARD" | "KC_WFWD" => "Fwd",
        "KC_WWW_HOME" | "KC_WHOM" => "WWW",
        "KC_WWW_REFRESH" | "KC_WREF" => "Refr",
        "KC_WWW_SEARCH" | "KC_WSCH" => "Srch",
        "KC_WWW_STOP" | "KC_WSTP" => "WStop",
        "KC_WWW_FAVORITES" | "KC_WFAV" => "Favs",
        "KC_MAIL" => "Mail",
        "KC_CALCULATOR" | "KC_CALC" => "Calc",
        "KC_MY_COMPUTER" | "KC_MYCM" => "MyPC",
        "KC_SYSTEM_POWER" | "KC_PWR" => "Power",
        "KC_SYSTEM_SLEEP" | "KC_SLEP" => "Sleep",
        "KC_SYSTEM_WAKE" | "KC_WAKE" => "Wake",

        // ── numpad ───────────────────────────────────────────────────────
        "KC_KP_0" | "KC_P0" => "KP0",
        "KC_KP_1" | "KC_P1" => "KP1",
        "KC_KP_2" | "KC_P2" => "KP2",
        "KC_KP_3" | "KC_P3" => "KP3",
        "KC_KP_4" | "KC_P4" => "KP4",
        "KC_KP_5" | "KC_P5" => "KP5",
        "KC_KP_6" | "KC_P6" => "KP6",
        "KC_KP_7" | "KC_P7" => "KP7",
        "KC_KP_8" | "KC_P8" => "KP8",
        "KC_KP_9" | "KC_P9" => "KP9",
        "KC_KP_DOT" | "KC_PDOT" => "KP.",
        "KC_KP_COMMA" | "KC_PCMM" => "KP,",
        "KC_KP_SLASH" | "KC_PSLS" => "KP/",
        "KC_KP_ASTERISK" | "KC_PAST" => "KP*",
        "KC_KP_MINUS" | "KC_PMNS" => "KP-",
        "KC_KP_PLUS" | "KC_PPLS" => "KP+",
        "KC_KP_EQUAL" | "KC_PEQL" => "KP=",
        "KC_KP_ENTER" | "KC_PENT" => "KPEnt",

        // ── mouse keys ───────────────────────────────────────────────────
        "KC_MS_BTN1" | "KC_BTN1" => "M1",
        "KC_MS_BTN2" | "KC_BTN2" => "M2",
        "KC_MS_BTN3" | "KC_BTN3" => "M3",
        "KC_MS_BTN4" | "KC_BTN4" => "M4",
        "KC_MS_BTN5" | "KC_BTN5" => "M5",
        "KC_MS_UP" | "KC_MS_U" => "Ms\u{2191}",
        "KC_MS_DOWN" | "KC_MS_D" => "Ms\u{2193}",
        "KC_MS_LEFT" | "KC_MS_L" => "Ms\u{2190}",
        "KC_MS_RIGHT" | "KC_MS_R" => "Ms\u{2192}",
        "KC_MS_WH_UP" | "KC_WH_U" => "Wh\u{2191}",
        "KC_MS_WH_DOWN" | "KC_WH_D" => "Wh\u{2193}",
        "KC_MS_WH_LEFT" | "KC_WH_L" => "Wh\u{2190}",
        "KC_MS_WH_RIGHT" | "KC_WH_R" => "Wh\u{2192}",
        "KC_MS_ACCEL0" | "KC_ACL0" => "Acc0",
        "KC_MS_ACCEL1" | "KC_ACL1" => "Acc1",
        "KC_MS_ACCEL2" | "KC_ACL2" => "Acc2",
        "DRAG_SCROLL" => "Drag",
        "NAVIGATOR_TURBO" => "Turbo",

        // ── RGB / lighting ───────────────────────────────────────────────
        "RGB_TOG" => "RGB",
        "RGB" => "RGB", // Oryx "set color" key; color hex is in the action's `color` field
        "RGB_MODE_FORWARD" | "RGB_MOD" => "Anim+",
        "RGB_MODE_REVERSE" | "RGB_RMOD" => "Anim-",
        "RGB_HUI" => "Hue+",
        "RGB_HUD" => "Hue-",
        "RGB_SAI" => "Sat+",
        "RGB_SAD" => "Sat-",
        "RGB_VAI" => "Val+",
        "RGB_VAD" => "Val-",
        "RGB_SPI" => "Spd+",
        "RGB_SPD" => "Spd-",
        "RGB_SLD" => "Solid",
        "TOGGLE_LAYER_COLOR" => "LrRGB",
        "LED_LEVEL" => "LED",

        // ── firmware / Oryx specials ─────────────────────────────────────
        "RESET" | "QK_BOOT" => "Boot",
        "EE_CLR" | "EEP_RST" => "EEClr",
        "DEBUG" | "QK_DEBUG_TOGGLE" => "Debug",
        "WEBUSB_PAIR" => "Pair",
        "MAGIC_TOGGLE_NKRO" | "NK_TOGG" => "NKRO",
        "DYN_REC_START1" => "Rec1",
        "DYN_REC_START2" => "Rec2",
        "DYN_REC_STOP" => "RStop",
        "DYN_MACRO_PLAY1" => "Play1",
        "DYN_MACRO_PLAY2" => "Play2",
        "AU_TOG" => "Audio",
        "MU_TOG" => "Music",
        "MU_MOD" => "MusMd",

        _ => return None,
    })
}
