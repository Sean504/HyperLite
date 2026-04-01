/// Keybinding registry and dispatch.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    // Input
    Submit, Newline, ClearInput, PasteClipboard, HistoryPrev, HistoryNext,
    // Navigation
    ScrollUp, ScrollDown, ScrollHalfUp, ScrollHalfDown,
    ScrollPageUp, ScrollPageDown, ScrollTop, ScrollBottom,
    ScrollMsgPrev, ScrollMsgNext, ScrollLastUser,
    // Sessions
    NewSession, SessionList, DeleteSession, RenameSession, ForkSession,
    UndoMessage, RedoMessage, CopyLastMessage, CompactSession,
    ParentSession, NextChild, PrevChild,
    // Model/Agent
    ModelPicker, CycleModelNext, CycleModelPrev,
    CycleFavoriteNext, CycleFavoritePrev,
    AgentPicker,
    // Display
    ToggleThinking, ToggleSidebar, ToggleToolDetails, ToggleConceal,
    ToggleScrollbar, ToggleTerminalTitle,
    // Dialogs
    CommandPalette, Help, StatusView, OpenFolder,
    // Drafts
    StashDraft, PopDraft,
    // App
    Quit, Interrupt, ExternalEditor,
    ThemePicker, ThemeCycleNext, ThemeCyclePrev,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub code:      KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyCombo {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
    pub fn plain(code: KeyCode) -> Self {
        Self { code, modifiers: KeyModifiers::NONE }
    }
    pub fn ctrl(code: KeyCode) -> Self {
        Self { code, modifiers: KeyModifiers::CONTROL }
    }
    pub fn alt(code: KeyCode) -> Self {
        Self { code, modifiers: KeyModifiers::ALT }
    }
    pub fn shift(code: KeyCode) -> Self {
        Self { code, modifiers: KeyModifiers::SHIFT }
    }
}

pub struct Keybinds {
    map: HashMap<KeyCombo, Action>,
    /// Reverse map for display (action -> display string)
    labels: HashMap<Action, String>,
}

