/// Hardware Detection Module
///
/// Detects CPU, RAM, GPU (NVIDIA/AMD/Apple Silicon/Intel Arc),
/// and VRAM to recommend appropriate models and quantizations.

use serde::{Deserialize, Serialize};
use std::process::Command;
use sysinfo::System;

// ── Structs ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu:        CpuInfo,
    pub memory:     MemoryInfo,
    pub gpus:       Vec<GpuInfo>,
    pub platform:   Platform,
    /// Effective usable VRAM for inference (best GPU, or 0 if CPU-only)
    pub best_vram_mb: u64,
    /// Whether CPU-only inference is likely (no usable GPU)
    pub cpu_only:   bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub name:           String,
    pub physical_cores: usize,
    pub logical_cores:  usize,
    pub arch:           CpuArch,
    pub is_apple_silicon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_mb:     u64,
    pub available_mb: u64,
    /// On Apple Silicon unified memory counts as both RAM and VRAM
    pub is_unified:   bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name:         String,
    pub vendor:       GpuVendor,
    pub vram_total_mb: u64,
    pub vram_free_mb:  u64,
    pub driver:       Option<String>,
    pub cuda_version: Option<String>,  // NVIDIA only
    pub rocm_version: Option<String>,  // AMD only
    pub compute_cap:  Option<String>,  // NVIDIA CUDA compute capability
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Apple,  // Unified memory — not a discrete GPU
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CpuArch {
    X86_64,
    Aarch64,  // ARM64 / Apple Silicon
    X86,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Platform {
    Linux,
    MacOs,
    Windows,
}

// ── Model size tiers derived from hardware ────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModelSizeTier {
    /// ≤ 1B params  — fits in 2GB VRAM or 4GB RAM
    Tiny,
    /// 1–4B params  — fits in 3GB VRAM or 8GB RAM
    Small,
    /// 4–8B params  — fits in 5GB VRAM or 16GB RAM
    Medium,
    /// 8–14B params — fits in 9GB VRAM or 24GB RAM
    Large,
    /// 14–35B params — fits in 16GB VRAM or 48GB RAM
    XLarge,
    /// 35–70B params — fits in 32GB VRAM or 80GB RAM
    Huge,
    /// 70B+ params  — needs 48GB+ VRAM or 128GB+ RAM
    Massive,
}

impl ModelSizeTier {
    pub fn label(&self) -> &'static str {
        match self {
            ModelSizeTier::Tiny    => "~1B",
            ModelSizeTier::Small   => "1–4B",
            ModelSizeTier::Medium  => "4–8B",
            ModelSizeTier::Large   => "8–14B",
            ModelSizeTier::XLarge  => "14–35B",
            ModelSizeTier::Huge    => "35–70B",
            ModelSizeTier::Massive => "70B+",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            ModelSizeTier::Tiny    => "Ultra-fast, minimal quality",
            ModelSizeTier::Small   => "Fast, decent quality",
            ModelSizeTier::Medium  => "Balanced speed & quality",
            ModelSizeTier::Large   => "High quality, moderate speed",
            ModelSizeTier::XLarge  => "Very high quality, slow on CPU",
            ModelSizeTier::Huge    => "Near-frontier quality",
            ModelSizeTier::Massive => "Frontier quality, needs server-grade GPU",
        }
    }
}

// ── Detection ─────────────────────────────────────────────────────────────────

/// Detect all hardware. Non-blocking, best-effort — never panics.
pub fn detect() -> HardwareInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let platform = detect_platform();
    let cpu      = detect_cpu(&sys);
    let memory   = detect_memory(&sys, &cpu, &platform);
    let mut gpus = vec![];

    // NVIDIA
    gpus.extend(detect_nvidia());
    // AMD
    gpus.extend(detect_amd());
    // Intel Arc (Linux via sysfs)
    #[cfg(target_os = "linux")]
    gpus.extend(detect_intel_arc());
    // Apple Silicon — unified memory counts as GPU memory
    if cpu.is_apple_silicon {
        gpus.push(apple_silicon_gpu(&memory));
    }

    // Determine best available VRAM
    let best_vram_mb = gpus.iter()
        .map(|g| g.vram_total_mb)
        .max()
        .unwrap_or(0);

    let cpu_only = gpus.is_empty()
        || gpus.iter().all(|g| g.vram_total_mb < 2048);  // < 2GB unusable

    HardwareInfo { cpu, memory, gpus, platform, best_vram_mb, cpu_only }
}

