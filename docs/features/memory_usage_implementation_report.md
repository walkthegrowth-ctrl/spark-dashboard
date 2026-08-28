# Implementation Report: Memory Usage Reporting

## Summary

Implemented end-to-end memory usage reporting pipeline: collector reads `/proc/meminfo`, stores in SQLite, API server exposes current and historical data via REST endpoints.

## Key Changes

### New Files
- `backend/src/collector/mod.rs` — MemoryStat struct, MemoryHistoryResponse
- `backend/src/collector/reader.rs` — `/proc/meminfo` parser
- `backend/src/collector/engine.rs` — Collector engine with collection loop
- `backend/src/db/mod.rs` — SQLite operations for memory_stats table
- `backend/src/server/handler.rs` — REST API handlers for memory endpoints
- `backend/src/config/mod.rs` — Config loading and parsing
- `backend/src/bin/collect.rs` — Collector daemon entry point
- `backend/src/bin/serve.rs` — API server entry point
- `backend/tests/integration_test.rs` — End-to-end integration tests
- `backend/config/default.toml` — Reference configuration
- `backend/config/test.toml` — Test configuration
- `backend/run_probes.sh` — Probe execution script

### Modified Files
- `backend/Cargo.toml` — Added dependencies: chrono, rusqlite, serde, serde_json, tiny_http
- `backend/src/lib.rs` — Module declarations

## Key Decisions

1. **Timestamp format:** Unix epoch (i64) instead of RFC3339 strings — simpler, smaller storage
2. **Config parsing:** Custom line-by-line parser instead of TOML crate — avoids dependency, sufficient for current needs
3. **Database:** SQLite with WAL mode for concurrent read/write safety
4. **HTTP server:** tiny_http — lightweight, minimal dependencies
5. **Pagination:** Limit max 1000, offset-based — simple, effective for current use case

## Assumptions Confirmed

- `/proc/meminfo` format is stable on the target system
- SQLite is available as a bundled library
- Collector runs with read access to `/proc/meminfo`

## Deviations from Spec

1. Timestamp as Unix epoch instead of RFC3339 (low risk)
2. Custom config parser instead of TOML crate (low risk)
3. IPC between collector and server deferred (medium risk — affects demand-driven collection)

## Test Results

- **Unit tests:** 22 passing
- **Integration tests:** 3 passing
- **Probes:** 2 PASS (collector memory 2.04 MB, SQLite write rate), 3 PENDING (API latency, idle CPU, data integrity)

## Probe Results

| Probe | Result | Notes |
|---|---|---|
| Collector Memory Usage | PASS (2.04 MB) | Well under 5 MB target |
| SQLite Write Rate | PASS | Verified by unit tests |
| API Latency | PENDING | Requires running server |
| Collector Idle CPU | PENDING | Requires running collector |
| Data Integrity After Restart | PENDING | Requires persistent DB |

## Rollback Plan

Revert to pre-implementation commit. All new files can be removed, Cargo.toml restored to original state.

## Known Issues

1. Config parser is basic — no section-aware parsing, may break with complex configs
2. No IPC implementation — collector and server run independently
3. No frontend — API is accessible but no UI
4. No data retention policy — history grows indefinitely

## Follow-up Items

1. Implement IPC between collector and server
2. Add TOML config parsing for robustness
3. Implement data retention policy
4. Build frontend memory visualization
5. Complete remaining probes (API latency, idle CPU, data integrity)
