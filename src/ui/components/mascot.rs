/// Animated pixel-robot mascot for the sidebar.
///
/// A 20×14-pixel sprite composited with half-blocks (→ 20 cols × 7 rows),
/// padded down a couple rows so it isn't jammed against the top.
///
/// Detail: blinking antenna beacon, bolt rivets on the head, and a recessed
/// visor "screen" so the eyes glow against a dark panel (mecha / Game-Boy look)
/// instead of floating on the face.
///
/// Mood is derived from app state every frame — no extra state machine:
///   idle      eyes blink every ~4s, antenna beacon pulses
///   thinking  eyes scan left/right (model loading / status phase)
///   talking   mouth flaps open/closed while tokens stream
///   working   KITT-scanner sweeps across the mouth grill while tools run
///   error     red X eyes while an error toast is up
///
/// Grid palette: # casing · . panel · = visor screen · o bolt · @ beacon ·
///               E eye · X error eye · M mouth · W scanner · (space) transparent

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use crate::app::App;
use crate::ui::components::toast::ToastLevel;
use super::pixel;

const SPRITE_WIDTH: u16 = 20;
const PAD_TOP:      u16 = 2;   // push the robot down off the top edge
const SPRITE_ROWS:  u16 = 7;   // 14 grid rows / 2
/// Total sidebar rows the mascot panel occupies (pad + sprite + status).
pub const PANEL_HEIGHT: u16 = PAD_TOP + SPRITE_ROWS + 1;

#[derive(Clone, Copy, PartialEq)]
enum Mood { Idle, Thinking, Talking, Working, Error }

fn mood_of(app: &App) -> Mood {
    if let Some(t) = &app.toast {
        if t.level == ToastLevel::Error { return Mood::Error; }
    }
    if !app.pending_tool_calls.is_empty() { return Mood::Working; }
    if app.is_streaming() {
        if !app.stream_status.is_empty() { return Mood::Thinking; }
        return Mood::Talking;
    }
    Mood::Idle
}

/// Assemble the 14-row sprite grid from the animated regions.
/// `ant` 2-wide beacon · `eyes`/`eyes2` 12-wide · `m_top`/`m_bot` 8-wide mouth.
fn build(ant: &str, eyes: &str, eyes2: &str, m_top: &str, m_bot: &str) -> Vec<String> {
    vec![
        format!("         {}         ", ant),       // beacon
        "         ##         ".into(),               // antenna stalk
        "    ############    ".into(),               // crown
        "  ################  ".into(),               // head top
        format!("  #o{}o#  ", "############"),        // bolt rivets
        format!("  #.{}.#  ", "============"),        // visor frame (top)
        format!("  #.{}.#  ", eyes),                  // eyes
        format!("  #.{}.#  ", eyes2),                 // eyes (lower)
        format!("  #.{}.#  ", "============"),         // visor frame (bottom)
        format!("  ##..{}..##  ", m_top),             // mouth (upper grill)
        format!("  ##..{}..##  ", m_bot),             // mouth (lower / flap)
        "  ################  ".into(),                // jaw
        "   ##############   ".into(),                // chin
        "     ##########     ".into(),                // neck collar
    ]
}

// Eye region is 12px wide and sits on the dark visor screen (= pixels) so the
// accent eyes glow against it. Keep the `=` fill consistent with the frame rows.
const EYES_OPEN:  &str = "==EE====EE==";
const EYES_SHUT:  &str = "==__====__==";  // closed lids
const EYES_LEFT:  &str = "=EE====EE===";
const EYES_RIGHT: &str = "===EE====EE=";
const EYES_X:     &str = "==XX====XX==";
const MOUTH_FULL: &str = "MMMMMMMM";
const MOUTH_NONE: &str = "........";

fn grid_for(mood: Mood, tick: usize) -> Vec<String> {
    // Antenna beacon: blinks off briefly every ~2s
    let ant = if tick % 24 < 3 { "  " } else { "@@" };

    match mood {
        Mood::Idle => {
            if tick % 50 < 2 {
                build(ant, EYES_SHUT, EYES_SHUT, MOUTH_FULL, MOUTH_NONE)
            } else {
                build(ant, EYES_OPEN, EYES_OPEN, MOUTH_FULL, MOUTH_NONE)
            }
        }
        Mood::Thinking => {
            if (tick / 5) % 2 == 0 {
                build(ant, EYES_LEFT, EYES_LEFT, MOUTH_FULL, MOUTH_NONE)
            } else {
                build(ant, EYES_RIGHT, EYES_RIGHT, MOUTH_FULL, MOUTH_NONE)
            }
        }
        Mood::Talking => {
            // Mouth opens (2px tall) then closes while streaming
            if (tick / 3) % 2 == 0 {
                build(ant, EYES_OPEN, EYES_OPEN, MOUTH_FULL, MOUTH_FULL)
            } else {
                build(ant, EYES_OPEN, EYES_OPEN, MOUTH_FULL, MOUTH_NONE)
            }
        }
        Mood::Working => {
            // KITT scanner: bright 2px window sweeping across the grill
            let pos = (tick / 2) % 4;
            let mut m: Vec<char> = MOUTH_FULL.chars().collect();
            m[pos * 2]     = 'W';
            m[pos * 2 + 1] = 'W';
            let m: String = m.into_iter().collect();
            build(ant, EYES_OPEN, EYES_OPEN, &m, MOUTH_NONE)
        }
        Mood::Error => build(ant, EYES_X, EYES_X, MOUTH_FULL, MOUTH_NONE),
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.height < PANEL_HEIGHT || area.width < SPRITE_WIDTH { return; }

    let theme = &app.theme;
    let mood  = mood_of(app);
    let grid  = grid_for(mood, app.anim_tick);

    let panel_bg = theme.bg_panel;
    let palette = move |c: char| -> Option<Color> {
        match c {
            '#' => Some(theme.primary),     // casing
            '.' => Some(theme.bg_element),   // face panel / visor margin
            '=' => Some(theme.bg),           // recessed visor screen (dark)
            'o' => Some(theme.secondary),    // bolt rivets
            '@' => Some(theme.accent),       // antenna beacon
            'E' => Some(theme.accent),       // eyes (glow on the screen)
            'X' => Some(theme.error),        // error eyes
            '_' => Some(theme.secondary),    // closed eyelids
            'M' => Some(theme.text_dim),     // mouth grill
            'W' => Some(theme.accent),       // scanner highlight
            _   => None,                     // transparent
        }
    };

    let sprite = pixel::sprite_lines(&grid, &palette, panel_bg);

    let pad_x = area.width.saturating_sub(SPRITE_WIDTH) / 2;
    for (i, line) in sprite.into_iter().enumerate() {
        frame.render_widget(
            Paragraph::new(line),
            Rect {
                x: area.x + pad_x,
                y: area.y + PAD_TOP + i as u16,
                width: SPRITE_WIDTH.min(area.width),
                height: 1,
            },
        );
    }

    // Status word under the sprite
    let (label, color) = match mood {
        Mood::Idle     => ("idle",      theme.text_dim),
        Mood::Thinking => ("thinking…", theme.secondary),
        Mood::Talking  => ("talking",   theme.accent),
        Mood::Working  => ("working…",  theme.warning),
        Mood::Error    => ("error",     theme.error),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(label, Style::default().fg(color))))
            .alignment(ratatui::layout::Alignment::Center),
        Rect { x: area.x, y: area.y + PAD_TOP + SPRITE_ROWS, width: area.width, height: 1 },
    );
}
