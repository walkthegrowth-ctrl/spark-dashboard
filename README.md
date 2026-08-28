# Spark Dashboard

Lightweight monitoring dashboard for the NVIDIA DGX Spark (GB10) and compatible hardware.

Designed as a more useful alternative to the official NVIDIA DGX Dashboard, with a strong focus on thermal visibility and history, accurate unified-memory reporting, and first-class support for common LLM serving engines.

## Architecture

Two runtime components distributed as a single binary:

- **`spark-collect`** — Collector daemon, reads hardware metrics, writes to SQLite
- **`spark-serve`** — API server, REST endpoints, serves static web assets

## Features

| Feature | Status |
|---|---|
| Memory usage reporting | Implemented |
| Temperature monitoring | Planned |
| Machine shutdown/restart | Planned |
| Thermal history | Planned |
| LLM engine detection | Planned |
| Tailscale status | Planned |

## Quick Start

```bash
# Build
cd backend
cargo build --release

# Run collector
SPARK_CONFIG_PATH=config/default.toml ./target/release/spark-collect

# Run server
SPARK_CONFIG_PATH=config/default.toml ./target/release/spark-serve
```

## API

| Endpoint | Description |
|---|---|
| `GET /api/memory/current` | Latest memory reading |
| `GET /api/memory/history?limit=100&offset=0` | Historical memory readings |

## Configuration

See `backend/config/default.toml` for reference configuration.

Override with `SPARK_CONFIG_PATH` environment variable.

## Hardware Target

- NVIDIA DGX Spark (GB10)
- Unified coherent memory architecture
- aarch64

## License

MIT
