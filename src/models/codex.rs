/// Model Capability Codex
///
/// A static database of known model families with:
///   - Capability tags (coding, writing, reasoning, search, vision, math, fast)
///   - Hardware requirements by quantization
///   - Recommended use cases
///   - Known model IDs / filename patterns for auto-detection
///
/// Used to match detected local model files to their capabilities and
/// recommend models based on the user's hardware and intent.

use crate::hardware::{HardwareInfo, ModelSizeTier};
use serde::{Deserialize, Serialize};

// ── Capability tags ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Code generation, completion, debugging, refactoring
    Coding,
    /// Long-form writing, creative writing, editing
    Writing,
    /// Chain-of-thought, step-by-step reasoning, logic puzzles
    Reasoning,
    /// Web search via tool use / function calling
    Search,
    /// Image understanding (multimodal)
    Vision,
    /// Mathematics, symbolic reasoning
    Math,
    /// Multiple natural languages
    Multilingual,
    /// Optimized for fast inference / small footprint
    Fast,
    /// Instruction following, chat
    Chat,
    /// Long context window (≥ 32K tokens)
    LongContext,
    /// Function calling / tool use
    ToolUse,
    /// Roleplay / creative fiction
    Roleplay,
    /// Data analysis, structured output
    DataAnalysis,
}

impl Capability {
    pub fn icon(&self) -> &'static str {
        match self {
            Capability::Coding      => "⌨",
            Capability::Writing     => "✍",
            Capability::Reasoning   => "🧠",
            Capability::Search      => "◈",
            Capability::Vision      => "👁",
            Capability::Math        => "∑",
            Capability::Multilingual=> "🌐",
            Capability::Fast        => "⚡",
            Capability::Chat        => "💬",
            Capability::LongContext => "📜",
            Capability::ToolUse     => "⚙",
            Capability::Roleplay    => "🎭",
            Capability::DataAnalysis=> "📊",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Capability::Coding      => "Coding",
            Capability::Writing     => "Writing",
            Capability::Reasoning   => "Reasoning",
            Capability::Search      => "Search",
            Capability::Vision      => "Vision",
            Capability::Math        => "Math",
            Capability::Multilingual=> "Multilingual",
            Capability::Fast        => "Fast",
            Capability::Chat        => "Chat",
            Capability::LongContext => "Long Context",
            Capability::ToolUse     => "Tool Use",
            Capability::Roleplay    => "Roleplay",
            Capability::DataAnalysis=> "Data Analysis",
        }
    }
}

// ── Quality tier ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QualityTier {
    Research,   // experimental / niche
    Good,       // solid for most tasks
    Great,      // recommended for power users
    Frontier,   // state-of-the-art
}

// ── Model family entry ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ModelFamily {
    /// Display name
    pub name:            &'static str,
    /// Common filename patterns (case-insensitive substring match)
    pub patterns:        &'static [&'static str],
    /// Creator / organization
    pub creator:         &'static str,
    /// Short description
    pub description:     &'static str,
    /// Recommended use cases ordered by priority
    pub capabilities:    &'static [Capability],
    /// Quality tier at full precision
    pub quality:         QualityTier,
    /// Context window in tokens
    pub context_tokens:  u32,
    /// VRAM required in MB at Q4_K_M for various param sizes
    /// Format: &[(param_billions_x10, vram_mb)]  (e.g. 70 = 7B, 130 = 13B)
    pub vram_q4:         &'static [(u32, u32)],
    /// Whether this family supports native function/tool calling
    pub tool_calling:    bool,
    /// Whether this family supports reasoning/thinking mode
    pub reasoning_mode:  bool,
}

// ── CODEX ─────────────────────────────────────────────────────────────────────

