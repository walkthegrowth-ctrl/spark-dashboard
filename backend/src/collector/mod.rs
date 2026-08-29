pub mod reader;
pub mod engine;
pub mod thermal;

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
