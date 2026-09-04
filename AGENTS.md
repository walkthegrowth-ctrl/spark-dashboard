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

## UI conventions (frontend)

- Every card follows a fixed three-level model, built from two reusable components in `frontend/src/components/`:
  - `CollapsibleCard.svelte` — the shell. Owns the **top toggle** (top-right) and an animated `.body` height. Exposes a `summary` slot (shown when the card is collapsed) and a default slot (the expanded content). Dispatches `toggle`.
  - `CollapsibleSection.svelte` — a reusable **second-level** region: a persistent header (optional `title` + `hint` on the left, chevron top-right) with the content area below it collapsing/expanding. Takes an `open` prop (the parent is the source of truth) and dispatches `toggle`.
- **All cards start fully collapsed** by default: each card component sets `let collapsed = true;` (showing only its `summary`), and any sections start closed (`graphsOpen = false`).
- When a card is expanded, its **second-level Time-dynamics section stays closed** by default and is expanded separately via its own chevron.
- **The top toggle resets section state:** each card's `onCardToggle` flips `collapsed` *and* sets `graphsOpen = false`, so reopening a card never restores a previously-open section.
- The **third level** (future): a section can contain more `CollapsibleSection`s, giving per-area expand/collapse; the chevron convention is always top-right and rotates to indicate direction.
- When adding a card, wrap its "time dynamics" charts in a `CollapsibleSection` inside the card and mirror the `collapsed`/`graphsOpen`/`onCardToggle`/`onGraphsToggle` pattern above.
- **Time-dynamics section hint** is always `since this page opened · using fixed scales` (do not re-list the axis ceilings there).

## Chart conventions (frontend)

- `Sparkline.svelte` renders a shared footer for **every** chart, driven by the component (callers don't author it):
  - **Bottom-left** — `Max: <axis max>` (the chart's fixed `max`, via the chart's `format`). Callers can override the label with the `maxLabel` prop — e.g. the Memory swap-free chart passes `maxLabel="Max: NA"` when the machine has no swap (it still plots against a **1 GB** scale so the line graph renders normally with values at 0).
  - **Bottom-right** — `Rec time: H:MM:SS`, the span of the in-memory history (newest sample − oldest).
- A chart's y-axis is always **fixed** at its natural ceiling (`max`), never auto-zoomed.
- **Power (Thermal) convention:** the power bar + GPU-power chart use a **120 W** ceiling (`POWER_MAX`); color/status bands are green <80, yellow <110, orange <120, red ≥120.

## Deployment (release bundle)

A self-contained per-machine bundle is the shipped artifact — no global installs, no package managers. aarch64-linux only (built on aarch64; `tools/make-bundle.sh` refuses other archs).

- **Bundle layout** (all files adjacent — the server resolves `static/` relative to its own exe dir or CWD, so the two must stay siblings):
  `spark-collect` · `spark-serve` · `static/` · `default.toml` · `spark.sh` · `LICENSE` · `SHA256SUMS` (inside the zip).
- **Build/publish:** `tools/make-bundle.sh` — builds both, assembles to `dist/`, verifies (`unzip` + `sha256sum -c`), writes `dist/spark-dashboard-aarch64-linux.zip` + `.zip.sha256`. Publishes to GitHub Releases when `--release --version vX.Y.Z` is given and `GH_TOKEN`/`GITHUB_TOKEN` is set (creates the release and uploads zip + sha256 + `install.sh` via the GitHub API). Without a token it stops at a ready `dist/`.
- **Install:** `install.sh` (curl | sh) — platform preflight, download with a live meter, sha256 verification (refuses on mismatch), extraction to `--dest` (default `~/.spark`). It never auto-starts. `SPARK_DASHBOARD_REPO_BASE=<base>` points it at any plain-HTTP mirror instead of GitHub. Uninstall: `install.sh --uninstall [--purge-db]`.
- **Runtime:** `spark.sh start [PORT]` / `stop` / `status` / `logs`. Daemons are backgrounded from a detached subshell (no `disown`); state in `<dest>/.spark/` (pid files, port, logs). Port priority: CLI arg → last-used (`.spark/port`) → installer hint (`.install-port`) → `default.toml`. `stop` does SIGTERM, waits, then SIGKILL.
- **Per-machine state:** DB at `~/.local/share/spark-dashboard/spark.db` (from `default.toml`), single-instance `flock` files live *in the DB directory* — two daemons on two different DBs may coexist; a second one on the same DB is refused. Reinstall/uninstall never touches the DB (except `--purge-db`).
- **Gotcha:** both binaries honor `SPARK_CONFIG_PATH`; `spark-serve` also takes `--config` (which overrides the env var). The bundled `default.toml` is read because `spark.sh` `cd`s into its own dir first — never launch the bundle binaries from elsewhere without setting the config path.

## Gotchas

- Frontend has no test runner; `npm run check` (svelte-check) is the typecheck. Browser verification is via Playwright (devDependency in `frontend/package.json`); `tools/verify_ipc.js` is a ready-made headless check against a running instance.
- `npm run dev` (vite, default port 5173) proxies `/api` to `http://localhost:8090` — start the backend first, or the UI loads with no data.
- `run.sh` traps SIGINT/SIGTERM and `pkill`s both binaries on exit — don't run it inside a wrapper that reaps its process group mid-session (the daemon dies with the wrapper).