fn detect_platform() -> Platform {
    match std::env::consts::OS {
        "linux"   => Platform::Linux,
        "macos"   => Platform::MacOs,
        "windows" => Platform::Windows,
        _         => Platform::Linux,
    }
}

fn detect_cpu(sys: &System) -> CpuInfo {
    let cpus = sys.cpus();
    let name = cpus.first()
        .map(|c| c.brand().to_string())
        .unwrap_or_else(|| "Unknown CPU".into());

    let arch = match std::env::consts::ARCH {
        "x86_64"  => CpuArch::X86_64,
        "aarch64" => CpuArch::Aarch64,
        "x86"     => CpuArch::X86,
        _         => CpuArch::Unknown,
    };

    let is_apple_silicon = arch == CpuArch::Aarch64
        && std::env::consts::OS == "macos";

    CpuInfo {
        physical_cores:  sys.physical_core_count().unwrap_or(1),
        logical_cores:   cpus.len(),
        name,
        arch,
        is_apple_silicon,
    }
}

fn detect_memory(sys: &System, cpu: &CpuInfo, _platform: &Platform) -> MemoryInfo {
    let total_mb     = sys.total_memory() / 1024 / 1024;
    let available_mb = sys.available_memory() / 1024 / 1024;
    let is_unified   = cpu.is_apple_silicon;
    MemoryInfo { total_mb, available_mb, is_unified }
}

// ── NVIDIA detection ──────────────────────────────────────────────────────────

fn detect_nvidia() -> Vec<GpuInfo> {
    // Query nvidia-smi for all GPUs
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,memory.free,driver_version,compute_cap",
            "--format=csv,noheader,nounits",
        ])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut gpus = vec![];

    for line in text.lines() {
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() < 3 { continue; }

        let vram_total = parts[1].parse::<u64>().unwrap_or(0);
        let vram_free  = parts[2].parse::<u64>().unwrap_or(0);
        let driver     = parts.get(3).map(|s| s.to_string());
        let compute    = parts.get(4).map(|s| s.to_string());

        // Also try to get CUDA version
        let cuda_version = detect_cuda_version();

        gpus.push(GpuInfo {
            name:          parts[0].to_string(),
            vendor:        GpuVendor::Nvidia,
            vram_total_mb: vram_total,
            vram_free_mb:  vram_free,
            driver,
            cuda_version,
            rocm_version:  None,
            compute_cap:   compute,
        });
    }

    gpus
}

fn detect_cuda_version() -> Option<String> {
    // nvidia-smi gives CUDA version in the header
    let output = Command::new("nvidia-smi").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // Look for "CUDA Version: X.Y"
    for line in text.lines() {
        if let Some(pos) = line.find("CUDA Version:") {
            let rest = line[pos + 13..].trim();
            let version = rest.split_whitespace().next().unwrap_or("").to_string();
            if !version.is_empty() { return Some(version); }
        }
    }
    // Try nvcc --version as fallback
    let output = Command::new("nvcc").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if line.contains("release") {
            if let Some(pos) = line.find("release ") {
                let rest = &line[pos + 8..];
                let ver = rest.split(',').next().unwrap_or("").trim().to_string();
                if !ver.is_empty() { return Some(ver); }
            }
        }
    }
    None
}

// ── AMD detection ─────────────────────────────────────────────────────────────

fn detect_amd() -> Vec<GpuInfo> {
    // Try rocm-smi
    let output = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram", "--csv"])
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            return parse_rocm_output(&String::from_utf8_lossy(&out.stdout));
        }
    }

    // Fallback: try /sys/class/drm on Linux for AMD
    #[cfg(target_os = "linux")]
    return detect_amd_sysfs();

    #[cfg(not(target_os = "linux"))]
    return vec![];
}

fn parse_rocm_output(text: &str) -> Vec<GpuInfo> {
    let mut gpus = vec![];
    let rocm_version = detect_rocm_version();

    // rocm-smi --showmeminfo vram --csv outputs:
    // device,VRAM Total Memory (B),VRAM Total Used Memory (B)
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 3 { continue; }

        let total_b = parts[1].trim().parse::<u64>().unwrap_or(0);
        let used_b  = parts[2].trim().parse::<u64>().unwrap_or(0);
        let total_mb = total_b / 1024 / 1024;
        let free_mb  = total_mb.saturating_sub(used_b / 1024 / 1024);

        // GPU name from device index
        let name = get_amd_gpu_name(parts[0].trim());

        gpus.push(GpuInfo {
            name,
            vendor:        GpuVendor::Amd,
            vram_total_mb: total_mb,
            vram_free_mb:  free_mb,
            driver:        None,
            cuda_version:  None,
            rocm_version:  rocm_version.clone(),
            compute_cap:   None,
        });
    }
    gpus
}

