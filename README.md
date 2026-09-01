# Spark Dashboard

Lightweight monitoring dashboard for the NVIDIA DGX Spark (GB10) and compatible hardware.

Designed as a more useful alternative to the official NVIDIA DGX Dashboard, with a strong focus on thermal visibility and history, accurate unified-memory reporting, and first-class support for common LLM serving engines.

## Architecture

One Rust crate, two binaries:

- **`spark-collect`** — Collector daemon, reads hardware metrics, writes to SQLite
- **`spark-serve`** — API server, REST endpoints, serves static web assets (Svelte frontend)

Two separate planes:

- **Data plane** — the collector samples hardware on a timer and persists rows to SQLite (WAL mode); the server reads them. The server never writes to the database.
- **Control plane (IPC)** — the collector listens on a Unix domain socket (`[collector] ipc_socket`). The server sends the collector commands over it: "take a fresh sample now" (used to back `GET /api/<stream>/current`) and "apply config changes" (intervals, retention). The response is a small confirmation (ok/skipped + timestamp), never data — data still comes from the DB.

If the collector is down, `GET /api/<stream>/current` transparently falls back to the latest DB rows, and `POST /api/config` returns `503`.

## Features

| Feature | Status |
|---|---|
| Memory usage reporting | Implemented |
| Temperature monitoring (SoC + peripherals, trip points) | Implemented |
| Machine shutdown/restart | Planned |
| CPU/GPU utilization & compute history | Implemented |
| Thermal history | Implemented |
| LLM engine detection | Planned |
| Tailscale status | Planned |

## Quick Start

Easiest: `./run.sh` — builds backend + frontend, starts both processes on port 8090 (options: `--port`, `--config`). Ctrl-C shuts both down.

Manually:

```bash
# Build both
cd backend && cargo build --release && cd ..
cd frontend && npm install && npm run build && cd ..

# Run collector (config path is relative to CWD)
cd backend
SPARK_CONFIG_PATH=config/default.toml ./target/release/spark-collect &

# Run server
SPARK_CONFIG_PATH=config/default.toml ./target/release/spark-serve --port 8090
```

## API

Read endpoints:

| Endpoint | Description |
|---|---|
| `GET /api/memory/current` | Latest memory reading (triggers a fresh collector sample first) |
| `GET /api/memory/history?limit=100&offset=0` | Historical memory readings (pure DB read) |
| `GET /api/thermal/current` | Latest thermal zone readings (SoC + peripherals) |
| `GET /api/thermal/history?limit=100&offset=0` | Historical thermal readings |
| `GET /api/compute/current` | CPU/GPU utilization, per-core load, load averages |
| `GET /api/compute/history?limit=100&offset=0` | Historical compute readings |
| `GET /api/power/current` | Current power draw (W) from `nvidia-smi power.draw` (unified GB10 package) |
| `GET /api/power/history?limit=100&offset=0` | Historical power readings |

Action endpoint (`POST`, JSON body):

| Endpoint | Description |
|---|---|
| `POST /api/config` | Apply runtime config changes to the collector |

`POST /api/config` body — any subset of these fields; omitted fields are unchanged:

```json
{
  "retention_days": 30,
  "memory_interval_secs": 60,
  "thermal_interval_secs": 60,
  "compute_interval_secs": 60,
  "power_interval_secs": 60
}
```

Returns `200 {"ok":true}` on success, `400` with an error message for invalid/unknown values, and `503` if the collector is unreachable.

## Configuration

See `backend/config/default.toml` for reference configuration.

Override with `SPARK_CONFIG_PATH` environment variable.

## Hardware Target

- NVIDIA DGX Spark (GB10)
- Unified coherent memory architecture
- aarch64

## License

MIT
