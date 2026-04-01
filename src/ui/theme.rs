/// HyperLite Theme System
///
/// All themes use a consistent 20-token color palette.
/// Default theme: "cyberpunk" — deep space blacks, electric teal, neon green, lavender purple.

use ratatui::style::Color;
use once_cell::sync::Lazy;
use std::collections::HashMap;

// ── Color helper ──────────────────────────────────────────────────────────────

const fn rgb(r: u8, g: u8, b: u8) -> Color { Color::Rgb(r, g, b) }

// ── Theme struct ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,

    // Backgrounds
    pub bg:          Color,   // main background
    pub bg_panel:    Color,   // message/dialog panels
    pub bg_element:  Color,   // interactive elements, hover
    pub bg_menu:     Color,   // dropdown / menu overlay

    // Text
    pub text:        Color,   // primary readable text
    pub text_muted:  Color,   // labels, timestamps, hints
    pub text_dim:    Color,   // very subtle — reasoning text

    // Accents
    pub primary:     Color,   // main accent (purple in cyberpunk)
    pub secondary:   Color,   // secondary accent
    pub accent:      Color,   // highlight (teal in cyberpunk)

    // Semantic
    pub success:     Color,   // neon green
    pub error:       Color,   // red/pink
    pub warning:     Color,   // orange

    // Borders
    pub border:      Color,   // inactive border
    pub border_hi:   Color,   // focused / active border

    // Diff
    pub diff_add_bg:    Color,
    pub diff_del_bg:    Color,
    pub diff_ctx_bg:    Color,
    pub diff_add_fg:    Color,
    pub diff_del_fg:    Color,
    pub diff_ln_fg:     Color,

    // Agent colors — assigned round-robin to sessions
    pub agents: [Color; 8],
}

impl Theme {
    /// Pick an agent color by session index
    pub fn agent_color(&self, idx: usize) -> Color {
        self.agents[idx % self.agents.len()]
    }
}

// ── CYBERPUNK (default) ───────────────────────────────────────────────────────
// Deep space blacks + electric teal + neon green + lavender purple

pub static CYBERPUNK: Theme = Theme {
    name: "cyberpunk",

    bg:         rgb(10,  10,  20),   // #0A0A14 deep space
    bg_panel:   rgb(16,  16,  30),   // #10101E panels
    bg_element: rgb(26,  26,  48),   // #1A1A30 interactive
    bg_menu:    rgb(13,  13,  31),   // #0D0D1F menus

    text:       rgb(224, 223, 255),  // #E0DFFF soft lavender-white
    text_muted: rgb(90,  90,  138),  // #5A5A8A muted purple-gray
    text_dim:   rgb(55,  55,  90),   // #37375A very dim

    primary:    rgb(189, 147, 249),  // #BD93F9 lavender purple
    secondary:  rgb(98,  114, 164),  // #6272A4 steel blue
    accent:     rgb(0,   245, 212),  // #00F5D4 electric teal

    success:    rgb(80,  250, 123),  // #50FA7B neon green
    error:      rgb(255, 85,  85),   // #FF5555 cyber red
    warning:    rgb(255, 184, 108),  // #FFB86C amber

    border:     rgb(45,  45,  85),   // #2D2D55 dark purple border
    border_hi:  rgb(189, 147, 249),  // #BD93F9 active = primary

    diff_add_bg:  rgb(15,  50,  30),
    diff_del_bg:  rgb(50,  15,  20),
    diff_ctx_bg:  rgb(16,  16,  30),
    diff_add_fg:  rgb(80,  250, 123),
    diff_del_fg:  rgb(255, 85,  85),
    diff_ln_fg:   rgb(90,  90,  138),

    agents: [
        rgb(189, 147, 249), // purple
        rgb(0,   245, 212), // teal
        rgb(80,  250, 123), // green
        rgb(255, 121, 198), // pink
        rgb(241, 250, 140), // yellow
        rgb(139, 233, 253), // sky blue
        rgb(255, 184, 108), // orange
        rgb(98,  114, 164), // steel blue
    ],
};

// ── DRACULA ───────────────────────────────────────────────────────────────────

