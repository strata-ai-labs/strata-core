//! Shared benchmark helpers used by the next-crate benchmark binaries.

pub mod metrics;
pub mod recorder;

use std::time::Duration;

static HARDWARE_INFO_ONCE: std::sync::Once = std::sync::Once::new();

/// Print hardware specs once per benchmark binary.
pub fn print_hardware_info() {
    HARDWARE_INFO_ONCE.call_once(|| {
        let cpu = read_cpu_model();
        let cores = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(0);
        let ram_gb = read_total_ram_gb();
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;

        eprintln!("=== Hardware ===");
        eprintln!("CPU:    {cpu}");
        eprintln!("Cores:  {cores}");
        eprintln!("RAM:    {ram_gb} GB");
        eprintln!("OS:     {os} ({arch})");
        eprintln!("================");
    });
}

pub fn read_cpu_model() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in contents.lines() {
                if line.starts_with("model name") {
                    if let Some(val) = line.split(':').nth(1) {
                        return val.trim().to_string();
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("machdep.cpu.brand_string")
            .output()
        {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }
    }
    "unknown".to_string()
}

pub fn read_total_ram_gb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
            for line in contents.lines() {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb / 1_048_576;
                        }
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output()
        {
            if output.status.success() {
                if let Ok(bytes) = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u64>()
                {
                    return bytes / (1024 * 1024 * 1024);
                }
            }
        }
    }
    0
}

/// Collected latency percentiles.
pub struct Percentiles {
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub min: Duration,
    pub max: Duration,
    pub samples: usize,
}

/// Format a duration for human-readable latency display.
fn fmt_duration(d: Duration) -> String {
    let nanos = d.as_nanos();
    if nanos < 1_000 {
        format!("{nanos} ns")
    } else if nanos < 1_000_000 {
        format!("{:.2} us", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
    }
}

/// Print percentiles to stderr in a compact table.
pub fn report_percentiles(label: &str, p: &Percentiles) {
    eprintln!(
        "  {:<45} p50={:<12} p95={:<12} p99={:<12} (n={})",
        label,
        fmt_duration(p.p50),
        fmt_duration(p.p95),
        fmt_duration(p.p99),
        p.samples,
    );
}
