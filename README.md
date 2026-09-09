# Spark Dashboard

A lightweight, local monitoring panel for the NVIDIA DGX Spark (GB10). MVP — it **complements** the official NVIDIA DGX Dashboard rather than replacing it: clearer thermal and power visibility, memory and compute history, in a small self-hosted stack that runs entirely on the box.

Built by **Embracing Challenges** ([@walkthegrowth](https://x.com/walkthegrowth)).

---

## What it does

- **Memory** — used / free / available (MemAvailable-aware), swap, and history
- **Thermal** — SoC + peripheral zones with trip points, and history
- **Compute** — global CPU utilization, per-core loads, load averages, GPU utilization (via `nvidia-smi`), and history
- **Power** — GPU package power draw (`nvidia-smi power.draw`), and history
- **History charts** — each stream keeps an inline time-dynamics chart (fixed scales, `Max:` + recording-time footer)

## What it doesn't do (limits)

- **Read-only** — it reports; it does not control (no shutdown/restart, no kill, no service management)
- **Local only** — the server binds `127.0.0.1` by default and has **no authentication**; it is not designed for untrusted networks
- **Platform** — aarch64 Linux (DGX Spark / GB10)
- **GPU data** needs `nvidia-smi` on PATH; without it, GPU-side metrics read as unavailable
- **Finite history** — rows older than `retention_days` (default 7) are pruned
- Not a substitute for the official NVIDIA dashboard, which still covers capabilities this MVP hasn't

## Planned (not yet implemented)

- Machine shutdown/restart controls
- LLM engine / serving-stack detection
- Tailscale status
- Per-SM GPU detail, alerts & thresholds with notifications

## Requirements

- aarch64 Linux with the GB10 toolchain (`nvidia-smi` available)
- `unzip` and `sha256sum` (standard on DGX OS)

## Install

```bash
curl -fsSL https://github.com/walkthegrowth-ctrl/spark-dashboard/releases/latest/download/install.sh | sh
```

Installs to `~/.spark` (default). The installer:

1. checks the platform (aarch64-Linux) and required tools
2. downloads the latest release bundle (live progress meter)
3. verifies the sha256 (refuses on mismatch)
4. extracts to `~/.spark`

It does **not** start anything and does **not** touch your history database.

### Installer options

| Option | Description |
|---|---|
| `--dest DIR` | Install directory (default `~/.spark`) |
| `--version TAG` | Pin a specific release (e.g. `v1.0.0`) instead of latest |
| `--port PORT` | Remember a preferred port for `spark.sh start` |
| `--reinstall` | Replace an existing install in `--dest` |
| `--uninstall` | Remove the installation |
| `--purge-db` | Along with `--uninstall`: also delete the history DB |

## Start / stop / status

Once installed, the dashboard runs in the background — you control it with `spark.sh` in the install directory:

```bash
# start (defaults to :8090; port priority: argument → last used → installer hint → config)
~/.spark/spark.sh start              # or: ~/.spark/spark.sh start 11020
# open http://localhost:8090 in a browser

# check it's up (HTTP liveness + pids)
~/.spark/spark.sh status

# view recent logs from both processes
~/.spark/spark.sh logs 40

# stop cleanly (SIGTERM, then SIGKILL if it doesn't exit in ~5 s)
~/.spark/spark.sh stop
```

Runtime state (pid files, last-used port, log files) lives in `~/.spark/.spark/` (yes, that's nested — the outer is the install dir). Your history database and any config you edit survive restarts and even reinstalls.

## Uninstall

Stops the daemons and removes the install. The history database at
`~/.local/share/spark-dashboard` is **kept** by default — pass `--purge-db`
to delete it too.

```bash
# 1) Stop the daemons (if running)
~/.spark/spark.sh stop

# 2) Remove the install
#    keeps history DB:
curl -fsSL https://github.com/walkthegrowth-ctrl/spark-dashboard/releases/latest/download/install.sh | sh -s -- --uninstall
#    removes history DB too:
curl -fsSL https://github.com/walkthegrowth-ctrl/spark-dashboard/releases/latest/download/install.sh | sh -s -- --uninstall --purge-db
```

By hand: `~/.spark/spark.sh stop`, then `rm -rf ~/.spark`
(and `rm -rf ~/.local/share/spark-dashboard` to also drop the history DB).

If you installed to a non-default `--dest`, prepend it:
`… install.sh | sh -s -- --dest /opt/spark --uninstall`.

## Configuration

Reference config: `backend/config/default.toml` (bundled as `default.toml` beside the binaries). You can edit it directly; the running daemons will pick up most keys on restart.

| Key | Default | Description |
|---|---|---|
| `server.host` / `server.port` | `127.0.0.1` / `8090` | Bind address and port |
| `collector.ipc_socket` | `/tmp/spark-collect.sock` | Control-plane socket |
| `database.path` | `~/.local/share/spark-dashboard/spark.db` | History DB location |
| `history.retention_days` | `7` | Rows older than this are pruned (at most once per 6 h) |
| `memory.collection_interval_secs` | `5` | Sampling interval |
| `thermal.collection_interval_secs` | `5` | Sampling interval |
| `compute.collection_interval_secs` | `5` | Sampling interval |
| `power.collection_interval_secs` | `5` | Sampling interval |

Live reconfigure (no restart) via the API:

```bash
curl -s -X POST http://localhost:8090/api/config \
  -H 'Content-Type: application/json' \
  -d '{"memory_interval_secs": 60, "retention_days": 30}'
```

Environment override: `SPARK_CONFIG_PATH=/path/to/custom.toml` — both binaries honor it; `spark-serve --config PATH` also works and wins over the env var.

## API

Read endpoints (per stream — `memory`, `thermal`, `compute`, `power`):

| Endpoint | Description |
|---|---|
| `GET /api/<stream>/current` | Latest reading (triggers a fresh collector sample first) |
| `GET /api/<stream>/history?limit=100&offset=0` | Historical readings (pure DB read) |

Control endpoint:

| Endpoint | Description |
|---|---|
| `POST /api/config` | Apply runtime config changes to the collector (200 ok / 400 invalid / 503 collector down) |

## Architecture

One Rust workspace, two processes:

- **`spark-collect`** — samples hardware on a timer and writes rows to a local SQLite DB (WAL mode)
- **`spark-serve`** — tiny HTTP server that reads the same DB and serves the Svelte frontend plus a small REST API

Data and control planes are separated: **data** flows through the shared SQLite DB (server is read-only — so any number of browser tabs can connect without lock contention); **control** flows over a Unix socket — the server can ask for fresh samples or apply config changes at runtime, and receives only a small confirmation, never the data itself. The frontend is a single Svelte app with no external chart library.

If the collector is down, `/api/<stream>/current` transparently falls back to the latest DB rows; `POST /api/config` returns `503`.

## API consumers & scripting

Because `spark-serve` is a small HTTP server you can script against it:

```bash
# JSON snapshot of a stream
curl -s http://localhost:8090/api/memory/current | jq

# N latest sample timestamps
curl -s "http://localhost:8090/api/memory/history?limit=24" | jq '.[].timestamp'
```

## Author & contact

Built by **Embracing Challenges** — find the project and updates at **[@walkthegrowth](https://x.com/walkthegrowth)** on X.

## License

MIT — see [LICENSE](LICENSE). Copyright (c) 2026 Embracing Challenges (@walkthegrowth).

## Developers

- Build & run in one command: `./run.sh` (builds backend + frontend, starts both on `:8090`; Ctrl-C shuts both down)
- Backend tests: `cd backend && cargo test`
- Frontend type-check: `cd frontend && npm run check`
- Build a release bundle: `tools/make-bundle.sh` (add `--release --version vX.Y.Z` + `GH_TOKEN` to publish)