impl Keybinds {
    pub fn default_binds() -> Self {
        let mut k = Self { map: HashMap::new(), labels: HashMap::new() };
        use KeyCode::*;

        // Input
        k.bind(KeyCombo::plain(Enter),                     Action::Submit,           "Enter");
        k.bind(KeyCombo::new(Enter, KeyModifiers::ALT),    Action::Newline,          "Alt+Enter");
        k.bind(KeyCombo::ctrl(Char('j')),                  Action::Newline,          "Ctrl+J");
        k.bind(KeyCombo::ctrl(Char('l')),                  Action::ClearInput,       "Ctrl+L");
        k.bind(KeyCombo::ctrl(Char('v')),                  Action::PasteClipboard,   "Ctrl+V");
        k.bind(KeyCombo::plain(Up),                        Action::HistoryPrev,      "↑");
        k.bind(KeyCombo::plain(Down),                      Action::HistoryNext,      "↓");

        // Navigation
        k.bind(KeyCombo::plain(Char('j')),                 Action::ScrollDown,       "j");
        k.bind(KeyCombo::plain(Char('k')),                 Action::ScrollUp,         "k");
        k.bind(KeyCombo::plain(Down),                      Action::ScrollDown,       "↓");
        k.bind(KeyCombo::plain(Up),                        Action::ScrollUp,         "↑");
        k.bind(KeyCombo::ctrl(Char('d')),                  Action::ScrollHalfDown,   "Ctrl+D");
        k.bind(KeyCombo::ctrl(Char('u')),                  Action::ScrollHalfUp,     "Ctrl+U");
        k.bind(KeyCombo::ctrl(Char('f')),                  Action::ScrollPageDown,   "Ctrl+F");
        k.bind(KeyCombo::ctrl(Char('b')),                  Action::ScrollPageUp,     "Ctrl+B");
        k.bind(KeyCombo::plain(PageDown),                  Action::ScrollPageDown,   "PgDn");
        k.bind(KeyCombo::plain(PageUp),                    Action::ScrollPageUp,     "PgUp");
        k.bind(KeyCombo::plain(Char('g')),                 Action::ScrollTop,        "g");
        k.bind(KeyCombo::plain(Char('G')),                 Action::ScrollBottom,     "G");
        k.bind(KeyCombo::plain(Char('[')),                 Action::ScrollMsgPrev,    "[");
        k.bind(KeyCombo::plain(Char(']')),                 Action::ScrollMsgNext,    "]");
        k.bind(KeyCombo::plain(Char('{')),                 Action::ScrollLastUser,   "{");

        // Sessions
        k.bind(KeyCombo::ctrl(Char('n')),                  Action::NewSession,       "Ctrl+N");
        k.bind(KeyCombo::ctrl(Char('s')),                  Action::SessionList,      "Ctrl+S");
        k.bind(KeyCombo::ctrl(Char('w')),                  Action::DeleteSession,    "Ctrl+W");
        k.bind(KeyCombo::ctrl(Char('r')),                  Action::RenameSession,    "Ctrl+R");
        k.bind(KeyCombo::ctrl(Char('z')),                  Action::UndoMessage,      "Ctrl+Z");
        k.bind(KeyCombo::ctrl(Char('y')),                  Action::RedoMessage,      "Ctrl+Y");
        k.bind(KeyCombo::ctrl(Char('c')),                  Action::CopyLastMessage,  "Ctrl+C");
        k.bind(KeyCombo::alt(Char('[')),                   Action::ParentSession,    "Alt+[");
        k.bind(KeyCombo::alt(Char(']')),                   Action::NextChild,        "Alt+]");

        // Model/Agent
        k.bind(KeyCombo::ctrl(Char('m')),                  Action::ModelPicker,      "Ctrl+M");
        k.bind(KeyCombo::alt(Char('m')),                   Action::CycleModelNext,   "Alt+M");
        k.bind(KeyCombo::new(Char('m'), KeyModifiers::ALT | KeyModifiers::SHIFT),
                                                           Action::CycleModelPrev,   "Alt+Shift+M");
        k.bind(KeyCombo::ctrl(Char('a')),                  Action::AgentPicker,      "Ctrl+A");

        // Display
        k.bind(KeyCombo::ctrl(Char('t')),                  Action::ToggleThinking,   "Ctrl+T");
        k.bind(KeyCombo::ctrl(Char('\\')),                 Action::ToggleSidebar,    "Ctrl+\\");
        k.bind(KeyCombo::ctrl(Char('h')),                  Action::ToggleToolDetails,"Ctrl+H");
        k.bind(KeyCombo::ctrl(Char('/')),                  Action::ToggleConceal,    "Ctrl+/");

        // Dialogs
        k.bind(KeyCombo::ctrl(Char('k')),                  Action::CommandPalette,   "Ctrl+K");
        k.bind(KeyCombo::plain(Char('?')),                 Action::Help,             "?");
        k.bind(KeyCombo::ctrl(Char('o')),                  Action::OpenFolder,       "Ctrl+O");

        // Drafts
        k.bind(KeyCombo::ctrl(Char('d')),                  Action::StashDraft,       "Ctrl+D");
        k.bind(KeyCombo::new(Char('d'), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
                                                           Action::PopDraft,         "Ctrl+Shift+D");

        // App — Ctrl+X is the primary quit (Ctrl+Q may be eaten by some terminals)
        k.bind(KeyCombo::ctrl(Char('x')),                  Action::Quit,             "Ctrl+X");
        k.bind(KeyCombo::ctrl(Char('q')),                  Action::Quit,             "Ctrl+X");
        k.bind(KeyCombo::ctrl(Char('e')),                  Action::ExternalEditor,   "Ctrl+E");
        k.bind(KeyCombo::new(Char('t'), KeyModifiers::CONTROL | KeyModifiers::SHIFT),
                                                           Action::ThemeCycleNext,   "Ctrl+Shift+T");

        k
    }

    fn bind(&mut self, combo: KeyCombo, action: Action, label: &str) {
        self.labels.entry(action.clone()).or_insert_with(|| label.to_string());
        self.map.insert(combo, action);
    }

    /// Resolve a key event to an action.
    pub fn resolve(&self, event: &KeyEvent) -> Option<&Action> {
        let combo = KeyCombo {
            code:      event.code,
            modifiers: event.modifiers,
        };
        self.map.get(&combo)
    }

    /// Get display label for an action.
    pub fn label(&self, action: &Action) -> &str {
        self.labels.get(action).map(|s| s.as_str()).unwrap_or("")
    }

    /// All actions with their labels, grouped for help display.
    pub fn help_sections(&self) -> Vec<(&'static str, Vec<(String, String)>)> {
        use Action::*;
        let groups: &[(&'static str, &[Action])] = &[
            ("Input", &[Submit, Newline, ClearInput, PasteClipboard, HistoryPrev, HistoryNext]),
            ("Navigation", &[ScrollUp, ScrollDown, ScrollHalfUp, ScrollHalfDown,
                              ScrollPageUp, ScrollPageDown, ScrollTop, ScrollBottom,
                              ScrollMsgPrev, ScrollMsgNext]),
            ("Sessions", &[NewSession, SessionList, UndoMessage, RedoMessage,
                            CopyLastMessage, ForkSession, CompactSession,
                            ParentSession, NextChild]),
            ("Model/Agent", &[ModelPicker, CycleModelNext, CycleModelPrev, AgentPicker]),
            ("Display", &[ToggleThinking, ToggleSidebar, ToggleToolDetails,
                           ToggleConceal, ThemePicker, ThemeCycleNext]),
            ("App", &[CommandPalette, Help, OpenFolder, Quit, ExternalEditor]),
        ];

        groups.iter().map(|(section, actions)| {
            let items: Vec<(String, String)> = actions.iter()
                .filter_map(|a| {
                    let label = self.label(a);
                    if label.is_empty() { return None; }
                    let desc = action_description(a);
                    Some((label.to_string(), desc.to_string()))
                })
                .collect();
            (*section, items)
        }).collect()
    }
}

fn action_description(action: &Action) -> &'static str {
    use Action::*;
    match action {
        Submit           => "Submit prompt",
        Newline          => "Insert newline",
        ClearInput       => "Clear input",
        PasteClipboard   => "Paste from clipboard",
        HistoryPrev      => "Previous prompt history",
        HistoryNext      => "Next prompt history",
        ScrollDown       => "Scroll down",
        ScrollUp         => "Scroll up",
        ScrollHalfDown   => "Half page down",
        ScrollHalfUp     => "Half page up",
        ScrollPageDown   => "Page down",
        ScrollPageUp     => "Page up",
        ScrollTop        => "Jump to top",
        ScrollBottom     => "Jump to bottom",
        ScrollMsgPrev    => "Previous message",
        ScrollMsgNext    => "Next message",
        ScrollLastUser   => "Last user message",
        NewSession       => "New session",
        SessionList      => "Switch session",
        DeleteSession    => "Delete session",
        RenameSession    => "Rename session",
        ForkSession      => "Fork session",
        UndoMessage      => "Undo last message",
        RedoMessage      => "Redo",
        CopyLastMessage  => "Copy last response",
        CompactSession   => "Compact/summarize session",
        ParentSession    => "Go to parent session",
        NextChild        => "Next child session",
        PrevChild        => "Prev child session",
        ModelPicker      => "Switch model",
        CycleModelNext   => "Next recent model",
        CycleModelPrev   => "Prev recent model",
        CycleFavoriteNext=> "Next favorite model",
        CycleFavoritePrev=> "Prev favorite model",
        AgentPicker      => "Switch agent",
        ToggleThinking   => "Toggle reasoning display",
        ToggleSidebar    => "Toggle sidebar",
        ToggleToolDetails=> "Toggle tool detail",
        ToggleConceal    => "Toggle code conceal",
        ToggleScrollbar  => "Toggle scrollbar",
        ToggleTerminalTitle => "Toggle terminal title",
        CommandPalette   => "Command palette",
        Help             => "Show help",
        StatusView       => "Show status",
        OpenFolder       => "Open folder / repo",
        StashDraft       => "Stash input draft",
        PopDraft         => "Restore stashed draft",
        Quit             => "Quit",
        Interrupt        => "Interrupt generation",
        ExternalEditor   => "Open in editor",
        ThemePicker      => "Pick theme",
        ThemeCycleNext   => "Next theme",
        ThemeCyclePrev   => "Prev theme",
    }
}