pub static DRACULA: Theme = Theme {
    name: "dracula",
    bg:         rgb(40,  42,  54),
    bg_panel:   rgb(50,  52,  65),
    bg_element: rgb(68,  71,  90),
    bg_menu:    rgb(33,  34,  44),
    text:       rgb(248, 248, 242),
    text_muted: rgb(98,  114, 164),
    text_dim:   rgb(68,  71,  90),
    primary:    rgb(189, 147, 249),
    secondary:  rgb(98,  114, 164),
    accent:     rgb(139, 233, 253),
    success:    rgb(80,  250, 123),
    error:      rgb(255, 85,  85),
    warning:    rgb(255, 184, 108),
    border:     rgb(68,  71,  90),
    border_hi:  rgb(189, 147, 249),
    diff_add_bg:  rgb(25,  60,  40),
    diff_del_bg:  rgb(60,  25,  30),
    diff_ctx_bg:  rgb(50,  52,  65),
    diff_add_fg:  rgb(80,  250, 123),
    diff_del_fg:  rgb(255, 85,  85),
    diff_ln_fg:   rgb(98,  114, 164),
    agents: [
        rgb(189, 147, 249),
        rgb(139, 233, 253),
        rgb(80,  250, 123),
        rgb(255, 121, 198),
        rgb(241, 250, 140),
        rgb(255, 184, 108),
        rgb(98,  114, 164),
        rgb(255, 85,  85),
    ],
};

// ── TOKYONIGHT ────────────────────────────────────────────────────────────────

pub static TOKYONIGHT: Theme = Theme {
    name: "tokyonight",
    bg:         rgb(26,  27,  38),
    bg_panel:   rgb(31,  35,  53),
    bg_element: rgb(41,  46,  66),
    bg_menu:    rgb(22,  22,  30),
    text:       rgb(192, 202, 245),
    text_muted: rgb(86,  95,  137),
    text_dim:   rgb(60,  68,  100),
    primary:    rgb(122, 162, 247),
    secondary:  rgb(86,  95,  137),
    accent:     rgb(125, 207, 255),
    success:    rgb(158, 206, 106),
    error:      rgb(247, 118, 142),
    warning:    rgb(224, 175, 104),
    border:     rgb(41,  46,  66),
    border_hi:  rgb(122, 162, 247),
    diff_add_bg:  rgb(20,  50,  30),
    diff_del_bg:  rgb(55,  20,  30),
    diff_ctx_bg:  rgb(31,  35,  53),
    diff_add_fg:  rgb(158, 206, 106),
    diff_del_fg:  rgb(247, 118, 142),
    diff_ln_fg:   rgb(86,  95,  137),
    agents: [
        rgb(122, 162, 247),
        rgb(125, 207, 255),
        rgb(158, 206, 106),
        rgb(247, 118, 142),
        rgb(224, 175, 104),
        rgb(187, 154, 247),
        rgb(86,  95,  137),
        rgb(255, 158, 100),
    ],
};

// ── CATPPUCCIN MOCHA ──────────────────────────────────────────────────────────

pub static CATPPUCCIN: Theme = Theme {
    name: "catppuccin",
    bg:         rgb(30,  30,  46),
    bg_panel:   rgb(36,  36,  54),
    bg_element: rgb(49,  50,  68),
    bg_menu:    rgb(24,  24,  37),
    text:       rgb(205, 214, 244),
    text_muted: rgb(108, 112, 134),
    text_dim:   rgb(88,  91,  112),
    primary:    rgb(203, 166, 247),
    secondary:  rgb(108, 112, 134),
    accent:     rgb(137, 220, 235),
    success:    rgb(166, 227, 161),
    error:      rgb(243, 139, 168),
    warning:    rgb(250, 179, 135),
    border:     rgb(49,  50,  68),
    border_hi:  rgb(203, 166, 247),
    diff_add_bg:  rgb(20,  48,  35),
    diff_del_bg:  rgb(55,  20,  30),
    diff_ctx_bg:  rgb(36,  36,  54),
    diff_add_fg:  rgb(166, 227, 161),
    diff_del_fg:  rgb(243, 139, 168),
    diff_ln_fg:   rgb(108, 112, 134),
    agents: [
        rgb(203, 166, 247),
        rgb(137, 220, 235),
        rgb(166, 227, 161),
        rgb(243, 139, 168),
        rgb(249, 226, 175),
        rgb(250, 179, 135),
        rgb(108, 112, 134),
        rgb(180, 190, 254),
    ],
};

// ── NORD ─────────────────────────────────────────────────────────────────────

