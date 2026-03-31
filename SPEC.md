# HyperLite — Full Technical Specification

> A terminal-native, provider-agnostic AI chat client written in Rust.
> Philosophy: fastest possible startup, smallest possible binary, zero runtime overhead — without sacrificing a single feature.

---

## 1. Technology Stack

| Concern               | Crate(s)                         | Reason |
|-----------------------|----------------------------------|--------|
| TUI rendering         | `ratatui` + `crossterm`          | Standard, maintained, full widget system, mouse support |
| Async runtime         | `tokio`                          | Industry standard, SSE streaming, event loop |
| HTTP / SSE streaming  | `reqwest`                        | Async-native, SSE support, works with Tokio |
| Syntax highlighting   | `syntect`                        | Sublime Text grammars, fast, full control |
| Markdown parsing      | `pulldown-cmark`                 | CommonMark, incremental, integrates with syntect |
| Config                | `toml` + `serde`                 | Rust ecosystem standard, human-readable |
| Persistence           | `rusqlite` (SQLite)              | SQL queryable history, single file, battle-tested |
| Text input widget     | `tui-textarea`                   | Multi-line, keybindings, crossterm native |
| Fuzzy search          | `nucleo`                         | Fastest fuzzy matcher, fzf algorithm, Helix-proven |
| Diff rendering        | `similar`                        | Clean diff algorithms, no deps |
| Clipboard             | `arboard`                        | Cross-platform clipboard read/write |
| Date/time             | `chrono`                         | Standard Rust datetime |
| UUID/ID generation    | `ulid`                           | Sortable IDs for sessions/messages |
| Serialization         | `serde` + `serde_json`           | Universal serialization |
| Env vars / dotenv     | `dotenvy`                        | Load .env files for API keys |
| Logging (debug)       | `tracing` + `tracing-subscriber` | Structured logs to file (never terminal) |

**Binary targets**: Linux x86_64, macOS arm64, macOS x86_64, Windows x86_64

---

## 2. Project Structure

```
hyperlite/
├── Cargo.toml
├── Cargo.lock
├── SPEC.md
├── README.md
├── src/
│   ├── main.rs                  # Entry point, CLI args, tokio runtime setup
│   ├── app.rs                   # Top-level App state machine
│   ├── event.rs                 # Input event loop (keyboard, mouse, resize, tick)
│   ├── config/
│   │   ├── mod.rs               # Config loading and merging
│   │   ├── schema.rs            # Config struct definitions (serde)
│   │   └── defaults.rs          # Default values
│   ├── db/
│   │   ├── mod.rs               # Database connection pool
│   │   ├── migrations.rs        # Schema migrations
│   │   ├── session.rs           # Session CRUD
│   │   └── message.rs           # Message CRUD
│   ├── providers/
│   │   ├── mod.rs               # Provider trait + registry
│   │   ├── anthropic.rs         # Claude API (SSE streaming)
│   │   ├── openai.rs            # OpenAI-compatible (GPT, local)
│   │   ├── google.rs            # Gemini API
│   │   ├── ollama.rs            # Local Ollama
│   │   └── openrouter.rs        # OpenRouter aggregator
│   ├── ui/
│   │   ├── mod.rs               # UI root, draws full frame
│   │   ├── layout.rs            # Layout computation (terminal size breakpoints)
│   │   ├── theme.rs             # Theme system (all named themes + custom)
│   │   ├── colors.rs            # RGBA helpers, contrast calculation
│   │   ├── components/
│   │   │   ├── mod.rs
│   │   │   ├── message_list.rs  # Scrollable message history
│   │   │   ├── message.rs       # Single message rendering (user/assistant)
│   │   │   ├── tool_call.rs     # Tool call + output rendering
│   │   │   ├── diff_view.rs     # Unified/split diff display
│   │   │   ├── input.rs         # Prompt input area
│   │   │   ├── sidebar.rs       # Right sidebar
│   │   │   ├── footer.rs        # Status footer bar
│   │   │   ├── spinner.rs       # Braille dot spinner
│   │   │   ├── toast.rs         # Toast notification overlay
│   │   │   └── badge.rs         # Inline badges (file type, QUEUED, etc.)
│   │   ├── dialogs/
│   │   │   ├── mod.rs           # Dialog stack manager
│   │   │   ├── base.rs          # Modal overlay + border + close behavior
│   │   │   ├── session_list.rs  # Session switcher dialog
│   │   │   ├── model_picker.rs  # Model selection with fuzzy search
│   │   │   ├── agent_picker.rs  # Agent selection dialog
│   │   │   ├── theme_picker.rs  # Theme selection
│   │   │   ├── help.rs          # Keybindings reference
│   │   │   ├── command.rs       # Command palette (Ctrl+K)
│   │   │   ├── confirm.rs       # Generic yes/no confirmation
│   │   │   └── message_actions.rs # Per-message action menu
│   │   ├── markdown.rs          # pulldown-cmark → ratatui Text spans
│   │   └── syntax.rs            # syntect integration for code blocks
│   ├── session/
│   │   ├── mod.rs               # Session state management
│   │   ├── message.rs           # Message types and part types
│   │   └── scroll.rs            # Scroll state (position, acceleration)
│   └── keybinds/
│       ├── mod.rs               # Keybind registry and dispatch
│       └── defaults.rs          # Default keybinding map
├── assets/
│   └── themes/                  # Built-in theme TOML files
└── migrations/
    └── 001_initial.sql          # SQLite schema
```

