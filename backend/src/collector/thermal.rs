use std::fs;
use std::path::Path;
use std::process::Command;

use crate::collector::ThermalStat;

fn read_optional(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn parse_milli(temp_milli: &str) -> Option<f64> {
    temp_milli.trim().parse::<i64>().ok().map(|t| t as f64 / 1000.0)
}

fn parse_milli_threshold(temp_milli: &str) -> Option<f64> {
    parse_milli(temp_milli).and_then(|t| {
        if t < -50.0 || t > 250.0 {
            None
        } else {
            Some(t)
        }
    })
}

pub fn read_thermal_zones() -> Vec<ThermalStat> {
    let mut stats = Vec::new();
    let thermal_dir = Path::new("/sys/class/thermal");

    if !thermal_dir.exists() {
        return stats;
    }

    let entries = fs::read_dir(thermal_dir).ok();
    if let Some(entries) = entries {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.to_string_lossy().starts_with("/sys/class/thermal/thermal_zone") {
                continue;
            }

            let type_path = path.join("type");
            let temp_path = path.join("temp");

            if let (Ok(zone_type), Ok(temp_milli)) = (fs::read_to_string(&type_path), fs::read_to_string(&temp_path)) {
                let temp_celsius = parse_milli(&temp_milli);
                let zone_id = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let zone_num = zone_id.strip_prefix("thermal_zone").unwrap_or("").to_string();

                let trip_point_type = read_optional(&path.join("trip_point_0_type"));
                let trip_point_temp = read_optional(&path.join("trip_point_0_temp"))
                    .and_then(|t| parse_milli_threshold(&t));

                stats.push(ThermalStat {
                    id: None,
                    timestamp: chrono::Utc::now().timestamp(),
                    zone: zone_num.clone(),
                    sensor_type: zone_type.trim().to_string(),
                    temperature_celsius: temp_celsius,
                    trip_point_type,
                    trip_point_temp_celsius: trip_point_temp,
                    sensor_label: Some(format!("Zone {}", zone_num)),
                });
            }
        }
    }

    stats
}