pub static NORD: Theme = Theme {
    name: "nord",
    bg:         rgb(46,  52,  64),
    bg_panel:   rgb(59,  66,  82),
    bg_element: rgb(67,  76,  94),
    bg_menu:    rgb(36,  41,  51),
    text:       rgb(236, 239, 244),
    text_muted: rgb(129, 161, 193),
    text_dim:   rgb(76,  86,  106),
    primary:    rgb(136, 192, 208),
    secondary:  rgb(129, 161, 193),
    accent:     rgb(143, 188, 187),
    success:    rgb(163, 190, 140),
    error:      rgb(191, 97,  106),
    warning:    rgb(235, 203, 139),
    border:     rgb(67,  76,  94),
    border_hi:  rgb(136, 192, 208),
    diff_add_bg:  rgb(38,  55,  45),
    diff_del_bg:  rgb(60,  40,  45),
    diff_ctx_bg:  rgb(59,  66,  82),
    diff_add_fg:  rgb(163, 190, 140),
    diff_del_fg:  rgb(191, 97,  106),
    diff_ln_fg:   rgb(129, 161, 193),
    agents: [
        rgb(136, 192, 208),
        rgb(143, 188, 187),
        rgb(163, 190, 140),
        rgb(208, 135, 112),
        rgb(235, 203, 139),
        rgb(180, 142, 173),
        rgb(191, 97,  106),
        rgb(129, 161, 193),
    ],
};

// ── GRUVBOX ───────────────────────────────────────────────────────────────────

pub static GRUVBOX: Theme = Theme {
    name: "gruvbox",
    bg:         rgb(40,  40,  40),
    bg_panel:   rgb(50,  48,  47),
    bg_element: rgb(60,  56,  54),
    bg_menu:    rgb(29,  32,  33),
    text:       rgb(235, 219, 178),
    text_muted: rgb(146, 131, 116),
    text_dim:   rgb(102, 92,  84),
    primary:    rgb(131, 165, 152),
    secondary:  rgb(146, 131, 116),
    accent:     rgb(142, 192, 124),
    success:    rgb(184, 187, 38),
    error:      rgb(204, 36,  29),
    warning:    rgb(215, 153, 33),
    border:     rgb(80,  73,  69),
    border_hi:  rgb(131, 165, 152),
    diff_add_bg:  rgb(32,  55,  25),
    diff_del_bg:  rgb(60,  25,  25),
    diff_ctx_bg:  rgb(50,  48,  47),
    diff_add_fg:  rgb(184, 187, 38),
    diff_del_fg:  rgb(204, 36,  29),
    diff_ln_fg:   rgb(146, 131, 116),
    agents: [
        rgb(131, 165, 152),
        rgb(142, 192, 124),
        rgb(250, 189, 47),
        rgb(251, 73,  52),
        rgb(211, 134, 155),
        rgb(254, 128, 25),
        rgb(146, 131, 116),
        rgb(184, 187, 38),
    ],
};

// ── MONOKAI ───────────────────────────────────────────────────────────────────

pub static MONOKAI: Theme = Theme {
    name: "monokai",
    bg:         rgb(39,  40,  34),
    bg_panel:   rgb(50,  51,  44),
    bg_element: rgb(62,  63,  55),
    bg_menu:    rgb(32,  32,  27),
    text:       rgb(248, 248, 242),
    text_muted: rgb(117, 113, 94),
    text_dim:   rgb(80,  78,  65),
    primary:    rgb(174, 129, 255),
    secondary:  rgb(117, 113, 94),
    accent:     rgb(102, 217, 239),
    success:    rgb(166, 226, 46),
    error:      rgb(249, 38,  114),
    warning:    rgb(253, 151, 31),
    border:     rgb(62,  63,  55),
    border_hi:  rgb(174, 129, 255),
    diff_add_bg:  rgb(25,  50,  20),
    diff_del_bg:  rgb(55,  20,  30),
    diff_ctx_bg:  rgb(50,  51,  44),
    diff_add_fg:  rgb(166, 226, 46),
    diff_del_fg:  rgb(249, 38,  114),
    diff_ln_fg:   rgb(117, 113, 94),
    agents: [
        rgb(174, 129, 255),
        rgb(102, 217, 239),
        rgb(166, 226, 46),
        rgb(249, 38,  114),
        rgb(253, 151, 31),
        rgb(230, 219, 116),
        rgb(117, 113, 94),
        rgb(102, 217, 239),
    ],
};

// ── ONE DARK ──────────────────────────────────────────────────────────────────

