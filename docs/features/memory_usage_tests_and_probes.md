# Tests and Probes: Memory Usage Reporting

## Tests

### Unit Tests — Parser (`src/collector/`)

**Test: parse_proc_meminfo happy path**
- Input: valid `/proc/meminfo` content with all expected fields
- Expected: all fields parsed correctly into struct

**Test: parse_proc_meminfo missing field**
- Input: `/proc/meminfo` content with `MemAvailable` missing
- Expected: field is `None`, no crash

**Test: parse_proc_meminfo malformed value**
- Input: `/proc/meminfo` with non-numeric value for a field
- Expected: field is `None`, no crash

**Test: parse_proc_meminfo empty input**
- Input: empty string
- Expected: all fields `None`, no crash

**Test: parse_proc_meminfo extra fields**
- Input: `/proc/meminfo` with unexpected fields
- Expected: extra fields ignored, known fields parsed correctly

### Unit Tests — Database (`src/db/`)

**Test: insert_memory_stat**
- Input: valid MemoryStat record
- Expected: row inserted, returns row ID

**Test: get_latest_memory_stat**
- Input: database with existing records
- Expected: returns most recent record by timestamp

**Test: get_memory_stats_paginated**
- Input: database with N records, limit=10, offset=0
- Expected: returns first 10 records, oldest first

**Test: get_memory_stats_paginated offset**
- Input: database with N records, limit=10, offset=10
- Expected: returns next 10 records

**Test: get_memory_stats_paginated exceeds bounds**
- Input: database with 5 records, limit=10, offset=0
- Expected: returns all 5 records, total=5

**Test: get_memory_stats_empty**
- Input: empty database
- Expected: returns empty data array, total=0

**Test: create_table if not exists**
- Input: fresh database
- Expected: table created, no error on repeated call

### Unit Tests — API Handlers (`src/server/`)

**Test: handle_memory_current**
- Input: database with records
- Expected: 200, JSON matches latest record

**Test: handle_memory_current empty db**
- Input: empty database
- Expected: 503 or null response (TBD)

**Test: handle_memory_history defaults**
- Input: database with records, no query params
- Expected: 200, limit=100 applied, offset=0

**Test: handle_memory_history custom params**
- Input: `?limit=50&offset=10`
- Expected: 200, correct slice of records

**Test: handle_memory_history limit exceeds max**
- Input: `?limit=5000`
- Expected: 400, limit capped or rejected

**Test: handle_memory_history invalid limit**
- Input: `?limit=-1` or `?limit=abc`
- Expected: 400, bad request

**Test: handle_memory_history invalid offset**
- Input: `?offset=-1` or `?offset=abc`
- Expected: 400, bad request

### Unit Tests — Config (`src/config/`)

**Test: load default config**
- Input: default.toml
- Expected: all fields populated with expected defaults

**Test: load test config**
- Input: test.toml
- Expected: in-memory DB, port 0

**Test: config missing field**
- Input: TOML with missing `collection_interval_secs`
- Expected: default value used

### Integration Tests (`tests/`)

**Test: end-to-end memory collection**
- Start collector with test config
- Wait for one collection cycle
- Query `GET /api/memory/current`
- Expected: returns valid data matching system memory

**Test: end-to-end memory history**
- Start collector, wait for 3 collection cycles
- Query `GET /api/memory/history?limit=10`
- Expected: returns 3 records with increasing timestamps

**Test: collector graceful shutdown**
- Start collector, send signal
- Expected: clean exit, no data corruption

**Test: concurrent API reads**
- Start server, spawn 10 concurrent requests to `/api/memory/current`
- Expected: all return 200, no panics

## Probes

### Probe: Collector Memory Usage

**What it measures:** RSS memory of the collector process
**How it's run:** Read `/proc/<pid>/status` VmRSS field every 5 seconds during a 60-second test run
**Criteria:** RSS stays below 5 MB throughout the test run

### Probe: API Latency — Current Endpoint

**What it measures:** P95 response time for `GET /api/memory/current`
**How it's run:** Send 100 sequential requests, measure server response time
**Criteria:** P95 < 100ms

### Probe: API Latency — History Endpoint

**What it measures:** P95 response time for `GET /api/memory/history?limit=100`
**How it's run:** Send 100 sequential requests, measure server response time
**Criteria:** P95 < 500ms

### Probe: SQLite Write Rate

**What it measures:** Time to insert a single memory record
**How it's run:** Measure wall time for 1000 sequential inserts, divide by 1000
**Criteria:** Average insert < 1ms

### Probe: Collector Idle CPU

**What it measures:** CPU usage of collector when idle (between collection cycles)
**How it's run:** Sample `/proc/<pid>/stat` over 30 seconds, compute average CPU%
**Criteria:** Average CPU < 0.5% when idle

### Probe: Data Integrity After Restart

**What it measures:** No data loss or duplication after collector restart
**How it's run:** 
1. Collect for 3 cycles
2. Record row count
3. Restart collector
4. Collect for 2 more cycles
5. Record new row count
**Criteria:** Row count increased by exactly 2 (no duplicates, no loss)
