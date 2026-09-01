pub mod reader;
pub mod engine;
pub mod thermal;
pub mod compute;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub timestamp: i64,
    pub mem_total_bytes: Option<i64>,
    pub mem_free_bytes: Option<i64>,
    pub mem_available_bytes: Option<i64>,
    pub buffers_bytes: Option<i64>,
    pub cached_bytes: Option<i64>,
    pub swap_total_bytes: Option<i64>,
    pub swap_free_bytes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct MemoryHistoryResponse {
    pub count: i64,
    pub total: i64,
    pub data: Vec<MemoryStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalStat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub timestamp: i64,
    pub zone: String,
    pub sensor_type: String,
    pub temperature_celsius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trip_point_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trip_point_temp_celsius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sensor_label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ThermalHistoryResponse {
    pub count: i64,
    pub total: i64,
    pub data: Vec<ThermalStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreStat {
    pub index: i64,
    pub utilization_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeStat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_utilization_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_utilization_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_memory_utilization_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_1: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_5: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_15: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub core_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cores: Vec<CoreStat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ComputeHistoryResponse {
    pub count: i64,
    pub total: i64,
    pub data: Vec<ComputeStat>,
}

// Power
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerStat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub timestamp: i64,
    /// Source name, e.g. "nvidia-smi power.draw".
    pub source: String,
    /// Instantaneous power draw in watts.
    pub power_w: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct PowerHistoryResponse {
    pub count: i64,
    pub total: i64,
    pub data: Vec<PowerStat>,
}