---

## 3. Layout Specification

### Breakpoints
```
< 80 cols:   Minimal — input + messages only, no sidebar, no footer icons
80–119 cols: Normal — input + messages + footer
≥ 120 cols:  Wide — input + messages + sidebar (42 cols) + footer
```

### Screen Regions (wide layout)
```
┌─────────────────────────────────────────────┬──────────────────────┐
│  MESSAGE PANE                               │  SIDEBAR (42 cols)   │
│  (terminal_width - 42 - 2 cols)             │                      │
│  Scrollable, sticky-bottom                  │  Session title       │
│  All conversation history                   │  ─────────────────   │
│                                             │  Plugin/info slots   │
│                                             │                      │
│                                             │  ─────────────────   │
│                                             │  • HyperLite v0.1.0  │
├─────────────────────────────────────────────┴──────────────────────┤
│  INPUT AREA  (prompt / permission / question — mutually exclusive)  │
├─────────────────────────────────────────────────────────────────────┤
│  FOOTER: cwd                    • N LSP  ⊙ N MCP  △ N permission   │
└─────────────────────────────────────────────────────────────────────┘
```

### Message Pane
- Sticky scroll: when near bottom, auto-scrolls as new tokens stream in
- Leaves auto-scroll when user scrolls up manually
- Re-engages auto-scroll when user scrolls back to bottom
- Scroll acceleration: configurable speed (default 3 lines per scroll tick)
- Vertical scrollbar: visible on demand (toggle) or always, right side of pane
  - Track: `theme.background_element`
  - Thumb: `theme.border`
  - Padding left: 1

### Input Area
- Minimum 3 lines, expands to max 10 lines
- Replaced entirely by `PermissionPrompt` when permission is pending
- Replaced entirely by `QuestionPrompt` when agent asks a question
- Original input restored (with content preserved) after permission/question resolved

### Sidebar
- Hidden on < 120 cols, or when in subagent session view
- Toggle with keybind regardless of width (forces visible/hidden)
- `auto` mode: visible if ≥ 120 cols

---

## 4. Theme System

### 27 Built-in Themes
```
aura, ayu, catppuccin, catppuccin-frappe, catppuccin-macchiato,
cobalt2, cursor, dracula, everforest, flexoki, github, github-light,
gruvbox, kanagawa, material, matrix, mercury, monokai, nightowl,
nord, one-dark, opencode, palenight, rosepine, solarized,
synthwave84, tokyonight, vesper, zenburn
```

### Theme Color Tokens
```rust
pub struct Theme {
    // Backgrounds
    pub background:          Color,  // main background
    pub background_panel:    Color,  // message box background
    pub background_element:  Color,  // hover states, inputs
    pub background_menu:     Color,  // dropdown/menu background

    // Text
    pub text:                Color,  // primary readable text
    pub text_muted:          Color,  // labels, timestamps, subtle info

    // Accents
    pub primary:             Color,  // links, active indicators
    pub secondary:           Color,  // secondary accents
    pub accent:              Color,  // selected items highlight

    // Semantic
    pub success:             Color,  // green (LSP active, MCP ok)
    pub error:               Color,  // red (failures, errors)
    pub warning:             Color,  // orange (permissions pending)

    // Borders
    pub border:              Color,  // inactive border
    pub border_active:       Color,  // focused border

    // Diff colors
    pub diff_added_bg:       Color,
    pub diff_removed_bg:     Color,
    pub diff_context_bg:     Color,
    pub diff_added:          Color,
    pub diff_removed:        Color,
    pub diff_line_number:    Color,
    pub diff_highlight_added:   Color,
    pub diff_highlight_removed: Color,

    // Markdown
    pub markdown_text:       Color,

    // Misc
    pub thinking_opacity:    f32,    // default 0.6 — applied to reasoning text
}
```

### Theme Loading Priority
1. System default (tokyonight)
2. `~/.config/hyperlite/themes/<name>.toml` (custom user themes)
3. Config file setting `theme = "name"`
4. CLI flag `--theme <name>`

### Agent Colors
Each session/agent is assigned one of N accent colors from the theme palette.
Used as the left-border color on user messages (identifies who sent what).

---

## 5. All UI Components

### 5.1 Message List
- Full-width scrollable region
- Each message rendered in order, top to bottom
- `marginTop = 1` between messages (not before first)
- Keyboard navigation: jump to message by index or ID
- Mouse click on user message: opens `MessageActionsDialog`