pub static CODEX: &[ModelFamily] = &[

    // ── CODING SPECIALISTS ────────────────────────────────────────────────────

    ModelFamily {
        name:         "Qwen2.5-Coder",
        patterns:     &["qwen2.5-coder", "qwen2-5-coder", "qwencoder"],
        creator:      "Alibaba",
        description:  "State-of-the-art coding model. Best open-source for code generation, completion, and debugging.",
        capabilities: &[Capability::Coding, Capability::ToolUse, Capability::DataAnalysis, Capability::Chat],
        quality:      QualityTier::Frontier,
        context_tokens: 131072,
        vram_q4:      &[(5, 3500), (70, 4200), (140, 8500), (320, 19000)],
        tool_calling: true,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "DeepSeek-Coder-V2",
        patterns:     &["deepseek-coder-v2", "dscoder", "deepseek-coder"],
        creator:      "DeepSeek",
        description:  "Mixture-of-experts coding model. Excellent at complex algorithms and multi-file edits.",
        capabilities: &[Capability::Coding, Capability::Math, Capability::Reasoning, Capability::ToolUse],
        quality:      QualityTier::Frontier,
        context_tokens: 128000,
        vram_q4:      &[(160, 9500), (360, 21000)],
        tool_calling: true,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "CodeLlama",
        patterns:     &["codellama", "code-llama", "code_llama"],
        creator:      "Meta",
        description:  "Meta's coding-focused Llama variant. Good for completions and infilling.",
        capabilities: &[Capability::Coding, Capability::Chat],
        quality:      QualityTier::Good,
        context_tokens: 100000,
        vram_q4:      &[(70, 4200), (130, 7800), (340, 19500)],
        tool_calling: false,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "Starcoder2",
        patterns:     &["starcoder2", "star-coder"],
        creator:      "BigCode",
        description:  "Trained on 600+ programming languages. Strong for uncommon languages and legacy code.",
        capabilities: &[Capability::Coding, Capability::Fast],
        quality:      QualityTier::Good,
        context_tokens: 16384,
        vram_q4:      &[(30, 2100), (70, 4200), (150, 9000)],
        tool_calling: false,
        reasoning_mode: false,
    },

    // ── REASONING SPECIALISTS ─────────────────────────────────────────────────

    ModelFamily {
        name:         "DeepSeek-R1",
        patterns:     &["deepseek-r1", "deepseek_r1", "r1-distill"],
        creator:      "DeepSeek",
        description:  "Extended chain-of-thought reasoning. Best open-source for math, logic, and hard problems.",
        capabilities: &[Capability::Reasoning, Capability::Math, Capability::Coding, Capability::DataAnalysis],
        quality:      QualityTier::Frontier,
        context_tokens: 65536,
        vram_q4:      &[(15, 9500), (70, 4200), (320, 19000), (6710, 40000)],
        tool_calling: false,
        reasoning_mode: true,
    },

    ModelFamily {
        name:         "QwQ",
        patterns:     &["qwq-32", "qwq_32", "qwq"],
        creator:      "Alibaba",
        description:  "Qwen's reasoning model. Exceptional math and scientific reasoning with visible thinking.",
        capabilities: &[Capability::Reasoning, Capability::Math, Capability::Coding],
        quality:      QualityTier::Frontier,
        context_tokens: 131072,
        vram_q4:      &[(320, 19000)],
        tool_calling: false,
        reasoning_mode: true,
    },

    ModelFamily {
        name:         "Phi-4",
        patterns:     &["phi-4", "phi4"],
        creator:      "Microsoft",
        description:  "Small but surprisingly capable. Excellent reasoning per parameter.",
        capabilities: &[Capability::Reasoning, Capability::Math, Capability::Coding, Capability::Fast],
        quality:      QualityTier::Great,
        context_tokens: 16384,
        vram_q4:      &[(140, 8500)],
        tool_calling: true,
        reasoning_mode: false,
    },

    // ── GENERAL / WRITING ─────────────────────────────────────────────────────

    ModelFamily {
        name:         "Llama 3.3",
        patterns:     &["llama-3.3", "llama3.3", "llama-3-3", "llama_3.3"],
        creator:      "Meta",
        description:  "Meta's flagship. Best all-around open model for writing, chat, and instruction following.",
        capabilities: &[Capability::Chat, Capability::Writing, Capability::Coding, Capability::Reasoning, Capability::ToolUse],
        quality:      QualityTier::Frontier,
        context_tokens: 131072,
        vram_q4:      &[(700, 43000)],
        tool_calling: true,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "Llama 3.1 / 3.2",
        patterns:     &["llama-3.1", "llama-3.2", "llama3.1", "llama3.2", "llama-3-1", "llama-3-2"],
        creator:      "Meta",
        description:  "Meta's Llama 3 family. Excellent general-purpose models from 1B to 405B.",
        capabilities: &[Capability::Chat, Capability::Writing, Capability::Coding, Capability::Multilingual, Capability::ToolUse],
        quality:      QualityTier::Great,
        context_tokens: 131072,
        vram_q4:      &[(10, 700), (30, 2100), (80, 4900), (110, 6700), (700, 43000), (4050, 248000)],
        tool_calling: true,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "Mistral / Mixtral",
        patterns:     &["mistral-7", "mistral-nemo", "mistral-small", "mixtral", "mistral-instruct"],
        creator:      "Mistral AI",
        description:  "Fast, strong instruction following. Mixtral MoE punches well above its weight.",
        capabilities: &[Capability::Chat, Capability::Writing, Capability::Coding, Capability::Fast, Capability::ToolUse],
        quality:      QualityTier::Great,
        context_tokens: 32768,
        vram_q4:      &[(70, 4200), (87, 5500), (470, 29000)],
        tool_calling: true,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "Qwen2.5",
        patterns:     &["qwen2.5-", "qwen2-5-", "qwen2_5", "qwen-2.5"],
        creator:      "Alibaba",
        description:  "Strong multilingual model with good coding. Huge context window.",
        capabilities: &[Capability::Chat, Capability::Multilingual, Capability::Coding, Capability::Writing, Capability::ToolUse, Capability::LongContext],
        quality:      QualityTier::Great,
        context_tokens: 131072,
        vram_q4:      &[(5, 3500), (15, 9500), (30, 19000), (72, 43000)],
        tool_calling: true,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "Gemma 3",
        patterns:     &["gemma-3", "gemma3", "gemma_3"],
        creator:      "Google",
        description:  "Google's open model. Excellent instruction following and multilingual ability.",
        capabilities: &[Capability::Chat, Capability::Writing, Capability::Multilingual, Capability::Vision],
        quality:      QualityTier::Great,
        context_tokens: 128000,
        vram_q4:      &[(10, 700), (40, 2500), (120, 7200), (270, 16500)],
        tool_calling: true,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "Phi-3 / Phi-3.5",
        patterns:     &["phi-3", "phi-3.5", "phi3", "phi3.5"],
        creator:      "Microsoft",
        description:  "Highly capable small models. Excellent quality-to-size ratio.",
        capabilities: &[Capability::Chat, Capability::Coding, Capability::Reasoning, Capability::Fast],
        quality:      QualityTier::Great,
        context_tokens: 128000,
        vram_q4:      &[(38, 2300), (75, 4600), (140, 8500)],
        tool_calling: true,
        reasoning_mode: false,
    },

    // ── TOOL USE / SEARCH SPECIALISTS ─────────────────────────────────────────

    ModelFamily {
        name:         "Hermes 3 / NousHermes",
        patterns:     &["hermes-3", "hermes3", "nous-hermes", "noushermes"],
        creator:      "Nous Research",
        description:  "Fine-tuned for tool use, JSON output, and agent workflows. Best for search/tool-heavy tasks.",
        capabilities: &[Capability::ToolUse, Capability::Search, Capability::DataAnalysis, Capability::Coding, Capability::Chat],
        quality:      QualityTier::Great,
        context_tokens: 131072,
        vram_q4:      &[(80, 4900), (700, 43000)],
        tool_calling: true,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "Functionary",
        patterns:     &["functionary"],
        creator:      "MeetKai",
        description:  "Specialized for function calling and structured tool use. Excellent API/agent integration.",
        capabilities: &[Capability::ToolUse, Capability::Search, Capability::DataAnalysis],
        quality:      QualityTier::Good,
        context_tokens: 32768,
        vram_q4:      &[(70, 4200), (130, 7800)],
        tool_calling: true,
        reasoning_mode: false,
    },

    // ── CREATIVE / WRITING SPECIALISTS ───────────────────────────────────────

    ModelFamily {
        name:         "Command-R",
        patterns:     &["command-r", "commandr", "c4ai-command"],
        creator:      "Cohere",
        description:  "Optimized for retrieval-augmented generation and long-document writing.",
        capabilities: &[Capability::Writing, Capability::Search, Capability::LongContext, Capability::Multilingual],
        quality:      QualityTier::Great,
        context_tokens: 128000,
        vram_q4:      &[(350, 21000), (1040, 63000)],
        tool_calling: true,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "WizardLM / Evol-Instruct",
        patterns:     &["wizardlm", "wizard-lm", "evol-instruct", "wizardcoder"],
        creator:      "WizardLM Team",
        description:  "Strong instruction following, especially for complex multi-step requests.",
        capabilities: &[Capability::Writing, Capability::Coding, Capability::Chat],
        quality:      QualityTier::Good,
        context_tokens: 8192,
        vram_q4:      &[(70, 4200), (130, 7800), (700, 43000)],
        tool_calling: false,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "Dolphin / Nous-Capybara",
        patterns:     &["dolphin", "nous-capybara", "capybara"],
        creator:      "Eric Hartford / Nous Research",
        description:  "Uncensored, helpful for creative fiction and roleplay.",
        capabilities: &[Capability::Roleplay, Capability::Writing, Capability::Chat],
        quality:      QualityTier::Good,
        context_tokens: 32768,
        vram_q4:      &[(70, 4200), (130, 7800), (700, 43000)],
        tool_calling: false,
        reasoning_mode: false,
    },

    // ── FAST / EDGE MODELS ────────────────────────────────────────────────────

    ModelFamily {
        name:         "SmolLM2",
        patterns:     &["smollm2", "smol-lm2", "smollm"],
        creator:      "HuggingFace",
        description:  "Tiny but capable. Runs on CPU in real-time. Great for low-end hardware.",
        capabilities: &[Capability::Fast, Capability::Chat, Capability::Coding],
        quality:      QualityTier::Research,
        context_tokens: 8192,
        vram_q4:      &[(1, 600), (17, 1100), (35, 2200)],
        tool_calling: false,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "TinyLlama",
        patterns:     &["tinyllama", "tiny-llama"],
        creator:      "TinyLlama Team",
        description:  "1.1B model that runs anywhere. Ultra-fast inference.",
        capabilities: &[Capability::Fast, Capability::Chat],
        quality:      QualityTier::Research,
        context_tokens: 2048,
        vram_q4:      &[(11, 700)],
        tool_calling: false,
        reasoning_mode: false,
    },

    // ── MULTIMODAL / VISION ───────────────────────────────────────────────────

    ModelFamily {
        name:         "LLaVA / BakLLaVA",
        patterns:     &["llava", "bakllava", "llava-v1", "llava1.5", "llava-1.5"],
        creator:      "Liu et al.",
        description:  "Visual question answering and image understanding.",
        capabilities: &[Capability::Vision, Capability::Chat],
        quality:      QualityTier::Good,
        context_tokens: 4096,
        vram_q4:      &[(70, 4700), (130, 8300)],
        tool_calling: false,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "MiniCPM-V",
        patterns:     &["minicpm-v", "minicpm_v", "minicpmv"],
        creator:      "OpenBMB",
        description:  "Compact multimodal model. Handles images, PDFs, and video frames efficiently.",
        capabilities: &[Capability::Vision, Capability::Fast, Capability::Chat],
        quality:      QualityTier::Great,
        context_tokens: 32768,
        vram_q4:      &[(26, 1700), (82, 5000)],
        tool_calling: false,
        reasoning_mode: false,
    },

    // ── LEGACY / CLASSIC ─────────────────────────────────────────────────────

    ModelFamily {
        name:         "Llama 2",
        patterns:     &["llama-2", "llama2", "llama_2"],
        creator:      "Meta",
        description:  "Previous Llama generation. Widely compatible, works with all backends.",
        capabilities: &[Capability::Chat, Capability::Writing],
        quality:      QualityTier::Good,
        context_tokens: 4096,
        vram_q4:      &[(70, 4200), (130, 7800), (700, 43000)],
        tool_calling: false,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "GPT-J / GPT-NeoX / Pythia",
        patterns:     &["gpt-j", "gptj", "gpt-neo", "neox", "pythia"],
        creator:      "EleutherAI",
        description:  "Early open GPT models. Legacy GGML format. Use KoboldCpp for best compatibility.",
        capabilities: &[Capability::Chat, Capability::Writing],
        quality:      QualityTier::Research,
        context_tokens: 2048,
        vram_q4:      &[(60, 3800), (200, 12500)],
        tool_calling: false,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "RWKV",
        patterns:     &["rwkv"],
        creator:      "BlinkDL",
        description:  "RNN-based LLM. Infinite context, constant memory usage. Use KoboldCpp.",
        capabilities: &[Capability::LongContext, Capability::Fast, Capability::Chat],
        quality:      QualityTier::Good,
        context_tokens: 0,  // infinite
        vram_q4:      &[(14, 900), (30, 1900), (70, 4500), (140, 9000)],
        tool_calling: false,
        reasoning_mode: false,
    },

    ModelFamily {
        name:         "Falcon",
        patterns:     &["falcon"],
        creator:      "TII UAE",
        description:  "Early powerful open model. Trained on large multilingual corpus.",
        capabilities: &[Capability::Chat, Capability::Writing, Capability::Multilingual],
        quality:      QualityTier::Good,
        context_tokens: 2048,
        vram_q4:      &[(70, 4500), (400, 25000), (1800, 110000)],
        tool_calling: false,
        reasoning_mode: false,
    },
];