pub static ONE_DARK: Theme = Theme {
    name: "one-dark",
    bg:         rgb(40,  44,  52),
    bg_panel:   rgb(49,  53,  61),
    bg_element: rgb(59,  64,  72),
    bg_menu:    rgb(33,  37,  43),
    text:       rgb(171, 178, 191),
    text_muted: rgb(92,  99,  112),
    text_dim:   rgb(64,  70,  80),
    primary:    rgb(198, 120, 221),
    secondary:  rgb(92,  99,  112),
    accent:     rgb(86,  182, 194),
    success:    rgb(152, 195, 121),
    error:      rgb(224, 108, 117),
    warning:    rgb(229, 192, 123),
    border:     rgb(59,  64,  72),
    border_hi:  rgb(198, 120, 221),
    diff_add_bg:  rgb(22,  48,  28),
    diff_del_bg:  rgb(52,  22,  28),
    diff_ctx_bg:  rgb(49,  53,  61),
    diff_add_fg:  rgb(152, 195, 121),
    diff_del_fg:  rgb(224, 108, 117),
    diff_ln_fg:   rgb(92,  99,  112),
    agents: [
        rgb(198, 120, 221),
        rgb(86,  182, 194),
        rgb(152, 195, 121),
        rgb(224, 108, 117),
        rgb(229, 192, 123),
        rgb(97,  175, 239),
        rgb(92,  99,  112),
        rgb(209, 154, 102),
    ],
};

// ── SYNTHWAVE84 ───────────────────────────────────────────────────────────────

pub static SYNTHWAVE84: Theme = Theme {
    name: "synthwave84",
    bg:         rgb(26,  14,  38),
    bg_panel:   rgb(35,  20,  50),
    bg_element: rgb(50,  30,  70),
    bg_menu:    rgb(20,  10,  30),
    text:       rgb(230, 220, 255),
    text_muted: rgb(140, 100, 180),
    text_dim:   rgb(90,  60,  120),
    primary:    rgb(255, 110, 253),  // hot pink/magenta
    secondary:  rgb(140, 100, 180),
    accent:     rgb(54,  255, 253),  // electric cyan
    success:    rgb(114, 241, 168),  // mint green
    error:      rgb(255, 70,  100),
    warning:    rgb(255, 210, 100),
    border:     rgb(80,  40,  100),
    border_hi:  rgb(255, 110, 253),
    diff_add_bg:  rgb(15,  55,  35),
    diff_del_bg:  rgb(60,  15,  30),
    diff_ctx_bg:  rgb(35,  20,  50),
    diff_add_fg:  rgb(114, 241, 168),
    diff_del_fg:  rgb(255, 70,  100),
    diff_ln_fg:   rgb(140, 100, 180),
    agents: [
        rgb(255, 110, 253),
        rgb(54,  255, 253),
        rgb(114, 241, 168),
        rgb(255, 70,  100),
        rgb(255, 210, 100),
        rgb(180, 100, 255),
        rgb(255, 165, 50),
        rgb(140, 100, 180),
    ],
};

// ── MATRIX ────────────────────────────────────────────────────────────────────

pub static MATRIX: Theme = Theme {
    name: "matrix",
    bg:         rgb(0,   10,  0),
    bg_panel:   rgb(0,   20,  0),
    bg_element: rgb(0,   35,  0),
    bg_menu:    rgb(0,   8,   0),
    text:       rgb(0,   255, 65),   // classic matrix green
    text_muted: rgb(0,   140, 35),
    text_dim:   rgb(0,   80,  20),
    primary:    rgb(0,   255, 65),
    secondary:  rgb(0,   180, 45),
    accent:     rgb(150, 255, 150),
    success:    rgb(0,   255, 65),
    error:      rgb(255, 50,  50),
    warning:    rgb(200, 200, 0),
    border:     rgb(0,   60,  15),
    border_hi:  rgb(0,   255, 65),
    diff_add_bg:  rgb(0,   40,  10),
    diff_del_bg:  rgb(40,  10,  0),
    diff_ctx_bg:  rgb(0,   20,  0),
    diff_add_fg:  rgb(0,   255, 65),
    diff_del_fg:  rgb(255, 50,  50),
    diff_ln_fg:   rgb(0,   140, 35),
    agents: [
        rgb(0,   255, 65),
        rgb(150, 255, 150),
        rgb(0,   200, 100),
        rgb(100, 255, 50),
        rgb(0,   180, 180),
        rgb(200, 255, 0),
        rgb(0,   140, 35),
        rgb(80,  255, 120),
    ],
};

// ── ROSEPINE ──────────────────────────────────────────────────────────────────