### 5.2 User Message
```
┃ message text here, wrapped at content width
  file1.png  filename.py
  QUEUED                    ← badge if pending, or HH:MM timestamp
```
- Left border: `┃` char (U+2503), colored with agent color
- Background: `background_panel`, changes to `background_element` on hover
- File attachments: colored badge with MIME type label + filename
  - Image: cyan bg
  - PDF: red bg
  - Text: gray bg
- Timestamp: `theme.text_muted`, right-aligned in footer of message box
- QUEUED badge: bold white text on agent color background

### 5.3 Assistant Message
Composed of ordered parts:

**ReasoningPart** (if model outputs chain-of-thought)
```
┃  _Thinking:_ lorem ipsum...
   continuation of reasoning
```
- Border: `theme.background_element` (very subtle)
- Text: `theme.text_muted` at `thinking_opacity`
- Hidden by default, togglable with `toggle_thinking` keybind
- Filters out `[REDACTED]` reasoning from OpenRouter

**TextPart** (main response text)
```
   Markdown rendered inline:
   - **bold**, _italic_, `inline code`
   - Fenced code blocks with syntax highlighting
   - Lists, blockquotes, headers
```
- `paddingLeft = 3`
- `marginTop = 1` above
- Streams in real-time as tokens arrive
- Markdown → ratatui Spans via pulldown-cmark
- Code blocks: syntect highlighting per language

**ToolPart** (tool invocation, see section 5.4)

**Message Footer** (last part of assistant message only)
```
   claude-3-5-sonnet  12.3s
```
- Model name + generation duration
- Color: `theme.text_muted`

### 5.4 Tool Call Rendering

#### Inline Mode (pending / quick)
```
  ~ Writing command...
```
Single line, muted, replaced when complete with:
```
  $ git status
```

#### Block Mode (running or complete)
```
  ⠹ git status          ← spinner while running
  ─────────────────────
  $ git status
  On branch main
  nothing to commit
                        ← "..." if >10 lines (click to expand)
```

**Per-tool icons and pending text:**

| Tool         | Icon | Pending text            |
|--------------|------|-------------------------|
| bash         | `$`  | `Running command...`    |
| read         | `→`  | `Reading file...`       |
| write        | `←`  | `Writing file...`       |
| edit         | `→`  | `Editing file...`       |
| glob         | `✱`  | `Finding files...`      |
| grep         | `✱`  | `Searching...`          |
| list         | `→`  | `Listing directory...`  |
| webfetch     | `%`  | `Fetching URL...`       |
| websearch    | `◈`  | `Searching web...`      |
| codesearch   | `◇`  | `Searching code...`     |
| task/todo    | `□`  | `Updating tasks...`     |
| generic      | `⚙`  | `Running tool...`       |

**Output truncation:**
- Default: show first 10 lines
- If > 10 lines: show `…` with "(click to expand)" in `text_muted`
- Expanded: show all, "(click to collapse)"
- Strip ANSI escape codes from bash output before rendering

**Error state:**
```
  $ bad-command          ← strikethrough text
  Error: command not found
```
- Strikethrough on tool line
- Error text in `theme.error` below

**Permission-pending state:**
- Icon and text in `theme.warning` (orange)
- Pulses or stays highlighted until resolved

### 5.5 Diff View
Used inside permission prompts for edit/write operations.

**Unified mode** (< 120 cols):
```
  @@ -1,5 +1,6 @@
   context line
  - removed line
  + added line
   context line
```

**Split mode** (≥ 120 cols):
```
  ─ Before ──────────────┬─ After ────────────────
   context               │  context
  - removed line         │
                         │+ added line
```

**Colors:**
- Added line bg: `diff_added_bg`
- Removed line bg: `diff_removed_bg`
- Context bg: `diff_context_bg`
- `+` sign: `diff_highlight_added`
- `-` sign: `diff_highlight_removed`
- Line numbers: `diff_line_number`
- Syntax highlighting within lines (via syntect)

### 5.6 Input Area
Multi-line text editor (tui-textarea):
- Placeholder text: rotates through configured examples
  - Normal mode: `"Fix a TODO in the codebase"`, `"What is the tech stack?"`, `"Fix broken tests"`
  - Shell mode: `"ls -la"`, `"git status"`, `"pwd"`
- Cursor: `theme.primary` color
- Border: `theme.border`, changes to `theme.border_active` when focused
- Shows attached files as badges above textarea (same style as user message badges)
- Autocomplete dropdown: appears below input, fuzzy filtered, keyboard navigable
  - Slash commands: `/new`, `/clear`, `/sessions`, `/models`, `/themes`, `/help`, etc.
  - `@file` mention completion (fuzzy file path search)
  - `#agent` mention for agent switching

