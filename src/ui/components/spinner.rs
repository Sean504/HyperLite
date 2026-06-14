/// Spinner for streaming / loading states.
///
/// Each theme gets its own frame set so the loading animation matches the
/// aesthetic — matrix gets a shade-fade, cyberpunk gets a rotating block,
/// sepia gets a classic ASCII spinner, everything else falls back to braille.

const BRAILLE: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];

/// Rotating quarter-block — hard pixel corners, 16-bit feel.
const BLOCKS: &[&str] = &["▖","▘","▝","▗"];

/// CRT shade fade in/out.
const SHADES: &[&str] = &["░","▒","▓","█","▓","▒"];

/// Audio-meter wave.
const WAVE: &[&str] = &["▁","▂","▃","▄","▅","▆","▇","█","▇","▆","▅","▄","▃","▂"];

/// Crosses for the goth register.
const CROSSES: &[&str] = &["✚","✛","✜","✝"];

/// Old-school ASCII — fits the aged-paper sepia theme.
const ASCII: &[&str] = &["|","/","-","\\"];

/// Sparkle pulse.
const SPARKLE: &[&str] = &["·","✦","✧","✦"];

fn frames_for(theme_name: &str) -> &'static [&'static str] {
    match theme_name {
        "cyberpunk" | "aura"  => BLOCKS,
        "matrix"              => SHADES,
        "synthwave84"         => WAVE,
        "goth"                => CROSSES,
        "sepia" | "gruvbox"   => ASCII,
        "kawaii" | "rosepine" => SPARKLE,
        _                     => BRAILLE,
    }
}

pub struct Spinner {
    frame: usize,
}

impl Spinner {
    pub fn new() -> Self { Self { frame: 0 } }

    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Current frame using the default braille set.
    pub fn current(&self) -> &'static str {
        BRAILLE[self.frame % BRAILLE.len()]
    }

    /// Current frame using the active theme's frame set.
    pub fn current_for(&self, theme_name: &str) -> &'static str {
        let frames = frames_for(theme_name);
        frames[self.frame % frames.len()]
    }
}

impl Default for Spinner {
    fn default() -> Self { Self::new() }
}