// ── Matching + Recommendations ────────────────────────────────────────────────

/// Try to match a model file name / ID to a known model family.
pub fn identify(model_name: &str) -> Option<&'static ModelFamily> {
    let lower = model_name.to_lowercase();
    CODEX.iter().find(|family| {
        family.patterns.iter().any(|pat| lower.contains(pat))
    })
}

/// Find model families matching a capability need, filtered by hardware tier.
pub fn recommend_for_capability(
    capability: &Capability,
    hw:         &HardwareInfo,
) -> Vec<(&'static ModelFamily, Vec<&'static str>)> {
    let tier = hw.max_model_tier();
    let vram = if hw.best_vram_mb > 0 { hw.best_vram_mb } else { hw.memory.available_mb / 2 };

    let mut results: Vec<(&'static ModelFamily, Vec<&'static str>)> = CODEX
        .iter()
        .filter(|f| f.capabilities.contains(capability))
        .filter_map(|f| {
            // Find viable param sizes given VRAM
            let viable: Vec<&'static str> = f.vram_q4.iter()
                .filter(|(_, req_mb)| vram >= *req_mb as u64)
                .map(|(pb, _)| param_billions_label(*pb))
                .collect();
            if viable.is_empty() { None } else { Some((f, viable)) }
        })
        .collect();

    // Sort by quality tier descending
    results.sort_by(|a, b| b.0.quality.cmp(&a.0.quality));
    results
}

