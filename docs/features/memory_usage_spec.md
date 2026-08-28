# Feature: Memory Usage Reporting

## Problem Statement

The DGX Spark has a unified memory architecture. Accurate memory reporting is essential for understanding system capacity, especially when running LLM workloads that consume significant memory. The default NVIDIA dashboard doesn't provide MemAvailable-aware reporting or historical trends.

## Functional Requirements

1. **Collector reads memory stats from `/proc/meminfo`** at configurable intervals
2. **Collector stores readings in SQLite** in a dedicated `memory_stats` table
3. **API endpoint `GET /api/memory/current`** returns the latest memory reading
4. **API endpoint `GET /api/memory/history`** returns historical readings with pagination
5. **Configurable collection interval** via server config (default: 60 seconds)

## Non-Functional Requirements

- Collector memory overhead: < 5 MB
- API response time: < 100ms for current, < 500ms for history
- SQLite WAL mode for concurrent read/write safety
- No external dependencies beyond stdlib and SQLite

## Error Handling

- `/proc/meminfo` unreadable: collector logs error, skips collection cycle
- SQLite write failure: collector retries once, then logs and continues
- API returns 500 on database errors, 400 on invalid query parameters
- Missing or malformed `/proc/meminfo` values: field returned as `null`, not a crash

## Edge Cases

- System with 0 available memory (OOM condition)
- Very large history queries (pagination limits)
- Collector restart: resumes from last known interval, no data duplication
- SQLite file locked: collector waits briefly, then retries

## API Surface

**`GET /api/memory/current`**
```json
{
  "timestamp": "2025-01-15T10:30:00Z",
  "mem_total_bytes": 17179869184,
  "mem_free_bytes": 2147483648,
  "mem_available_bytes": 8589934592,
  "buffers_bytes": 536870912,
  "cached_bytes": 4294967296,
  "swap_total_bytes": 0,
  "swap_free_bytes": 0
}
```

**`GET /api/memory/history?limit=100&offset=0`**
```json
{
  "count": 100,
  "total": 5000,
  "data": [ /* same structure as current */ ]
}
```

Query parameters: `limit` (default 100, max 1000), `offset` (default 0)

## Database Schema

```sql
CREATE TABLE memory_stats (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  timestamp TEXT NOT NULL,
  mem_total_bytes INTEGER NOT NULL,
  mem_free_bytes INTEGER NOT NULL,
  mem_available_bytes INTEGER NOT NULL,
  buffers_bytes INTEGER NOT NULL,
  cached_bytes INTEGER NOT NULL,
  swap_total_bytes INTEGER NOT NULL,
  swap_free_bytes INTEGER NOT NULL
);

CREATE INDEX idx_memory_stats_timestamp ON memory_stats(timestamp);
```

## Assumptions

- Linux `/proc/meminfo` format is stable on the target system
- SQLite is available as a static or system library
- The collector runs with read access to `/proc/meminfo`
- No GPU-specific memory partitioning (unified memory = OS memory)

## Known Unknowns

- Exact memory layout on GB10 (unified vs partitioned) — may need adjustment later
- Whether swap is present or relevant on the target system

## Out of Scope

- Per-process memory breakdown
- GPU memory metrics (separate feature)
- Memory alerts or thresholds
- Memory visualization (frontend feature)