### 5.7 Sidebar (42 cols)
```
┌──────────────────────────────────────────┐
│ Session title truncated to fit...        │
│ https://share.url (if shared)            │
├──────────────────────────────────────────┤
│                                          │
│  (plugin slot / info area)               │
│                                          │
├──────────────────────────────────────────┤
│ • HyperLite v0.1.0                       │
└──────────────────────────────────────────┘
```
- Background: `theme.background_panel`
- Session title: bold, truncated with `…`
- Version footer: `•` in `theme.success`, version in `theme.text_muted`

### 5.8 Footer
```
~/projects/myapp                • 2 LSP  ⊙ 3 MCP  △ 1 permission
```
- Left: current working directory (truncated if needed)
- Right cluster:
  - `• N LSP` — dot green if any active, gray if none
  - `⊙ N MCP` — circle green if ok, red if error, hidden if none
  - `△ N permission` — orange, shown only when permissions pending
  - `/status` hint text in `text_muted`
- All separated by 2 spaces

### 5.9 Spinner
- Braille frames: `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`
- Interval: 80ms
- Color: `theme.text_muted`
- Fallback when animations disabled: `⋯`

### 5.10 Toast Notifications
- Position: top-right corner, 2 rows from top, 2 cols from right
- Max width: `min(60, terminal_width - 6)`
- Border: left + right `┃` chars, colored by variant
  - `error` → `theme.error`
  - `success` → `theme.success`
  - `warning` → `theme.warning`
  - `info` → `theme.primary`
- Bold title (optional)
- Body text word-wrapped
- Auto-dismiss after 3 seconds
- Stack-able (multiple toasts queue)

---

## 6. All Dialogs / Modals

### Dialog System
- Overlay: semi-transparent black background (`rgba(0,0,0,150)`)
- Centered on screen
- Size variants:
  - `Small`:  50 cols
  - `Medium`: 60 cols
  - `Large`:  88 cols
  - `XLarge`: 116 cols
- Escape closes top dialog
- Dialog stack: nested dialogs supported, Escape pops one layer
- All dialogs have: title bar, content area, optional keybind hint row at bottom

### 6.1 Session List Dialog
```
  ┌─ Sessions ───────────────────────────────────┐
  │ > filter...                                  │
  ├──────────────────────────────────────────────┤
  │ Today                                        │
  │ ● Fix auth bug in middleware                 │
  │   Refactor database layer            ⠹       │
  │ Yesterday                                    │
  │   Add unit tests                             │
  │   Update dependencies                        │
  ├──────────────────────────────────────────────┤
  │ [d] delete  [r] rename  [Enter] open         │
  └──────────────────────────────────────────────┘
```
- Fuzzy search filter at top
- Grouped by date: Today, Yesterday, date strings
- `●` marks current session
- `⠹` spinner on active sessions
- Delete: press `d` twice to confirm
- Rename: press `r`, inline rename input appears

### 6.2 Model Picker Dialog
```
  ┌─ Models ─────────────────────────────────────┐
  │ > search models...                           │
  ├──────────────────────────────────────────────┤
  │ Favorites                                    │
  │ ● claude-sonnet-4-5     Anthropic            │
  │ Recent                                       │
  │   gpt-4o                OpenAI               │
  │ Anthropic                                    │
  │   claude-opus-4-6       Anthropic            │
  │   claude-haiku-4-5      Anthropic   Free     │
  │ OpenAI                                       │
  │   gpt-4o                OpenAI               │
  │   o3-mini               OpenAI               │
  ├──────────────────────────────────────────────┤
  │ [f] favorite  [c] connect provider           │
  └──────────────────────────────────────────────┘
```
- Fuzzy search across title + provider
- Sections: Favorites → Recent → Providers (grouped alphabetically)
- `●` marks current selection, `★` marks favorites
- `Free` badge for no-cost models
- `f` toggles favorite on highlighted item
- Provider groups collapsible

### 6.3 Agent Picker Dialog
- Same structure as Model Picker
- Lists: built-in agents + configured custom agents
- Shows agent description/mode below name

### 6.4 Theme Picker Dialog
- Live preview: theme applies immediately as you navigate
- Reverts if Escape pressed, commits on Enter
- Shows theme name + light/dark indicator

### 6.5 Command Palette (Ctrl+K)
```
  ┌─ Commands ───────────────────────────────────┐
  │ > type a command...                          │
  ├──────────────────────────────────────────────┤
  │ New session                          /new    │
  │ Switch session                   /sessions   │
  │ Switch model                       /models   │
  │ Compact session                    /compact  │
  │ Toggle thinking                  /thinking   │
  │ Toggle sidebar                              │
  │ Help                                 /help   │
  └──────────────────────────────────────────────┘
```
- All commands searchable by name or slash command
- Shows keybind hint on right
- Immediate action on Enter

### 6.6 Help Dialog
```
  ┌─ Keyboard Shortcuts ─────────────────────────┐
  │ Navigation                                   │
  │   j / ↓          Scroll down                 │
  │   k / ↑          Scroll up                   │
  │   g              Jump to top                 │
  │   G              Jump to bottom              │
  │   Ctrl+U         Half page up                │
  │   Ctrl+D         Half page down              │
  │ Sessions                                     │
  │   Ctrl+N         New session                 │
  │   Ctrl+S         Switch session              │
  │   ...                                        │
  └──────────────────────────────────────────────┘
```
- Grouped by category
- Scrollable

