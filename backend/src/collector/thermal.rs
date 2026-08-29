use std::fs;
use std::path::Path;

use crate::collector::ThermalStat;

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
                let temp_celsius = temp_milli.trim().parse::<i64>().ok().map(|t| t as f64 / 1000.0);
                let zone_id = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let zone_num = zone_id.strip_prefix("thermal_zone").unwrap_or("").to_string();
                
                stats.push(ThermalStat {
                    id: None,
                    timestamp: chrono::Utc::now().timestamp(),
                    zone: zone_num,
                    sensor_type: zone_type.trim().to_string(),
                    temperature_celsius: temp_celsius,
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
                                let temp_celsius = temp_milli.trim().parse::<i64>().ok().map(|t| t as f64 / 1000.0);
                                let sensor_num = file_name.strip_prefix("temp").unwrap_or("").strip_suffix("_input").unwrap_or("");
                                
                                stats.push(ThermalStat {
                                    id: None,
                                    timestamp: chrono::Utc::now().timestamp(),
                                    zone: format!("{}_{}", sensor_name, sensor_num),
                                    sensor_type: "hwmon".to_string(),
                                    temperature_celsius: temp_celsius,
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
        }
    }

    #[test]
    fn test_read_hwmon_sensors() {
        let stats = read_hwmon_sensors();
        assert!(!stats.is_empty());
        for stat in stats {
            assert!(stat.sensor_type == "hwmon");
            assert!(stat.temperature_celsius.is_some());
        }
    }
}
