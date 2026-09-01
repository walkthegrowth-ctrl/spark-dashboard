# Spec: Compute Load Reporting (CPU + GPU)

The DGX Spark dashboard showed memory and thermal health but not *load* — how busy the CPU and GPU actually are. This feature adds a third live stream.

## Requirements

1. **CPU total utilization** — percentage of CPU time not idle, computed as the tick delta between consecutive `/proc/stat` reads (the first sample after collector start has no CPU reading).
2. **Per-core utilization** — same delta math for each of the 20 cores, so hotspot cores are visible.
3. **Load averages** — 1/5/15 min from `/proc/loadavg` (context for how load maps to core count).
4. **GPU utilization** — `nvidia-smi --query-gpu=utilization.gpu` (core utilization, 0–100). GPU memory utilization is read when available; GB10 reports `N/A`, so it is stored as `Option`.
5. **Cadence** — separate `[compute] collection_interval_secs` config section (default 10 s) because utilization needs a higher sampling rate than memory/thermal for meaningful deltas. The previous config parser could not distinguish repeated `collection_interval_secs` keys, so it was rewritten to be section-aware.
6. **Persistence** — new `compute_stats` table; per-core list stored as JSON in a `cores` TEXT column (variable core count, avoids a joins table).
7. **API** — `GET /api/compute/current` and `GET /api/compute/history?limit=&offset=`, mirroring the memory/thermal handlers (200 with data, 503 empty, 400 bad params).
8. **UI** — a "Compute Load" card with:
   - CPU % (total), GPU % (with GPU model name), load averages (1/5/15m) tiles
   - Two utilization bars (CPU average across cores, GPU)
   - Per-core grid, sorted by core index (stable layout — consistent with the thermal card requirement)
   - Color scale: green <50, yellow <80, orange <95, red ≥95 %
9. **Graceful degradation** — all nvidia-smi-dependent fields are `Option`; on a system without a GPU the card still shows CPU.

## Verification

- `cargo test` — new suites in `collector::compute` (parse, utilization math, loadavg, nvidia-smi shape, live smoke), `db` (round-trip incl. core JSON), `server::handler` (current/history/empty/bad-param)
- `npm run build`
- Playwright: card renders CPU % + all per-core bars, no console errors
- Live: after restart `/api/compute/current` returns `cpu_utilization_pct`, per-core `cores[]`, `gpu_utilization_pct`, `load_1/5/15`, `gpu_name: "NVIDIA GB10"`

## Out of scope

- GPU memory bandwidth/throughput (N/A on GB10)
- Per-process CPU attribution
- History charts (future)