### 6.7 Message Actions Dialog
Triggered by clicking a user message:
```
  ┌─────────────────┐
  │ Revert to here  │
  │ Copy message    │
  │ Fork session    │
  └─────────────────┘
```

### 6.8 Permission Prompt
Replaces input area entirely (not a floating dialog):

```
  ┌─ Permission Required ────────────────────────┐
  │ ▲ Edit file                                  │
  │ src/main.rs                                  │
  ├──────────────────────────────────────────────┤
  │ @@ -12,7 +12,8 @@                           │
  │  fn main() {                                 │
  │ -    println!("Hello");                      │
  │ +    println!("Hello, World!");              │
  │ +    println!("Starting...");                │
  ├──────────────────────────────────────────────┤
  │ [a] Allow once  [A] Allow always  [r] Reject │
  └──────────────────────────────────────────────┘
```
- `▲` in `theme.warning`
- Diff view for edit/write operations (split/unified by width)
- Command/path display for bash operations
- URL display for webfetch operations
- `[a]` Allow once, `[A]` Allow always, `[r]` Reject
- On "Allow always": shows confirmation screen with patterns
- On "Reject": shows optional reason textarea

### 6.9 Question Prompt
Replaces input area. Tab bar across top, options below:
```
  ┌─ Question ───────────────────────────────────┐
  │ [ Framework ]  [ Database ]  [ Confirm ]     │
  ├──────────────────────────────────────────────┤
  │ Choose a framework:                          │
  │ 1. [ ] React        Popular, large ecosystem │
  │ 2. [ ] Vue          Progressive, flexible    │
  │ 3. [✓] SolidJS      Fastest, smallest        │
  ├──────────────────────────────────────────────┤
  │ [1-9] select  [Space] toggle  [Enter] next   │
  └──────────────────────────────────────────────┘
```
- Tab bar: each question + final Confirm tab
- Selected tab: `theme.accent` background
- Options: numbered, `[✓]` for selected (multi), or `●` for single select
- Keyboard: number keys for direct selection, space to toggle, enter to advance

---

## 7. Permission System

### Rule Types
```rust
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,   // prompt user at runtime
}

pub struct PermissionRule {
    pub tool: String,           // tool name or "*"
    pub pattern: String,        // file glob, command pattern, or "*"
    pub action: PermissionAction,
}
```

### Rule Precedence (lowest to highest)
1. Hardcoded defaults (bash → ask, webfetch → ask, file writes → ask)
2. Global config (`~/.config/hyperlite/settings.toml`)
3. Project config (`.hyperlite/settings.toml`)
4. Session/agent config
5. Runtime "Allow always" grants

---

## 8. Complete Keybinding Map

All keybindings configurable. Defaults:

### Navigation (message pane)
| Key          | Action                    |
|--------------|---------------------------|
| `j` / `↓`   | Scroll down 1 line        |
| `k` / `↑`   | Scroll up 1 line          |
| `Ctrl+D`     | Half page down            |
| `Ctrl+U`     | Half page up              |
| `Ctrl+F` / `PgDn` | Page down            |
| `Ctrl+B` / `PgUp` | Page up              |
| `g`          | Jump to first message     |
| `G`          | Jump to last message      |
| `[`          | Jump to previous message  |
| `]`          | Jump to next message      |
| `{`          | Jump to last user message |

### Sessions
| Key          | Action                        |
|--------------|-------------------------------|
| `Ctrl+N`     | New session                   |
| `Ctrl+S`     | Session list dialog           |
| `Ctrl+W`     | Delete current session        |
| `Ctrl+R`     | Rename current session        |
| `Ctrl+F`     | Fork session from message     |
| `Ctrl+Z`     | Undo last message             |
| `Ctrl+Y`     | Redo (re-apply undone msg)    |
| `Alt+[`      | Go to parent session          |
| `Alt+]`      | Next child session            |
| `Alt+{`      | Previous child session        |
| `Alt+↓`      | First child session           |

### Model / Agent
| Key          | Action                        |
|--------------|-------------------------------|
| `Ctrl+M`     | Model picker dialog           |
| `Alt+M`      | Cycle to next recent model    |
| `Alt+Shift+M`| Cycle to prev recent model    |
| `Alt+F`      | Cycle favorite models         |
| `Alt+Shift+F`| Cycle favorites reverse       |
| `Ctrl+A`     | Agent picker dialog           |
| `Alt+A`      | Cycle agents                  |