/// Recommend best models for all key capabilities given the hardware.
pub fn full_recommendations(hw: &HardwareInfo) -> RecommendationSet {
    RecommendationSet {
        coding:    recommend_for_capability(&Capability::Coding,    hw),
        writing:   recommend_for_capability(&Capability::Writing,   hw),
        reasoning: recommend_for_capability(&Capability::Reasoning, hw),
        search:    recommend_for_capability(&Capability::ToolUse,   hw),
        fast:      recommend_for_capability(&Capability::Fast,      hw),
        vision:    recommend_for_capability(&Capability::Vision,    hw),
    }
}

fn param_billions_label(pb_x10: u32) -> &'static str {
    // pb_x10 is param count in units of 100M  (e.g. 70 = 7B, 130 = 13B, 700 = 70B)
    match pb_x10 {
        1..=9    => "~1B",
        10..=19  => "1B",
        20..=49  => "3B",
        50..=99  => "7B",
        100..=149 => "13B",
        150..=249 => "20B",
        250..=399 => "32B",
        400..=799 => "70B",
        _         => "70B+",
    }
}

#[derive(Debug)]
pub struct RecommendationSet {
    pub coding:    Vec<(&'static ModelFamily, Vec<&'static str>)>,
    pub writing:   Vec<(&'static ModelFamily, Vec<&'static str>)>,
    pub reasoning: Vec<(&'static ModelFamily, Vec<&'static str>)>,
    pub search:    Vec<(&'static ModelFamily, Vec<&'static str>)>,
    pub fast:      Vec<(&'static ModelFamily, Vec<&'static str>)>,
    pub vision:    Vec<(&'static ModelFamily, Vec<&'static str>)>,
}

impl RecommendationSet {
    /// Top pick for a given use case label ("coding", "writing", etc.)
    pub fn top_for(&self, use_case: &str) -> Option<&(&'static ModelFamily, Vec<&'static str>)> {
        match use_case {
            "coding"    => self.coding.first(),
            "writing"   => self.writing.first(),
            "reasoning" => self.reasoning.first(),
            "search"    => self.search.first(),
            "fast"      => self.fast.first(),
            "vision"    => self.vision.first(),
            _           => None,
        }
    }
}