fn get_amd_gpu_name(device: &str) -> String {
    // Try rocm-smi --showproductname
    let output = Command::new("rocm-smi")
        .args(["--showproductname", "-d", device])
        .output();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("Card series:") || line.contains("GPU[") {
                if let Some(colon) = line.find(':') {
                    return line[colon + 1..].trim().to_string();
                }
            }
        }
    }
    format!("AMD GPU {}", device)
}

fn detect_rocm_version() -> Option<String> {
    let output = Command::new("rocm-smi").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find(|l| l.contains("ROCm"))
        .map(|l| l.trim().to_string())
}

#[cfg(target_os = "linux")]
fn detect_amd_sysfs() -> Vec<GpuInfo> {
    use std::fs;
    let mut gpus = vec![];

    let drm_path = std::path::Path::new("/sys/class/drm");
    if !drm_path.exists() { return gpus; }

    let entries = match fs::read_dir(drm_path) {
        Ok(e) => e,
        Err(_) => return gpus,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Only renderD* entries are GPUs
        if !name.starts_with("renderD") { continue; }

        let card = drm_path.join(&name);
        let vendor_path = card.join("device/vendor");
        let vendor_str = fs::read_to_string(&vendor_path).unwrap_or_default();
        if !vendor_str.trim().eq_ignore_ascii_case("0x1002") { continue; }  // AMD

        // Read VRAM
        let vram_path = card.join("device/mem_info_vram_total");
        let vram_total: u64 = fs::read_to_string(&vram_path)
            .ok().and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);

        let vram_used_path = card.join("device/mem_info_vram_used");
        let vram_used: u64 = fs::read_to_string(&vram_used_path)
            .ok().and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);

        let product_path = card.join("device/product_name");
        let gpu_name = fs::read_to_string(&product_path)
            .unwrap_or_else(|_| "AMD GPU".into())
            .trim().to_string();

        gpus.push(GpuInfo {
            name:          gpu_name,
            vendor:        GpuVendor::Amd,
            vram_total_mb: vram_total / 1024 / 1024,
            vram_free_mb:  (vram_total.saturating_sub(vram_used)) / 1024 / 1024,
            driver:        None,
            cuda_version:  None,
            rocm_version:  None,
            compute_cap:   None,
        });
    }
    gpus
}

// ── Intel Arc detection (Linux sysfs) ─────────────────────────────────────────

#[cfg(target_os = "linux")]
fn detect_intel_arc() -> Vec<GpuInfo> {
    use std::fs;
    let mut gpus = vec![];
    let drm_path = std::path::Path::new("/sys/class/drm");
    if !drm_path.exists() { return gpus; }

    for entry in std::fs::read_dir(drm_path).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("renderD") { continue; }
        let card = drm_path.join(&name);
        let vendor_path = card.join("device/vendor");
        let vendor_str = fs::read_to_string(&vendor_path).unwrap_or_default();
        if !vendor_str.trim().eq_ignore_ascii_case("0x8086") { continue; } // Intel
        // Only include if it's a discrete GPU (Arc) — check subsystem
        let product_path = card.join("device/product_name");
        let gpu_name = fs::read_to_string(&product_path)
            .unwrap_or_else(|_| "Intel GPU".into()).trim().to_string();
        if !gpu_name.to_lowercase().contains("arc") { continue; }

        let vram_path = card.join("device/mem_info_vram_total");
        let vram_mb: u64 = fs::read_to_string(&vram_path)
            .ok().and_then(|s| s.trim().parse().ok())
            .map(|b: u64| b / 1024 / 1024)
            .unwrap_or(0);

        gpus.push(GpuInfo {
            name:          gpu_name,
            vendor:        GpuVendor::Intel,
            vram_total_mb: vram_mb,
            vram_free_mb:  vram_mb,  // not easily readable
            driver:        None,
            cuda_version:  None,
            rocm_version:  None,
            compute_cap:   None,
        });
    }
    gpus
}

// ── Apple Silicon ─────────────────────────────────────────────────────────────