### Input
| Key              | Action                       |
|------------------|------------------------------|
| `Enter`          | Submit prompt                |
| `Ctrl+J` / `Alt+Enter` | Insert newline         |
| `Esc`            | Clear input / close dialog   |
| `Ctrl+C` (2x)    | Interrupt running generation |
| `Ctrl+V`         | Paste from clipboard         |
| `Ctrl+L`         | Clear input                  |
| `↑` (in input)   | Previous prompt history      |
| `↓` (in input)   | Next prompt history          |
| Standard editing keys (Home, End, Ctrl+A, Ctrl+E, Ctrl+K, Ctrl+U, etc.) |

### Display
| Key          | Action                        |
|--------------|-------------------------------|
| `Ctrl+T`     | Toggle thinking/reasoning     |
| `Ctrl+\`     | Toggle sidebar                |
| `Ctrl+H`     | Toggle tool detail expansion  |
| `Ctrl+/`     | Toggle code concealment       |
| `Alt+S`      | Toggle scrollbar              |
| `Ctrl+K`     | Command palette               |
| `?`          | Help dialog                   |

### Misc
| Key          | Action                        |
|--------------|-------------------------------|
| `Ctrl+E`     | Open in external editor       |
| `Ctrl+C`     | Copy last assistant message   |
| `Ctrl+Shift+T` | Cycle themes                |
| `Ctrl+Q`     | Quit                          |

---

## 9. Slash Commands

Typed into input, trigger actions:

| Command              | Action                              |
|----------------------|-------------------------------------|
| `/new`, `/clear`     | New session                         |
| `/sessions`, `/resume`, `/continue` | Session list dialog    |
| `/models`            | Model picker                        |
| `/agents`            | Agent picker                        |
| `/themes`            | Theme picker                        |
| `/help`              | Help dialog                         |
| `/status`            | Status dialog (LSP, MCP, providers) |
| `/thinking`          | Toggle reasoning display            |
| `/compact`, `/summarize` | Compact session (summarize history) |
| `/fork`              | Fork session from last message      |
| `/share`             | Share session (if backend available) |
| `/unshare`           | Stop sharing session                |
| `/timeline`          | Jump to specific message            |
| `/mcps`              | Toggle MCP servers                  |
| `/variants`          | Switch model variant                |

---

## 10. Data Models

### Session
```rust
pub struct Session {
    pub id: Ulid,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub model_id: String,           // "anthropic/claude-sonnet-4-5"
    pub provider_id: String,        // "anthropic"
    pub agent_id: Option<String>,
    pub working_dir: PathBuf,
    pub parent_id: Option<Ulid>,    // for forked/child sessions
    pub summary: Option<SessionSummary>,
    pub is_shared: bool,
}

pub struct SessionSummary {
    pub additions: u32,
    pub deletions: u32,
    pub files: Vec<String>,
}
```

### Message
```rust
pub struct Message {
    pub id: Ulid,
    pub session_id: Ulid,
    pub role: Role,
    pub parts: Vec<Part>,
    pub created_at: DateTime<Utc>,
    pub model: Option<String>,      // which model responded
    pub duration_ms: Option<u64>,   // generation time
}

pub enum Role {
    User,
    Assistant,
}

pub enum Part {
    Text(TextPart),
    Reasoning(ReasoningPart),
    Tool(ToolPart),
    File(FilePart),
}

pub struct TextPart {
    pub id: Ulid,
    pub text: String,
    pub is_streaming: bool,
}

pub struct ReasoningPart {
    pub id: Ulid,
    pub text: String,
}

pub struct ToolPart {
    pub id: Ulid,
    pub tool: String,
    pub input: serde_json::Value,
    pub state: ToolState,
    pub metadata: ToolMetadata,
}

pub enum ToolState {
    Pending,
    Running,
    Complete,
    Error(String),
    AwaitingPermission,
}

pub struct ToolMetadata {
    pub title: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
}

pub struct FilePart {
    pub id: Ulid,
    pub filename: String,
    pub mime: String,
    pub data: Vec<u8>,
}
```

### Config
```toml
# ~/.config/hyperlite/settings.toml

theme = "tokyonight"
model = "anthropic/claude-sonnet-4-6"
provider = "anthropic"
agent = "default"
animations = true
scroll_speed = 3
sidebar = "auto"           # "auto" | "always" | "never"
terminal_title = true
thinking = false           # show reasoning by default
tool_details = true        # expand tool blocks by default

[keybinds]
# Override any keybind:
# submit = "ctrl+enter"
# new_session = "ctrl+n"

[providers.anthropic]
api_key = "${ANTHROPIC_API_KEY}"  # env var interpolation

[providers.openai]
api_key = "${OPENAI_API_KEY}"

[providers.ollama]
base_url = "http://localhost:11434"

[permissions]
# Pre-grant specific patterns:
# [[permissions.rules]]
# tool = "bash"
# pattern = "git *"
# action = "allow"

[agents.default]
name = "default"
model = "anthropic/claude-sonnet-4-6"

