# Spec: Thermal Reporting Improvements (DGX Spark / GB10)

The DGX Spark (GB10) exposes two thermal layers: anonymous ACPI `acpitz` zones (board/package sensors — TI TMP461/TMP451 — plus on-die zones) and an NVIDIA GPU/SoC junction temperature reachable via `nvidia-smi`. The community `spark_hwmon` driver publishes eight named SPBM channels but is not available on all systems. This feature improves reporting using only what the reference board always provides.

## Requirements

1. **GPU channel** — Collector reads `nvidia-smi --query-gpu=temperature.gpu` and records it as a first-class sensor (`sensor_type: "nvidia"`, zone `gpu`). Skipped gracefully when nvidia-smi or a GPU is absent.

2. **Zone identification** — For anonymous `acpitz` zones, per sample:
   - the zone within 1 °C of the nvidia-smi reading is labeled `GPU die (SoC)`
   - the hottest zone is labeled `Board / package (est.)` (TMP461/TMP451 + Tj, runs 15–20 °C hotter under load)
   - remaining zones are labeled `SoC zone N (est.)`
   Labels are stored in the existing `sensor_label` column — no schema change.

3. **No duplicate acpitz rows** — The `acpitz` hwmon device mirrors the thermal zones from `/sys/class/thermal`; the hwmon reader skips it.

4. **UI grouping** — Thermal card shows **SoC (GB10)** (GPU die + acpitz zones, hottest first) and **Peripherals** (NVMe, WiFi PHY, hottest first) as separate groups. A hot NVMe must not be mistaken for a SoC issue.

5. **Thresholds**
   - Core: green < 80, yellow < 90, orange < 95, red ≥ 95 °C (GB10 throttles near 95 °C; community reports incidents at 94–97 °C)
   - Peripherals: keyed relative to the sensor's own `trip_point_temp` (NVMe crit 84.85 °C); orange within 10 °C of trip, red at/above trip

6. **Peak bar** — Still the max across ALL sensors (per product decision), but the label names the peaking sensor so the source is unambiguous.

## Out of scope

- Installing/reading `spark_hwmon` (kernel module not present on reference system)
- Fan/RPM telemetry, per-process memory, GPU memory temp (N/A on GB10)

## Verification

- `cargo test` — new tests: `read_gpu_sensor` (live on this box), `annotate_zones` (match/hottest/no-GPU/empty cases)
- `npm run build`
- Live check: `/api/thermal/current` must include a `nvidia` sensor plus annotated `acpitz` labels