fn apple_silicon_gpu(memory: &MemoryInfo) -> GpuInfo {
    // On Apple Silicon, unified memory is shared between CPU and GPU.
    // All available RAM can be used for inference via Metal.
    let chip_name = detect_apple_chip_name();
    GpuInfo {
        name:          chip_name,
        vendor:        GpuVendor::Apple,
        vram_total_mb: memory.total_mb,      // entire unified memory pool
        vram_free_mb:  memory.available_mb,
        driver:        Some("Metal".into()),
        cuda_version:  None,
        rocm_version:  None,
        compute_cap:   None,
    }
}

fn detect_apple_chip_name() -> String {
    let output = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output();
    if let Ok(out) = output {
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !s.is_empty() { return s; }
    }
    // Try system_profiler
    let output = Command::new("system_profiler")
        .args(["SPHardwareDataType"])
        .output();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("Chip:") {
                if let Some(colon) = line.find(':') {
                    return line[colon + 1..].trim().to_string();
                }
            }
        }
    }
    "Apple Silicon".to_string()
}

// ── Recommendations ───────────────────────────────────────────────────────────

impl HardwareInfo {
    /// Maximum recommended model size tier for this hardware.
    pub fn max_model_tier(&self) -> ModelSizeTier {
        let vram = self.best_vram_mb;
        let ram  = self.memory.total_mb;

        // GPU VRAM takes priority
        if vram >= 80_000 { return ModelSizeTier::Massive; }
        if vram >= 40_000 { return ModelSizeTier::Huge; }
        if vram >= 20_000 { return ModelSizeTier::XLarge; }
        if vram >= 12_000 { return ModelSizeTier::Large; }
        if vram >= 6_000  { return ModelSizeTier::Medium; }
        if vram >= 3_000  { return ModelSizeTier::Small; }
        if vram >= 1_500  { return ModelSizeTier::Tiny; }

        // CPU-only RAM fallback
        if ram >= 128_000 { return ModelSizeTier::Huge; }
        if ram >= 48_000  { return ModelSizeTier::XLarge; }
        if ram >= 32_000  { return ModelSizeTier::Large; }
        if ram >= 20_000  { return ModelSizeTier::Medium; }
        if ram >= 12_000  { return ModelSizeTier::Small; }
        ModelSizeTier::Tiny
    }

    /// Best quantization to use given available VRAM/RAM for a given param count (billions).
    pub fn best_quantization(&self, params_b: f32) -> &'static str {
        let available_mb = if self.best_vram_mb > 2048 {
            self.best_vram_mb
        } else {
            self.memory.available_mb / 2  // leave half for system
        };

        // Approximate VRAM needed (MB) = params_billions * factor
        // F16: ~2000 MB/B, Q8: ~1050 MB/B, Q5: ~650 MB/B, Q4: ~525 MB/B, Q2: ~280 MB/B
        let f16_mb = (params_b * 2000.0) as u64;
        let q8_mb  = (params_b * 1050.0) as u64;
        let q5_mb  = (params_b * 650.0) as u64;
        let q4_mb  = (params_b * 525.0) as u64;

        if available_mb >= f16_mb { "F16 (best quality)" }
        else if available_mb >= q8_mb  { "Q8_0 (near-lossless)" }
        else if available_mb >= q5_mb  { "Q5_K_M (excellent)" }
        else if available_mb >= q4_mb  { "Q4_K_M (recommended)" }
        else                           { "Q2_K (minimum quality)" }
    }

    /// Human-readable summary of hardware for the status/welcome screen.
    pub fn summary(&self) -> String {
        let mut parts = vec![];

        // CPU
        parts.push(format!("CPU: {} ({} cores)", self.cpu.name, self.cpu.logical_cores));

        // RAM
        parts.push(format!(
            "RAM: {:.0} GB total, {:.0} GB free",
            self.memory.total_mb as f64 / 1024.0,
            self.memory.available_mb as f64 / 1024.0,
        ));

        // GPUs
        for gpu in &self.gpus {
            let vram_str = if gpu.vram_total_mb > 0 {
                format!(" ({:.0} GB VRAM)", gpu.vram_total_mb as f64 / 1024.0)
            } else {
                String::new()
            };
            parts.push(format!("GPU: {}{}", gpu.name, vram_str));
        }

        if self.gpus.is_empty() {
            parts.push("GPU: None detected (CPU-only inference)".into());
        }

        parts.join("\n")
    }

    /// One-line recommendation for welcome screen.
    pub fn recommendation_line(&self) -> String {
        let tier = self.max_model_tier();
        format!(
            "Your hardware can run {} parameter models ({}) — {}",
            tier.label(),
            if self.cpu_only { "CPU inference" } else { "GPU accelerated" },
            tier.description()
        )
    }
}