[agents.fast]
name = "fast"
model = "anthropic/claude-haiku-4-5"
```

---

## 11. Provider System

### Provider Trait
```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn models(&self) -> Vec<ModelInfo>;
    async fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        model: &str,
        config: &GenerationConfig,
    ) -> Result<impl Stream<Item = StreamEvent>>;
}

pub enum StreamEvent {
    Token(String),
    Reasoning(String),
    ToolCall(ToolCall),
    Done(CompletionMetadata),
    Error(ProviderError),
}
```

### Built-in Providers
| ID            | Name           | Auth                    | Notes                          |
|---------------|----------------|-------------------------|--------------------------------|
| `anthropic`   | Anthropic      | `ANTHROPIC_API_KEY`     | Claude family                  |
| `openai`      | OpenAI         | `OPENAI_API_KEY`        | GPT family + o-series          |
| `google`      | Google         | `GEMINI_API_KEY`        | Gemini family                  |
| `ollama`      | Ollama         | None (local)            | All local models               |
| `openrouter`  | OpenRouter     | `OPENROUTER_API_KEY`    | Aggregator, 100s of models     |
| `azure`       | Azure OpenAI   | `AZURE_OPENAI_API_KEY`  | Enterprise Azure               |
| `groq`        | Groq           | `GROQ_API_KEY`          | Fast inference                 |
| `mistral`     | Mistral        | `MISTRAL_API_KEY`       | Mistral family                 |

---

## 12. Built-in Tool Definitions

Tools called by the LLM (model generates JSON, app executes):

| Tool         | Description                               | Permission Required |
|--------------|-------------------------------------------|---------------------|
| `bash`       | Execute shell command                     | Yes (ask)           |
| `read`       | Read file contents                        | No                  |
| `write`      | Write/create file                         | Yes (ask)           |
| `edit`       | Edit file (search/replace or patch)       | Yes (ask)           |
| `glob`       | Find files by pattern                     | No                  |
| `grep`       | Search file contents with regex           | No                  |
| `list`       | List directory contents                   | No                  |
| `webfetch`   | Fetch URL content                         | Yes (ask)           |
| `websearch`  | Search the web                            | Yes (ask)           |
| `task`       | Create/update todo list items             | No                  |

---

## 13. Session Management Features

| Feature       | Description |
|---------------|-------------|
| **New session** | Clears context, starts fresh in same directory |
| **Fork**       | Creates child session branching from a specific message |
| **Undo/Revert** | Rolls back messages + all file changes to pre-message state (via snapshot) |
| **Compact**    | Summarizes old context to reduce token count while preserving recent messages |
| **Rename**     | Custom session title instead of auto-generated |
| **Delete**     | Removes session + all messages from DB |
| **Export**     | Exports session as markdown transcript |
| **Share**      | Publishes session to cloud (requires account) |
| **Parent/Child navigation** | Navigate between forked session tree |
| **Interrupt**  | Ctrl+C twice cancels in-flight generation |

---

## 14. Streaming and State Management

### App State Machine
```
Idle
  → Generating (user submitted, waiting for response)
      → StreamingText (receiving tokens)
      → StreamingTool (tool call in progress)
          → AwaitingPermission (tool needs user approval)
          → ExecutingTool (tool running)
      → Done (generation complete)
  → PermissionPending (outside generation, stacked permissions)
  → QuestionPending (agent asked structured question)
```

### Scroll State
```rust
pub struct ScrollState {
    pub offset: u16,           // lines from top
    pub total_lines: u16,      // total rendered lines
    pub viewport_height: u16,
    pub auto_scroll: bool,     // sticky bottom
    pub acceleration: f32,     // scroll speed multiplier
}
```

Auto-scroll re-engages when `offset == total_lines - viewport_height`.

### Prompt History
- Last N prompts stored in DB per session
- `↑` / `↓` in empty input navigates history
- Stash: current draft saved when navigating history, restored on `↓` past end

---

## 15. File Attachment System

- Drag-and-drop files onto terminal: detected via clipboard or path argument
- `/attach path/to/file` slash command
- Ctrl+V pastes image directly (from clipboard)
- Supported: images (PNG/JPG/GIF/WEBP), PDFs, plain text, code files
- Displayed as inline badges before submission
- Sent as multipart content to providers that support vision

---

## 16. Configuration File Locations

| Platform | Global Config                            | Project Config              |
|----------|------------------------------------------|-----------------------------|
| Linux    | `~/.config/hyperlite/settings.toml`      | `.hyperlite/settings.toml`  |
| macOS    | `~/.config/hyperlite/settings.toml`      | `.hyperlite/settings.toml`  |
| Windows  | `%APPDATA%\hyperlite\settings.toml`      | `.hyperlite\settings.toml`  |

**DB location:**
| Platform | Path |
|----------|------|
| Linux    | `~/.local/share/hyperlite/history.db` |
| macOS    | `~/Library/Application Support/hyperlite/history.db` |
| Windows  | `%APPDATA%\hyperlite\history.db` |

---

## 17. Performance Targets

| Metric                    | Target     |
|---------------------------|------------|
| Cold startup to first paint | < 50ms   |
| Binary size (release)     | < 15 MB    |
| Memory at idle            | < 30 MB    |
| Memory under load         | < 80 MB    |
| Input → render latency    | < 16ms (60fps) |
| Token streaming FPS       | ≥ 30fps    |

**Optimizations:**
- Release builds with `opt-level = 3`, `lto = true`, `codegen-units = 1`
- `strip = true` in release profile
- Lazy-load syntect grammars (only load on first use of each language)
- SQLite WAL mode for non-blocking reads during writes
- Message rendering cache: re-render only changed messages
- Diff rendering cached per permission request

---

## 18. CLI Arguments

```
hyperlite [OPTIONS] [PROMPT]