pub static ROSEPINE: Theme = Theme {
    name: "rosepine",
    bg:         rgb(25,  23,  36),
    bg_panel:   rgb(31,  29,  46),
    bg_element: rgb(38,  35,  58),
    bg_menu:    rgb(20,  18,  30),
    text:       rgb(224, 222, 244),
    text_muted: rgb(110, 106, 134),
    text_dim:   rgb(78,  75,  97),
    primary:    rgb(196, 167, 231),
    secondary:  rgb(110, 106, 134),
    accent:     rgb(235, 188, 186),
    success:    rgb(156, 207, 216),
    error:      rgb(235, 111, 146),
    warning:    rgb(246, 193, 119),
    border:     rgb(64,  61,  82),
    border_hi:  rgb(196, 167, 231),
    diff_add_bg:  rgb(15,  45,  40),
    diff_del_bg:  rgb(55,  20,  35),
    diff_ctx_bg:  rgb(31,  29,  46),
    diff_add_fg:  rgb(156, 207, 216),
    diff_del_fg:  rgb(235, 111, 146),
    diff_ln_fg:   rgb(110, 106, 134),
    agents: [
        rgb(196, 167, 231),
        rgb(235, 188, 186),
        rgb(156, 207, 216),
        rgb(235, 111, 146),
        rgb(246, 193, 119),
        rgb(144, 122, 169),
        rgb(110, 106, 134),
        rgb(235, 157, 186),
    ],
};

// ── EVERFOREST ────────────────────────────────────────────────────────────────

pub static EVERFOREST: Theme = Theme {
    name: "everforest",
    bg:         rgb(35,  42,  41),
    bg_panel:   rgb(45,  53,  51),
    bg_element: rgb(58,  69,  66),
    bg_menu:    rgb(29,  35,  34),
    text:       rgb(211, 198, 170),
    text_muted: rgb(127, 143, 136),
    text_dim:   rgb(90,  106, 100),
    primary:    rgb(131, 192, 146),
    secondary:  rgb(127, 143, 136),
    accent:     rgb(125, 196, 228),
    success:    rgb(167, 192, 128),
    error:      rgb(230, 126, 128),
    warning:    rgb(219, 188, 127),
    border:     rgb(80,  94,  90),
    border_hi:  rgb(131, 192, 146),
    diff_add_bg:  rgb(25,  55,  35),
    diff_del_bg:  rgb(60,  28,  30),
    diff_ctx_bg:  rgb(45,  53,  51),
    diff_add_fg:  rgb(167, 192, 128),
    diff_del_fg:  rgb(230, 126, 128),
    diff_ln_fg:   rgb(127, 143, 136),
    agents: [
        rgb(131, 192, 146),
        rgb(125, 196, 228),
        rgb(167, 192, 128),
        rgb(230, 126, 128),
        rgb(219, 188, 127),
        rgb(214, 153, 182),
        rgb(127, 143, 136),
        rgb(223, 167, 105),
    ],
};

// ── SOLARIZED ─────────────────────────────────────────────────────────────────

pub static SOLARIZED: Theme = Theme {
    name: "solarized",
    bg:         rgb(0,   43,  54),
    bg_panel:   rgb(7,   54,  66),
    bg_element: rgb(0,   72,  89),
    bg_menu:    rgb(0,   33,  43),
    text:       rgb(131, 148, 150),
    text_muted: rgb(88,  110, 117),
    text_dim:   rgb(58,  80,  87),
    primary:    rgb(108, 113, 196),
    secondary:  rgb(88,  110, 117),
    accent:     rgb(42,  161, 152),
    success:    rgb(133, 153, 0),
    error:      rgb(220, 50,  47),
    warning:    rgb(181, 137, 0),
    border:     rgb(0,   72,  89),
    border_hi:  rgb(108, 113, 196),
    diff_add_bg:  rgb(0,   40,  25),
    diff_del_bg:  rgb(50,  15,  15),
    diff_ctx_bg:  rgb(7,   54,  66),
    diff_add_fg:  rgb(133, 153, 0),
    diff_del_fg:  rgb(220, 50,  47),
    diff_ln_fg:   rgb(88,  110, 117),
    agents: [
        rgb(108, 113, 196),
        rgb(42,  161, 152),
        rgb(133, 153, 0),
        rgb(220, 50,  47),
        rgb(181, 137, 0),
        rgb(38,  139, 210),
        rgb(211, 54,  130),
        rgb(88,  110, 117),
    ],
};

// ── KANAGAWA ──────────────────────────────────────────────────────────────────

pub static KANAGAWA: Theme = Theme {
    name: "kanagawa",
    bg:         rgb(22,  22,  29),
    bg_panel:   rgb(31,  31,  40),
    bg_element: rgb(47,  47,  65),
    bg_menu:    rgb(16,  16,  23),
    text:       rgb(220, 215, 186),
    text_muted: rgb(113, 112, 96),
    text_dim:   rgb(84,  84,  109),
    primary:    rgb(127, 180, 202),
    secondary:  rgb(113, 112, 96),
    accent:     rgb(118, 148, 166),
    success:    rgb(106, 153, 85),
    error:      rgb(195, 64,  67),
    warning:    rgb(194, 168, 90),
    border:     rgb(54,  54,  74),
    border_hi:  rgb(127, 180, 202),
    diff_add_bg:  rgb(15,  45,  25),
    diff_del_bg:  rgb(55,  18,  20),
    diff_ctx_bg:  rgb(31,  31,  40),
    diff_add_fg:  rgb(106, 153, 85),
    diff_del_fg:  rgb(195, 64,  67),
    diff_ln_fg:   rgb(113, 112, 96),
    agents: [
        rgb(127, 180, 202),
        rgb(118, 148, 166),
        rgb(106, 153, 85),
        rgb(195, 64,  67),
        rgb(194, 168, 90),
        rgb(149, 127, 184),
        rgb(113, 112, 96),
        rgb(210, 126, 153),
    ],
};