pub fn read_hwmon_sensors() -> Vec<ThermalStat> {
    let mut stats = Vec::new();
    let hwmon_dir = Path::new("/sys/class/hwmon");

    if !hwmon_dir.exists() {
        return stats;
    }

    let entries = fs::read_dir(hwmon_dir).ok();
    if let Some(entries) = entries {
        for entry in entries.flatten() {
            let path = entry.path();
            let name_path = path.join("name");

            if let Ok(sensor_name) = fs::read_to_string(&name_path) {
                let sensor_name = sensor_name.trim().to_string();

                // ACPI thermal zones are already reported via /sys/class/thermal.
                if sensor_name == "acpitz" {
                    continue;
                }

                let temp_entries = fs::read_dir(&path).ok();
                if let Some(temp_entries) = temp_entries {
                    for temp_entry in temp_entries.flatten() {
                        let file_name = temp_entry.file_name().to_string_lossy().to_string();
                        if file_name.starts_with("temp") && file_name.ends_with("_input") {
                            if let Ok(temp_milli) = fs::read_to_string(temp_entry.path()) {
                                let temp_celsius = parse_milli(&temp_milli);
                                if temp_celsius.map_or(true, |t| t < 0.0 || t > 125.0) {
                                    continue;
                                }
                                let sensor_num = file_name.strip_prefix("temp").unwrap_or("").strip_suffix("_input").unwrap_or("");

                                let label_path = path.join(format!("temp{}_label", sensor_num));
                                let label = read_optional(&label_path);

                                let crit_path = path.join(format!("temp{}_crit", sensor_num));
                                let crit_temp = read_optional(&crit_path).and_then(|t| parse_milli_threshold(&t));

                                let max_path = path.join(format!("temp{}_max", sensor_num));
                                let max_temp = read_optional(&max_path).and_then(|t| parse_milli_threshold(&t));

                                let trip_type = if crit_temp.is_some() {
                                    Some("critical".to_string())
                                } else if max_temp.is_some() {
                                    Some("max".to_string())
                                } else {
                                    None
                                };

                                let trip_temp = crit_temp.or(max_temp);

                                stats.push(ThermalStat {
                                    id: None,
                                    timestamp: chrono::Utc::now().timestamp(),
                                    zone: format!("{}_{}", sensor_name, sensor_num),
                                    sensor_type: sensor_name.clone(),
                                    temperature_celsius: temp_celsius,
                                    trip_point_type: trip_type,
                                    trip_point_temp_celsius: trip_temp,
                                    sensor_label: label,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    stats
}

/// Read the GB10 package power draw in watts via nvidia-smi.
///
/// The DGX Spark is a single unified chip (GPU + ARM CPU + shared memory),
/// so `nvidia-smi power.draw` is the best available "whole device" power
/// figure the platform exposes. Returns None when nvidia-smi is missing or
/// the GPU is not reporting a sane value.
pub fn read_gpu_power() -> Option<f64> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=power.draw", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = stdout.lines().next()?.trim().parse::<f64>().ok()?;
    if !(0.0..=500.0).contains(&value) {
        return None;
    }
    Some(value)
}

pub fn read_gpu_sensor() -> Option<ThermalStat> {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=temperature.gpu", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value = stdout.lines().next()?.trim().parse::<f64>().ok()?;
    Some(ThermalStat {
        id: None,
        timestamp: chrono::Utc::now().timestamp(),
        zone: "gpu".to_string(),
        sensor_type: "nvidia".to_string(),
        temperature_celsius: Some(value),
        trip_point_type: None,
        trip_point_temp_celsius: None,
        sensor_label: Some("GPU".to_string()),
    })
}

/// Label anonymous `acpitz` zones, but only on first sight of each zone.
///
/// Identification uses the best information available on the reference board:
/// - the zone tracking the nvidia-smi GPU reading within 1 C is the on-die
///   GPU junction (`GPU die (SoC)`);
/// - the hottest zone is the board/package sensor (TMP461/TMP451 remote + Tj,
///   runs 15-20 C hotter under load) -> `Board / package`;
/// - the remainder -> `SoC zone N`.
///
/// The physical zone->sensor mapping is fixed for a running collector, but the
/// temperature-based identification is fragile (under load the hottest zone can
/// flip). So the label for a given zone is decided the FIRST time we see it and
/// then frozen into `known` (keyed "acpitz:<zone>") for the collector's
/// lifetime. Subsequent samples reuse the frozen label and never reassign, so
/// each sensor keeps one stable identity and one UI slot.
pub fn annotate_zones(stats: &mut [ThermalStat], gpu_temp: Option<f64>, known: &mut std::collections::HashMap<String, String>) {
    let acpitz: Vec<usize> = stats
        .iter()
        .enumerate()
        .filter(|(_, s)| s.sensor_type == "acpitz")
        .map(|(i, _)| i)
        .collect();

    if acpitz.is_empty() {
        return;
    }

    // Provisional identification for this sample (used only to seed labels we
    // haven't frozen yet).
    let mut gpu_zone: Option<usize> = None;
    if let Some(gpu) = gpu_temp {
        // 1. Prefer a zone tracking the nvidia-smi reading within 1 C (the
        //    on-die junction sensor).
        // 2. Fallback: the single closest zone within 3 C, so an idle-state
        //    rounding gap (e.g. GPU 64.0 C vs zone 65.3 C) still yields a
        //    "GPU die" identification instead of leaving the slot empty.
        let mut closest: Option<(usize, f64)> = None;
        for &i in &acpitz {
            let t = stats[i].temperature_celsius;
            if let Some(t) = t {
                let diff = (t - gpu).abs();
                if diff <= 1.0 {
                    gpu_zone = Some(i);
                    break;
                }
                if diff <= 3.0 && closest.map_or(true, |(_, d)| diff < d) {
                    closest = Some((i, diff));
                }
            }
        }
        if gpu_zone.is_none() {
            gpu_zone = closest.map(|(i, _)| i);
        }
    }

    // Hottest zone = board/package sensor; exclude the GPU-die zone so both
    // slots always get filled.
    let mut hot_zone: Option<usize> = None;
    for &i in &acpitz {
        if Some(i) == gpu_zone {
            continue;
        }
        if hot_zone.map_or(true, |h| {
            stats[i].temperature_celsius.unwrap_or(0.0) > stats[h].temperature_celsius.unwrap_or(0.0)
        }) {
            hot_zone = Some(i);
        }
    }
    let hot_zone = hot_zone.unwrap_or(acpitz[0]);

    for &i in &acpitz {
        let zone_num = stats[i].zone.clone();
        let key = format!("acpitz:{}", zone_num);
        let label = if let Some(existing) = known.get(&key).cloned() {
            existing // frozen identity: do not re-identify
        } else if Some(i) == gpu_zone {
            "GPU die (SoC)".to_string()
        } else if i == hot_zone {
            "Board / package".to_string()
        } else {
            format!("SoC zone {}", zone_num)
        };
        known.insert(key, label.clone());
        stats[i].sensor_label = Some(label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_thermal_zones() {
        let stats = read_thermal_zones();
        assert!(!stats.is_empty());
        for stat in stats {
            assert!(stat.sensor_type == "acpitz" || stat.sensor_type == "x86_pkg_temp");
            assert!(stat.temperature_celsius.is_some());
            assert!(stat.trip_point_type.is_some());
            assert!(stat.sensor_label.is_some());
        }
    }

    #[test]
    fn test_read_hwmon_sensors() {
        let stats = read_hwmon_sensors();
        assert!(!stats.is_empty());
        let nvme = stats.iter().find(|s| s.sensor_type == "nvme");
        assert!(nvme.is_some());
        let nvme = nvme.unwrap();
        assert!(nvme.sensor_label.is_some());
    }

    #[test]
    fn test_parse_milli_valid() {
        assert_eq!(parse_milli("65400"), Some(65.4));
        assert_eq!(parse_milli("0"), Some(0.0));
    }

    #[test]
    fn test_parse_milli_invalid() {
        assert_eq!(parse_milli("abc"), None);
        assert_eq!(parse_milli(""), None);
    }

    #[test]
    fn test_parse_milli_threshold_valid() {
        assert_eq!(parse_milli_threshold("104800"), Some(104.8));
        assert_eq!(parse_milli_threshold("0"), Some(0.0));
    }

    #[test]
    fn test_parse_milli_threshold_bogus() {
        assert_eq!(parse_milli_threshold("65261850"), None);
        assert_eq!(parse_milli_threshold("-100000"), None);
    }

    fn zone(id: &str, temp: f64) -> ThermalStat {
        ThermalStat {
            id: None,
            timestamp: 0,
            zone: id.to_string(),
            sensor_type: "acpitz".to_string(),
            temperature_celsius: Some(temp),
            trip_point_type: Some("critical".to_string()),
            trip_point_temp_celsius: Some(104.8),
            sensor_label: Some(format!("Zone {}", id)),
        }
    }

    #[test]
    fn test_annotate_zones_with_gpu_match() {
        // Zone 3 tracks the GPU reading; zone 1 runs hotter (board/package).
        let mut stats = vec![zone("0", 62.0), zone("1", 97.0), zone("2", 63.0), zone("3", 94.5)];
        let mut known = std::collections::HashMap::new();
        annotate_zones(&mut stats, Some(94.5), &mut known);
        assert_eq!(stats[3].sensor_label.as_deref(), Some("GPU die (SoC)"));
        assert_eq!(stats[1].sensor_label.as_deref(), Some("Board / package"));
        assert_eq!(stats[0].sensor_label.as_deref(), Some("SoC zone 0"));
        assert_eq!(stats[2].sensor_label.as_deref(), Some("SoC zone 2"));
    }

    #[test]
    fn test_annotate_zones_gpu_is_hottest() {
        // GPU dies at 96 == the GPU reading. With the GPU zone excluded from
        // the "hottest/board" selection, the next-hottest zone (z2=61) becomes
        // Board / package, leaving the Board slot filled.
        let mut stats = vec![zone("0", 60.0), zone("1", 96.0), zone("2", 61.0)];
        let mut known = std::collections::HashMap::new();
        annotate_zones(&mut stats, Some(96.0), &mut known);
        assert_eq!(stats[1].sensor_label.as_deref(), Some("GPU die (SoC)"));
        assert_eq!(stats[0].sensor_label.as_deref(), Some("SoC zone 0"));
        assert_eq!(stats[2].sensor_label.as_deref(), Some("Board / package"));
    }

    #[test]
    fn test_annotate_zones_without_gpu() {
        let mut stats = vec![zone("0", 58.0), zone("1", 74.0)];
        let mut known = std::collections::HashMap::new();
        annotate_zones(&mut stats, None, &mut known);
        assert_eq!(stats[1].sensor_label.as_deref(), Some("Board / package"));
        assert_eq!(stats[0].sensor_label.as_deref(), Some("SoC zone 0"));
    }

    #[test]
    fn test_annotate_zones_freezes_label() {
        // Zone 1 is hottest on first sight -> "Board / package".
        let mut stats = vec![zone("0", 58.0), zone("1", 74.0)];
        let mut known = std::collections::HashMap::new();
        annotate_zones(&mut stats, None, &mut known);
        assert_eq!(stats[1].sensor_label.as_deref(), Some("Board / package"));

        // Next sample: temperatures shift so the *heuristic* would now call
        // zone 0 the board (it's hotter and zone 1 is no longer hottest). The
        // frozen identity must win, so both zones keep their original slot.
        let mut next = vec![zone("0", 90.0), zone("1", 60.0)];
        annotate_zones(&mut next, None, &mut known);
        assert_eq!(stats[0].sensor_label.as_deref(), Some("SoC zone 0"));
        assert_eq!(next[1].sensor_label.as_deref(), Some("Board / package"));
        // zone 0 was "SoC zone 0" before and stays "SoC zone 0" (not re-id'd as board).
        assert_eq!(next[0].sensor_label.as_deref(), Some("SoC zone 0"));
    }

    #[test]
    fn test_annotate_zones_gpu_closest_zone_fallback() {
        // GPU reads 64.0 C; no zone within 1 C, but zone 1 at 65.3 C is the
        // closest within 3 C -> it is identified as the GPU die instead of
        // leaving the slot empty.
        let mut stats = vec![zone("0", 68.9), zone("1", 65.3), zone("2", 62.0)];
        let mut known = std::collections::HashMap::new();
        annotate_zones(&mut stats, Some(64.0), &mut known);
        let labels = stats.iter().map(|s| s.sensor_label.clone().unwrap()).collect::<Vec<_>>();
        assert!(labels.contains(&"GPU die (SoC)".to_string()), "got: {:?}", labels);
        assert_eq!(stats[1].sensor_label.as_deref(), Some("GPU die (SoC)"));
    }

    #[test]
    fn test_annotate_zones_gpu_gap_too_large_no_assignment() {
        // GPU 60 C, all zones 10 C away -> no GPU-die identification, only
        // board + SoC labels.
        let mut stats = vec![zone("0", 70.0), zone("1", 72.0)];
        let mut known = std::collections::HashMap::new();
        annotate_zones(&mut stats, Some(60.0), &mut known);
        let labels = stats.iter().map(|s| s.sensor_label.clone().unwrap()).collect::<Vec<_>>();
        assert!(!labels.contains(&"GPU die (SoC)".to_string()));
        assert!(labels.contains(&"Board / package".to_string()));
    }

    #[test]
    fn test_annotate_zones_empty() {
        let mut stats = vec![ThermalStat {
            id: None,
            timestamp: 0,
            zone: "nvme_1".to_string(),
            sensor_type: "nvme".to_string(),
            temperature_celsius: Some(45.0),
            trip_point_type: None,
            trip_point_temp_celsius: None,
            sensor_label: Some("Composite".to_string()),
        }];
        let mut known = std::collections::HashMap::new();
        annotate_zones(&mut stats, Some(75.0), &mut known);
        assert_eq!(stats[0].sensor_label.as_deref(), Some("Composite"));
    }

    #[test]
    fn test_read_gpu_sensor() {
        if Command::new("nvidia-smi").arg("--version").output().is_err() {
            return; // nvidia-smi not present on build host
        }
        let gpu = read_gpu_sensor();
        assert!(gpu.is_some(), "nvidia-smi present but no GPU sensor returned");
        let gpu = gpu.unwrap();
        assert_eq!(gpu.sensor_type, "nvidia");
        assert!(gpu.temperature_celsius.unwrap_or(0.0) > 0.0);
        assert_eq!(gpu.sensor_label.as_deref(), Some("GPU"));
    }
}
