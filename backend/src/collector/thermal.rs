use std::fs;
use std::path::Path;

use crate::collector::ThermalStat;

fn read_optional(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn parse_milli(temp_milli: &str) -> Option<f64> {
    temp_milli.trim().parse::<i64>().ok().map(|t| t as f64 / 1000.0)
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
                    .and_then(|t| parse_milli(&t));

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

                let temp_entries = fs::read_dir(&path).ok();
                if let Some(temp_entries) = temp_entries {
                    for temp_entry in temp_entries.flatten() {
                        let file_name = temp_entry.file_name().to_string_lossy().to_string();
                        if file_name.starts_with("temp") && file_name.ends_with("_input") {
                            if let Ok(temp_milli) = fs::read_to_string(temp_entry.path()) {
                                let temp_celsius = parse_milli(&temp_milli);
                                let sensor_num = file_name.strip_prefix("temp").unwrap_or("").strip_suffix("_input").unwrap_or("");

                                let label_path = path.join(format!("temp{}_label", sensor_num));
                                let label = read_optional(&label_path);

                                let crit_path = path.join(format!("temp{}_crit", sensor_num));
                                let crit_temp = read_optional(&crit_path).and_then(|t| parse_milli(&t));

                                let max_path = path.join(format!("temp{}_max", sensor_num));
                                let max_temp = read_optional(&max_path).and_then(|t| parse_milli(&t));

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
}