ARGS:
  [PROMPT]    Optional initial prompt (skips input, sends immediately)

OPTIONS:
  -m, --model <MODEL>       Override model (e.g. anthropic/claude-sonnet-4-6)
  -p, --provider <PROV>     Override provider
  -t, --theme <THEME>       Override theme
  -d, --dir <DIR>           Working directory (default: cwd)
  -s, --session <ID>        Resume session by ID
      --no-sidebar          Disable sidebar
      --no-animations       Disable spinner/animations
      --headless            Non-interactive mode (pipe input/output)
  -v, --verbose             Enable debug logging to file
  -h, --help                Show help
  -V, --version             Show version
```

---

## 19. Status Dialog (/status)

```
  ┌─ Status ─────────────────────────────────────┐
  │ HyperLite v0.1.0                             │
  │                                              │
  │ Providers                                    │
  │ ● Anthropic         connected                │
  │ ● OpenAI            connected                │
  │ ○ Google            no API key               │
  │ ● Ollama            2 models available       │
  │                                              │
  │ LSP                                          │
  │ ● rust-analyzer     running (pid 12345)      │
  │ ● typescript-lsp    running (pid 12346)      │
  │                                              │
  │ MCP                                          │
  │ ● filesystem        connected                │
  │ ✗ my-mcp-server     error: timeout           │
  │                                              │
  │ Session                                      │
  │   Messages: 42    Tokens: ~18,500            │
  │   Working dir: ~/projects/myapp              │
  └──────────────────────────────────────────────┘
```

---

## 20. Home / Welcome Screen

Shown when no active session:
```

              ██╗  ██╗██╗   ██╗██████╗ ███████╗██████╗
              ██║  ██║╚██╗ ██╔╝██╔══██╗██╔════╝██╔══██╗
              ███████║ ╚████╔╝ ██████╔╝█████╗  ██████╔╝
              ██╔══██║  ╚██╔╝  ██╔═══╝ ██╔══╝  ██╔══██╗
              ██║  ██║   ██║   ██║     ███████╗██║  ██║
              ╚═╝  ╚═╝   ╚═╝   ╚═╝     ╚══════╝╚═╝  ╚═╝
                          L I T E

              ┌──────────────────────────────────────────┐
              │ Fix a TODO in the codebase...            │
              └──────────────────────────────────────────┘
              Ctrl+K commands   ? help   Ctrl+N new session
```
- Logo centered, max 75 cols wide for prompt
- Placeholder text cycles through examples every 3 seconds
- Minimal hint bar below

---

## 21. Markdown Rendering Spec

Rendered inline within assistant text parts using pulldown-cmark events:

| Element        | Rendering                                  |
|----------------|--------------------------------------------|
| `**bold**`     | `Style::Bold`                              |
| `_italic_`     | `Style::Italic` (if terminal supports)     |
| `` `code` ``   | Monospace, `theme.primary` fg              |
| `# H1`         | Bold + underline, newline above/below      |
| `## H2`        | Bold, newline above/below                  |
| `### H3`       | Bold, newline above                        |
| `- list`       | `  • item` indented                        |
| `1. list`      | `  1. item` numbered                       |
| `> quote`      | `┃ ` prefix in `text_muted`               |
| ` ```lang` ` ` | Syntect-highlighted block, box border      |
| `[link](url)`  | Text in `theme.primary`, url in `text_muted` |
| `---`          | Full-width `─` line in `theme.border`      |
| `\n\n`         | Empty line between paragraphs              |

Code blocks: bordered box, language tag in top-right, syntect highlighting.
- Concealment toggle hides code block content (shows language + line count only)

---

## 22. Accessibility & Terminal Compatibility

- Works in any true-color terminal (detected, falls back to 256-color, then 16-color)
- Mouse support: optional, disabled if terminal doesn't report mouse events
- Wide character (CJK) support via `unicode-width`
- Respects `NO_COLOR` environment variable
- Respects `TERM`, `COLORTERM` for capability detection
- Works over SSH
- Works in tmux / screen (with true-color passthrough)

---

## 23. Build Configuration

```toml
# Cargo.toml [profile.release]
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

Target binaries:
- `x86_64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-pc-windows-msvc`

---

*End of HyperLite Specification v1.0*
