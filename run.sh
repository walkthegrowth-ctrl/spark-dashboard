#!/bin/bash
set -e

PORT=8090
CONFIG="backend/config/default.toml"

while [[ $# -gt 0 ]]; do
  case $1 in
    --port) PORT="$2"; shift 2 ;;
    --config) CONFIG="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--port PORT] [--config PATH]"
      echo ""
      echo "Starts the Spark Dashboard (collector + server)"
      echo ""
      echo "Options:"
      echo "  --port PORT   Server port (default: 8090)"
      echo "  --config PATH Config file (default: backend/config/default.toml)"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Spark Dashboard ==="
echo "Port: $PORT"
echo "Config: $CONFIG"
echo ""

# Build backend
echo "Building backend..."
cd backend
cargo build --release 2>&1 | tail -1
cd ..

# Build frontend
echo "Building frontend..."
cd frontend
npm run build > /dev/null 2>&1
cd ..

# Cleanup function
CLEANUP_DONE=0
cleanup() {
  [ $CLEANUP_DONE -eq 1 ] && return
  CLEANUP_DONE=1
  echo ""
  echo "Shutting down..."
  pkill -f spark-collect 2>/dev/null || true
  pkill -f spark-serve 2>/dev/null || true
  sleep 1
  pkill -9 -f spark-collect 2>/dev/null || true
  pkill -9 -f spark-serve 2>/dev/null || true
  wait 2>/dev/null || true
  echo "Done."
  exit 0
}
trap cleanup EXIT INT TERM

# Start collector in background
echo "Starting collector..."
SPARK_CONFIG_PATH="$CONFIG" ./backend/target/release/spark-collect &
COLLECTOR_PID=$!

# Start server in background
echo "Starting server on port $PORT..."
SPARK_CONFIG_PATH="$CONFIG" ./backend/target/release/spark-serve --port "$PORT" &
SERVER_PID=$!

echo "Open http://localhost:$PORT in your browser"
echo ""

# Wait for either process to exit
wait
