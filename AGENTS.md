# Development Notes

## Rules

- Always do thorough debugging with the browser tool (i.e. using Playwright with Chromium) — verify UI changes in a real browser, not just via API/curl.
- Never push to GitHub without permission.

## Running the dashboard

The app is two long-running processes, normally started together via `./run.sh` (port 8090):

- `spark-collect` — collector daemon (no CLI port; reads config)
- `spark-serve` — HTTP server (`--port`, `--config` flags)

**Before starting new instances, check whether they are already running:**

```bash
ps aux | grep -E "spark-(collect|serve)" | grep -v grep
curl -s http://127.0.0.1:8090/api/memory/current  # liveness check
```

If a restart is needed (e.g. after code changes), it is fine to kill the existing ones — `run.sh`'s own cleanup trap does the same:

```bash
pkill -f spark-collect || true
pkill -f spark-serve || true
```

Then restart (foreground for interactive use, or `nohup ./run.sh >/tmp/spark-run.log 2>&1 &` for background).

## Build & verify

- Backend: `cd backend && cargo build` / `cargo test`
- Frontend: `cd frontend && npm run build` (builds into `backend/static`)
- `run.sh` rebuilds both; use it for user-facing restarts.

## API

- Streams are `memory`, `thermal`, `compute`, `power` (4 total). For each:
  - `GET /api/<stream>/current` — latest reading (triggers a fresh collector sample)
  - `GET /api/<stream>/history?limit=&offset=` — pure DB read
- `POST /api/config` — apply runtime config to the collector; JSON body, any subset of `{retention_days, memory_interval_secs, thermal_interval_secs, compute_interval_secs, power_interval_secs}`. `200 {"ok":true}` / `400` invalid / `503` collector down.

- Config: `backend/config/default.toml` (DB at `~/.local/share/spark-dashboard/spark.db`). Both binaries read `SPARK_CONFIG_PATH` (defaults to `config/default.toml`, relative to CWD). `run.sh` sets it automatically; when running the binaries manually you must `cd backend` first (see README Quick Start).
- Two communication planes between the binaries: the **data plane** is the shared SQLite DB (collector writes, server reads — WAL mode); the **control plane** is the Unix socket at `[collector] ipc_socket` (`/tmp/spark-collect.sock`). Line-delimited JSON, one request per connection: `{"type":"sample","stream":"memory|thermal|compute|power"}` → collector takes and persists a fresh sample; `{"type":"config","changes":{...}}` → apply validated interval/retention changes at runtime. Response is a small confirmation (`ok`, `skipped?`, `timestamp`), never the data itself.
- `GET /api/<stream>/current` first sends a `sample` IPC (3 s timeout) then reads the latest DB rows; if the collector is down it serves the latest rows anyway (fallback). `compute` samples within 2 s of the last one are suppressed (skip-guard) to avoid noisy utilization deltas.
- History retention is enforced: the collector prunes rows older than `retention_days` at most once per 6 h.
- Default collector intervals are 5 s for every stream (memory / thermal / compute / power). Override per-stream in the TOML config, live via `POST /api/config` (`memory_interval_secs`, `power_interval_secs`, etc.), or by restarting.

## Gotchas

- Frontend has no test runner; `npm run check` (svelte-check) is the typecheck. Browser verification is via Playwright (devDependency in `frontend/package.json`); `tools/verify_ipc.js` is a ready-made headless check against a running instance.
- `npm run dev` (vite, default port 5173) proxies `/api` to `http://localhost:8090` — start the backend first, or the UI loads with no data.
- `run.sh` traps SIGINT/SIGTERM and `pkill`s both binaries on exit — don't run it inside a wrapper that reaps its process group mid-session (the daemon dies with the wrapper).