// ── VESPER ────────────────────────────────────────────────────────────────────

pub static VESPER: Theme = Theme {
    name: "vesper",
    bg:         rgb(17,  17,  17),
    bg_panel:   rgb(24,  24,  24),
    bg_element: rgb(35,  35,  35),
    bg_menu:    rgb(12,  12,  12),
    text:       rgb(197, 200, 198),
    text_muted: rgb(90,  93,  91),
    text_dim:   rgb(60,  63,  61),
    primary:    rgb(255, 188, 100),
    secondary:  rgb(120, 120, 120),
    accent:     rgb(100, 200, 160),
    success:    rgb(130, 200, 100),
    error:      rgb(210, 90,  90),
    warning:    rgb(210, 170, 80),
    border:     rgb(50,  50,  50),
    border_hi:  rgb(255, 188, 100),
    diff_add_bg:  rgb(20,  45,  25),
    diff_del_bg:  rgb(50,  18,  18),
    diff_ctx_bg:  rgb(24,  24,  24),
    diff_add_fg:  rgb(130, 200, 100),
    diff_del_fg:  rgb(210, 90,  90),
    diff_ln_fg:   rgb(90,  93,  91),
    agents: [
        rgb(255, 188, 100),
        rgb(100, 200, 160),
        rgb(130, 200, 100),
        rgb(210, 90,  90),
        rgb(180, 130, 220),
        rgb(100, 180, 220),
        rgb(210, 170, 80),
        rgb(120, 120, 120),
    ],
};

// ── AURA ──────────────────────────────────────────────────────────────────────

pub static AURA: Theme = Theme {
    name: "aura",
    bg:         rgb(21,  18,  38),
    bg_panel:   rgb(28,  24,  50),
    bg_element: rgb(40,  35,  68),
    bg_menu:    rgb(16,  14,  30),
    text:       rgb(237, 233, 254),
    text_muted: rgb(110, 100, 150),
    text_dim:   rgb(75,  68,  110),
    primary:    rgb(162, 140, 255),
    secondary:  rgb(110, 100, 150),
    accent:     rgb(96,  220, 220),
    success:    rgb(97,  230, 159),
    error:      rgb(255, 100, 130),
    warning:    rgb(255, 200, 100),
    border:     rgb(60,  50,  100),
    border_hi:  rgb(162, 140, 255),
    diff_add_bg:  rgb(15,  50,  35),
    diff_del_bg:  rgb(55,  15,  30),
    diff_ctx_bg:  rgb(28,  24,  50),
    diff_add_fg:  rgb(97,  230, 159),
    diff_del_fg:  rgb(255, 100, 130),
    diff_ln_fg:   rgb(110, 100, 150),
    agents: [
        rgb(162, 140, 255),
        rgb(96,  220, 220),
        rgb(97,  230, 159),
        rgb(255, 100, 130),
        rgb(255, 200, 100),
        rgb(200, 140, 255),
        rgb(110, 100, 150),
        rgb(255, 165, 130),
    ],
};

// ── PALENIGHT ─────────────────────────────────────────────────────────────────

pub static PALENIGHT: Theme = Theme {
    name: "palenight",
    bg:         rgb(41,  45,  62),
    bg_panel:   rgb(48,  54,  74),
    bg_element: rgb(58,  65,  88),
    bg_menu:    rgb(34,  38,  52),
    text:       rgb(166, 172, 205),
    text_muted: rgb(84,  90,  120),
    text_dim:   rgb(58,  65,  88),
    primary:    rgb(199, 146, 234),
    secondary:  rgb(84,  90,  120),
    accent:     rgb(130, 170, 255),
    success:    rgb(195, 232, 141),
    error:      rgb(240, 113, 120),
    warning:    rgb(255, 203, 107),
    border:     rgb(84,  90,  120),
    border_hi:  rgb(199, 146, 234),
    diff_add_bg:  rgb(22,  55,  30),
    diff_del_bg:  rgb(60,  22,  28),
    diff_ctx_bg:  rgb(48,  54,  74),
    diff_add_fg:  rgb(195, 232, 141),
    diff_del_fg:  rgb(240, 113, 120),
    diff_ln_fg:   rgb(84,  90,  120),
    agents: [
        rgb(199, 146, 234),
        rgb(130, 170, 255),
        rgb(195, 232, 141),
        rgb(240, 113, 120),
        rgb(255, 203, 107),
        rgb(137, 221, 255),
        rgb(84,  90,  120),
        rgb(255, 136, 89),
    ],
};

