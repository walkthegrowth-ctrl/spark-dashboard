use std::fs;
use std::process::Command;

use crate::collector::{ComputeStat, CoreStat};

/// Raw tick counters from /proc/stat: one aggregate row plus one row per core.
/// Fields follow kernel order: user nice system idle iowait irq softirq steal.
#[derive(Debug, Clone)]
pub struct CpuRaw {
    pub total: Vec<u64>,
    pub cores: Vec<Vec<u64>>,
}

fn parse_row(fields: &[u64]) -> Option<Vec<u64>> {
    if fields.len() < 8 {
        return None;
    }
    Some(fields[..8].to_vec())
}

pub fn parse_proc_stat(content: &str) -> Option<CpuRaw> {
    let mut total: Option<Vec<u64>> = None;
    let mut cores: Vec<Vec<u64>> = Vec::new();
    for line in content.lines() {
        let fields: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|f| f.parse().ok())
            .collect();
        let Some(row) = parse_row(&fields) else {
            continue;
        };
        if line.starts_with("cpu ") {
            total = Some(row);
        } else if line[3..].as_bytes().first().is_some_and(|b| b.is_ascii_digit()) {
            cores.push(row);
        }
    }
    total.map(|t| CpuRaw { total: t, cores })
}

fn sum_ticks(t: &[u64]) -> i128 {
    t.iter().map(|v| *v as i128).sum::<i128>()
}

/// idle-ish = idle + iowait (fields 3 and 4).
fn idle_ticks(t: &[u64]) -> i128 {
    (t[3] + t[4]) as i128
}

/// CPU utilization over the interval between two /proc/stat snapshots.
pub fn utilization(prev: &[u64], cur: &[u64]) -> Option<f64> {
    let dt = sum_ticks(cur) - sum_ticks(prev);
    let di = idle_ticks(cur) - idle_ticks(prev);
    if dt <= 0 {
        return None;
    }
    let busy = (dt - di).max(0);
    Some(((busy as f64 / dt as f64) * 100.0).clamp(0.0, 100.0))
}

pub fn parse_loadavg(line: &str) -> Option<(f64, f64, f64)> {
    let parts: Vec<f64> = line
        .split_whitespace()
        .take(3)
        .filter_map(|v| v.parse().ok())
        .collect();
    if parts.len() == 3 {
        Some((parts[0], parts[1], parts[2]))
    } else {
        None
    }
}

pub fn read_loadavg() -> Option<(f64, f64, f64)> {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| parse_loadavg(s.trim()))
}

pub fn count_cores() -> Option<i64> {
    std::thread::available_parallelism().ok().map(|p| p.get() as i64)
}

