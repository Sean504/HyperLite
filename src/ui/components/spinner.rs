/// Braille-dot spinner for streaming / loading states.

const FRAMES: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];

pub struct Spinner {
    frame: usize,
}

impl Spinner {
    pub fn new() -> Self { Self { frame: 0 } }

    pub fn tick(&mut self) {
        self.frame = (self.frame + 1) % FRAMES.len();
    }

    pub fn current(&self) -> &'static str {
        FRAMES[self.frame]
    }
}

impl Default for Spinner {
    fn default() -> Self { Self::new() }
}