// ── NIGHTOWL ──────────────────────────────────────────────────────────────────

pub static NIGHTOWL: Theme = Theme {
    name: "nightowl",
    bg:         rgb(1,   22,  39),
    bg_panel:   rgb(10,  35,  60),
    bg_element: rgb(20,  55,  90),
    bg_menu:    rgb(0,   15,  28),
    text:       rgb(214, 222, 235),
    text_muted: rgb(100, 124, 158),
    text_dim:   rgb(60,  80,  110),
    primary:    rgb(130, 170, 255),
    secondary:  rgb(100, 124, 158),
    accent:     rgb(127, 219, 202),
    success:    rgb(173, 219, 103),
    error:      rgb(255, 88,  116),
    warning:    rgb(255, 203, 107),
    border:     rgb(30,  68,  110),
    border_hi:  rgb(130, 170, 255),
    diff_add_bg:  rgb(10,  55,  30),
    diff_del_bg:  rgb(55,  10,  20),
    diff_ctx_bg:  rgb(10,  35,  60),
    diff_add_fg:  rgb(173, 219, 103),
    diff_del_fg:  rgb(255, 88,  116),
    diff_ln_fg:   rgb(100, 124, 158),
    agents: [
        rgb(130, 170, 255),
        rgb(127, 219, 202),
        rgb(173, 219, 103),
        rgb(255, 88,  116),
        rgb(255, 203, 107),
        rgb(199, 146, 234),
        rgb(100, 124, 158),
        rgb(255, 165, 50),
    ],
};

// ── KAWAII ────────────────────────────────────────────────────────────────────
// Pastel pink, soft lavender, bubblegum — cute & high contrast

pub static KAWAII: Theme = Theme {
    name: "kawaii",
    bg:         rgb(255, 240, 248),  // warm cream-pink
    bg_panel:   rgb(255, 228, 241),  // cotton candy
    bg_element: rgb(255, 210, 230),  // slightly deeper pink
    bg_menu:    rgb(255, 235, 245),
    text:       rgb(80,  40,  80),   // deep plum — readable on light
    text_muted: rgb(180, 100, 160),
    text_dim:   rgb(210, 150, 190),
    primary:    rgb(255, 100, 180),  // hot pink
    secondary:  rgb(200, 120, 220),  // orchid
    accent:     rgb(140, 80,  220),  // violet
    success:    rgb(100, 200, 130),  // mint
    error:      rgb(240, 60,  100),
    warning:    rgb(255, 160, 60),
    border:     rgb(240, 180, 220),
    border_hi:  rgb(255, 100, 180),
    diff_add_bg:  rgb(200, 255, 220),
    diff_del_bg:  rgb(255, 200, 210),
    diff_ctx_bg:  rgb(255, 228, 241),
    diff_add_fg:  rgb(40,  140, 80),
    diff_del_fg:  rgb(200, 40,  80),
    diff_ln_fg:   rgb(180, 100, 160),
    agents: [
        rgb(255, 100, 180),
        rgb(140, 80,  220),
        rgb(100, 200, 130),
        rgb(240, 60,  100),
        rgb(255, 160, 60),
        rgb(200, 120, 220),
        rgb(80,  180, 255),
        rgb(255, 200, 80),
    ],
};

// ── GOTH ──────────────────────────────────────────────────────────────────────
// Near-black backgrounds, blood red accents, bone-white text