/// GPU name, utilization %, and memory utilization % from nvidia-smi, or Nones.
pub fn read_nvidia_gpu() -> (Option<f64>, Option<f64>, Option<String>) {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,utilization.gpu,utilization.memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok();
    let Some(output) = output else { return (None, None, None) };
    if !output.status.success() {
        return (None, None, None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts = match stdout.lines().next() {
        Some(l) => l.split(',').map(|p| p.trim().to_string()).collect::<Vec<_>>(),
        None => return (None, None, None),
    };
    if parts.len() < 3 {
        return (None, None, None);
    }
    let gpu = parts[1].parse().ok();
    let mem = parts[2].parse().ok();
    (gpu, mem, Some(parts[0].clone()))
}

/// Read one compute sample. `prev` is threaded between calls so CPU utilization
/// is a tick delta; the first sample after startup has no CPU/GPU-core reading.
pub fn read_compute(prev: &mut Option<CpuRaw>) -> ComputeStat {
    let raw = fs::read_to_string("/proc/stat").ok().and_then(|s| parse_proc_stat(&s));

    let mut cpu_util: Option<f64> = None;
    let mut cores: Vec<CoreStat> = Vec::new();

    if let (Some(p), Some(cur)) = (prev.as_ref(), raw.as_ref()) {
        cpu_util = utilization(&p.total, &cur.total);
        for (i, core) in cur.cores.iter().enumerate() {
            if let Some(prev_core) = p.cores.get(i) {
                if let Some(u) = utilization(prev_core, core) {
                    cores.push(CoreStat {
                        index: i as i64,
                        utilization_pct: u,
                    });
                }
            }
        }
    }
    *prev = raw;

    let (load_1, load_5, load_15) = read_loadavg().unwrap_or((0.0, 0.0, 0.0));
    let (gpu, gpu_mem, gpu_name) = read_nvidia_gpu();

    ComputeStat {
        id: None,
        timestamp: chrono::Utc::now().timestamp(),
        cpu_utilization_pct: cpu_util,
        gpu_utilization_pct: gpu,
        gpu_memory_utilization_pct: gpu_mem,
        load_1: Some(load_1),
        load_5: Some(load_5),
        load_15: Some(load_15),
        core_count: if cores.is_empty() { count_cores() } else { Some(cores.len() as i64) },
        cores,
        gpu_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Total: a=[user 10] sum=10, b=[user 70, idle 40] sum=110 -> d_total=100, d_idle=40, busy=60 -> 60%
    // core0: a=[user 10], b=[user 40] -> 100%; core1: a=[user 10], b=[user 10, idle 50] -> 0%
    const STAT_A: &str = "cpu  10 0 0 0 0 0 0 0\n\
        cpu0 10 0 0 0 0 0 0 0\n\
        cpu1 10 0 0 0 0 0 0 0\n";

    const STAT_B: &str = "cpu  70 0 0 40 0 0 0 0\n\
        cpu0 40 0 0 0 0 0 0 0\n\
        cpu1 10 0 0 50 0 0 0 0\n";

    #[test]
    fn test_parse_proc_stat() {
        let raw = parse_proc_stat(STAT_A).unwrap();
        assert_eq!(raw.total.len(), 8);
        assert_eq!(raw.cores.len(), 2);
        assert_eq!(raw.cores[0].len(), 8);
    }

    #[test]
    fn test_parse_proc_stat_empty() {
        assert!(parse_proc_stat("").is_none());
    }

    #[test]
    fn test_utilization() {
        let a = parse_proc_stat(STAT_A).unwrap();
        let b = parse_proc_stat(STAT_B).unwrap();
        // agg: d_total = 250, d_idle = 0, busy = 150 -> 60%
        let u = utilization(&a.total, &b.total).unwrap();
        assert!((u - 60.0).abs() < 1e-6, "got {u}");
    }

    #[test]
    fn test_utilization_no_progress() {
        let a = parse_proc_stat(STAT_A).unwrap();
        assert!(utilization(&a.total, &a.total).is_none());
    }

    #[test]
    fn test_per_core_utilization() {
        let a = parse_proc_stat(STAT_A).unwrap();
        let b = parse_proc_stat(STAT_B).unwrap();
        // core0: d_busy = 25, d_total = 25 + 0 = 25 -> 100%
        let c0 = utilization(&a.cores[0], &b.cores[0]).unwrap();
        assert!((c0 - 100.0).abs() < 1e-6, "got {c0}");
        // core1: all idle -> 0%
        let c1 = utilization(&a.cores[1], &b.cores[1]).unwrap();
        assert_eq!(c1, 0.0);
    }

    #[test]
    fn test_utilization_idle_only() {
        // only idle ticks increase -> 0%
        let u = utilization(&[0, 0, 0, 0, 0, 0, 0, 0], &[0, 0, 0, 1000, 0, 0, 0, 0]).unwrap();
        assert_eq!(u, 0.0);
    }

    #[test]
    fn test_utilization_busy_only() {
        // only busy ticks increase -> 100%
        let u = utilization(&[0, 0, 0, 0, 0, 0, 0, 0], &[100, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(u, 100.0);
    }

    #[test]
    fn test_parse_loadavg() {
        let (a, b, c) = parse_loadavg("0.61 0.70 0.63 2/1017 174080").unwrap();
        assert!((a - 0.61).abs() < 1e-9);
        assert!((b - 0.70).abs() < 1e-9);
        assert!((c - 0.63).abs() < 1e-9);
    }

    #[test]
    fn test_parse_loadavg_invalid() {
        assert!(parse_loadavg("abc def").is_none());
        assert!(parse_loadavg("").is_none());
    }

    #[test]
    fn test_count_cores() {
        assert!(count_cores().unwrap() >= 1);
    }

    #[test]
    fn test_read_nvidia_gpu_shape() {
        let (gpu, _mem, name) = read_nvidia_gpu();
        if name.is_none() {
            return; // nvidia-smi absent
        }
        assert!(gpu.map_or(true, |g| (0.0..=100.0).contains(&g)));
        assert!(name.unwrap().starts_with("NVIDIA"));
    }

    #[test]
    fn test_read_compute_smoke() {
        let mut prev = None;
        let _ = read_compute(&mut prev);
        let s = read_compute(&mut prev);
        assert!(s.cpu_utilization_pct.is_some());
        assert!(!s.cores.is_empty());
    }
}