pub static GOTH: Theme = Theme {
    name: "goth",
    bg:         rgb(8,   4,   10),   // near-void black
    bg_panel:   rgb(18,  10,  22),
    bg_element: rgb(32,  18,  38),
    bg_menu:    rgb(12,  6,   15),
    text:       rgb(225, 215, 225),  // bone white
    text_muted: rgb(130, 90,  130),  // dusty mauve
    text_dim:   rgb(70,  45,  70),
    primary:    rgb(180, 20,  40),   // blood red
    secondary:  rgb(120, 40,  100),  // deep burgundy
    accent:     rgb(200, 60,  80),   // crimson
    success:    rgb(80,  160, 80),   // sickly green
    error:      rgb(220, 30,  60),
    warning:    rgb(180, 120, 40),   // tarnished gold
    border:     rgb(60,  25,  55),
    border_hi:  rgb(180, 20,  40),
    diff_add_bg:  rgb(15,  40,  15),
    diff_del_bg:  rgb(50,  10,  15),
    diff_ctx_bg:  rgb(18,  10,  22),
    diff_add_fg:  rgb(80,  160, 80),
    diff_del_fg:  rgb(220, 30,  60),
    diff_ln_fg:   rgb(130, 90,  130),
    agents: [
        rgb(180, 20,  40),
        rgb(200, 60,  80),
        rgb(80,  160, 80),
        rgb(150, 80,  200),
        rgb(180, 120, 40),
        rgb(100, 150, 200),
        rgb(130, 90,  130),
        rgb(220, 140, 60),
    ],
};

// ── SEPIA ─────────────────────────────────────────────────────────────────────
// Warm parchment tones — aged paper feel

pub static SEPIA: Theme = Theme {
    name: "sepia",
    bg:         rgb(30,  22,  12),   // dark espresso
    bg_panel:   rgb(42,  30,  18),
    bg_element: rgb(60,  44,  26),
    bg_menu:    rgb(24,  17,  9),
    text:       rgb(220, 200, 165),  // warm parchment
    text_muted: rgb(150, 120, 80),   // faded ink
    text_dim:   rgb(100, 78,  50),
    primary:    rgb(200, 150, 80),   // aged gold
    secondary:  rgb(160, 110, 60),
    accent:     rgb(220, 170, 90),   // warm amber
    success:    rgb(130, 170, 90),   // olive green
    error:      rgb(190, 70,  50),   // rust red
    warning:    rgb(210, 155, 60),
    border:     rgb(80,  58,  34),
    border_hi:  rgb(200, 150, 80),
    diff_add_bg:  rgb(25,  45,  18),
    diff_del_bg:  rgb(55,  22,  14),
    diff_ctx_bg:  rgb(42,  30,  18),
    diff_add_fg:  rgb(130, 170, 90),
    diff_del_fg:  rgb(190, 70,  50),
    diff_ln_fg:   rgb(150, 120, 80),
    agents: [
        rgb(200, 150, 80),
        rgb(220, 170, 90),
        rgb(130, 170, 90),
        rgb(190, 70,  50),
        rgb(160, 130, 200),
        rgb(100, 160, 180),
        rgb(150, 120, 80),
        rgb(210, 130, 70),
    ],
};

// ── Theme registry ────────────────────────────────────────────────────────────

static THEMES: Lazy<HashMap<&'static str, &'static Theme>> = Lazy::new(|| {
    let mut m: HashMap<&'static str, &'static Theme> = HashMap::new();
    m.insert("cyberpunk",   &CYBERPUNK);
    m.insert("dracula",     &DRACULA);
    m.insert("tokyonight",  &TOKYONIGHT);
    m.insert("catppuccin",  &CATPPUCCIN);
    m.insert("nord",        &NORD);
    m.insert("gruvbox",     &GRUVBOX);
    m.insert("monokai",     &MONOKAI);
    m.insert("one-dark",    &ONE_DARK);
    m.insert("synthwave84", &SYNTHWAVE84);
    m.insert("matrix",      &MATRIX);
    m.insert("rosepine",    &ROSEPINE);
    m.insert("everforest",  &EVERFOREST);
    m.insert("solarized",   &SOLARIZED);
    m.insert("kanagawa",    &KANAGAWA);
    m.insert("vesper",      &VESPER);
    m.insert("aura",        &AURA);
    m.insert("palenight",   &PALENIGHT);
    m.insert("nightowl",    &NIGHTOWL);
    m.insert("kawaii",      &KAWAII);
    m.insert("goth",        &GOTH);
    m.insert("sepia",       &SEPIA);
    m
});

/// Get a theme by name. Falls back to cyberpunk if not found.
pub fn get(name: &str) -> &'static Theme {
    THEMES.get(name).copied().unwrap_or(&CYBERPUNK)
}

/// All available theme names sorted.
pub fn all_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = THEMES.keys().copied().collect();
    names.sort();
    names
}

/// Cycle to the next theme in alphabetical order.
pub fn next_theme(current: &str) -> &'static str {
    let names = all_names();
    let idx = names.iter().position(|&n| n == current).unwrap_or(0);
    names[(idx + 1) % names.len()]
}

pub fn prev_theme(current: &str) -> &'static str {
    let names = all_names();
    let idx = names.iter().position(|&n| n == current).unwrap_or(0);
    names[(idx + names.len() - 1) % names.len()]
}
