#!/usr/bin/env bash
#
# spark.sh — runtime manager for the Spark Dashboard
#
#   spark.sh start [PORT]    Start collector + server (background daemons)
#   spark.sh stop            Stop them cleanly
#   spark.sh status          Show state, port, and liveness
#   spark.sh logs [N]        Tail the last N log lines (default 40)
#
# State lives in ./.spark/ (pid files, logs, runtime port).
#
set -uo pipefail

SELF="$0"
BIN_DIR="$(cd "$(dirname "$SELF")" && pwd)"
STATE_DIR="$BIN_DIR/.spark"
LOG_DIR="$STATE_DIR/logs"
COLLECT_PID_FILE="$STATE_DIR/collect.pid"
SERVE_PID_FILE="$STATE_DIR/serve.pid"
PORT_FILE="$STATE_DIR/port"
LOG_COLLECT="$LOG_DIR/spark-collect.log"
LOG_SERVE="$LOG_DIR/spark-serve.log"

COLLECT_BIN="$BIN_DIR/spark-collect"
SERVE_BIN="$BIN_DIR/spark-serve"
CONFIG="$BIN_DIR/default.toml"

if [ ! -x "$COLLECT_BIN" ] || [ ! -x "$SERVE_BIN" ]; then
  echo "error: spark-collect / spark-serve not found next to $(basename "$SELF")" >&2
  exit 1
fi

# --- cosmetics -------------------------------------------------------------------
if [ -t 1 ]; then
  C_CYAN=$'\033[36m'; C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'
  C_RED=$'\033[31m';  C_DIM=$'\033[2m';   C_BOLD=$'\033[1m'; C_RESET=$'\033[0m'
else
  C_CYAN=""; C_GREEN=""; C_YELLOW=""; C_RED=""; C_DIM=""; C_BOLD=""; C_RESET=""
fi

say()  { printf '%s\n' "$*"; }
ok()   { printf '  %s✓ %s%s\n' "$C_GREEN" "$*" "$C_RESET"; }
warn() { printf '  %s⚠ %s%s\n' "$C_YELLOW" "$*" "$C_RESET" >&2; }
die()  { printf '\n%s✗ %s%s\n' "$C_RED" "$*" "$C_RESET" >&2; exit 1; }

# --- helpers ----------------------------------------------------------------------
is_running() { # $1 = pidfile
  local f="$1" pid
  [ -f "$f" ] || return 1
  pid=$(cat "$f" 2>/dev/null)
  case "$pid" in (''|*[!0-9]) return 1 ;; esac
  kill -0 "$pid" 2>/dev/null
}

read_port() {
  local p
  if [ -f "$PORT_FILE" ]; then
    p=$(awk 'NR==1{print $1}' "$PORT_FILE")
    case "$p" in (''|*[!0-9]) p="" ;; esac
  fi
  if [ -n "$p" ]; then echo "$p"; return; fi
  # fall back to the shipped config
  awk '/^port[[:space:]]*=[[:space:]]*[0-9]+/{gsub(/[^0-9]/,"",$3); print $3; exit}' "$CONFIG" 2>/dev/null
}

http_code() { # $1 = url -> HTTP status (000 on failure)
  curl -o /dev/null -s -m 3 -w '%{http_code}' "$1" 2>/dev/null || echo 000
}

wait_http_up() { # $1 = port
  local port=$1 i code
  for i in $(seq 1 40); do
    code=$(http_code "http://127.0.0.1:${port}/")
    case "$code" in 2*|3*) return 0 ;; esac
    sleep 0.25
  done
  return 1
}

wait_gone() { # $1 = pidfile
  local f="$1" pid i
  [ -f "$f" ] || return 0
  pid=$(cat "$f" 2>/dev/null)
  case "$pid" in (''|*[!0-9]) return 0 ;; esac
  for i in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.25
  done
  kill -0 "$pid" 2>/dev/null && kill -KILL "$pid" 2>/dev/null
  return 0
}

# --- start --------------------------------------------------------------------------
do_start() {
  local port="${1:-}"
  if is_running "$SERVE_PID_FILE" || is_running "$COLLECT_PID_FILE"; then
    local running_port
    running_port=$(read_port)
    warn "already running (port ${running_port:-?}) — run 'spark.sh status' / 'spark.sh stop'"
    exit 0
  fi

  if [ -z "$port" ]; then
    if [ -f "$PORT_FILE" ]; then
      port=$(awk 'NR==1{print $1}' "$PORT_FILE")
    fi
  fi
  if [ -z "$port" ] && [ -f "$BIN_DIR/.install-port" ]; then
    port=$(sed -n 's/^PORT_DEFAULT=//p' "$BIN_DIR/.install-port")
  fi
  if [ -z "$port" ]; then
    port=$(awk '/^port[[:space:]]*=[[:space:]]*[0-9]+/{gsub(/[^0-9]/,"",$3);print $3;exit}' "$CONFIG" 2>/dev/null || echo 8090)
  fi
  case "$port" in (''|*[!0-9]) die "could not determine a port — give one explicitly: spark.sh start 8090" ;; esac

  mkdir -p -- "$STATE_DIR" "$LOG_DIR"

  local code
  code=$(http_code "http://127.0.0.1:${port}/")
  case "$code" in
    2*|3*) warn "port $port already answers — is another instance running? (try 'spark.sh start <other-port>')" ;;
  esac

  say "  starting Spark Dashboard"
  say "  ${C_DIM}collector + server · port $port · state $STATE_DIR${C_RESET}"

  # --- collector
  rm -f -- "$COLLECT_PID_FILE"
  (
    cd "$BIN_DIR"
    export SPARK_CONFIG_PATH="$CONFIG"
    exec "$COLLECT_BIN" >>"$LOG_COLLECT" 2>&1
  ) &
  COLLECT_PID=$!
  echo "$COLLECT_PID" > "$COLLECT_PID_FILE"
  disown "$COLLECT_PID" 2>/dev/null || true
  ok "collector · pid $COLLECT_PID"

  # --- server
  rm -f -- "$SERVE_PID_FILE"
  (
    cd "$BIN_DIR"
    export SPARK_CONFIG_PATH="$CONFIG"
    exec "$SERVE_BIN" --port "$port" >>"$LOG_SERVE" 2>&1
  ) &
  SERVE_PID=$!
  echo "$SERVE_PID" > "$SERVE_PID_FILE"
  disown "$SERVE_PID" 2>/dev/null || true
  ok "server · pid $SERVE_PID"

  # --- verify: give the collector a moment, then ask for a fresh sample
  if wait_http_up "$port"; then
    echo "$port" > "$PORT_FILE"
    say ""
    say "  ${C_GREEN}${C_BOLD}✓ Spark Dashboard is up${C_RESET}"
    echo "  127.0.0.1:${port}"
    say "${C_DIM}  stop it with:  $(basename "$SELF") stop${C_RESET}"
    say "${C_DIM}  logs at:      $LOG_DIR${C_RESET}"
  else
    warn "server did not answer on :$port within ~10 s"
    warn "last server log lines:"
    tail -n 8 "$LOG_SERVE" 2>/dev/null | sed 's/^/    /'
    exit 1
  fi
}

# --- stop -----------------------------------------------------------------------------
do_stop() {
  if ! is_running "$SERVE_PID_FILE" && ! is_running "$COLLECT_PID_FILE"; then
    ok "not running (nothing to stop)"
    exit 0
  fi
  say "  stopping Spark Dashboard"
  # stop the server first (so it stops asking the collector to sample), then the collector
  if is_running "$SERVE_PID_FILE"; then
    local pid
    pid=$(cat "$SERVE_PID_FILE")
    kill -TERM "$pid" 2>/dev/null || true
    wait_gone "$SERVE_PID_FILE"
    ok "server stopped"
  fi
  if is_running "$COLLECT_PID_FILE"; then
    local pid
    pid=$(cat "$COLLECT_PID_FILE")
    kill -TERM "$pid" 2>/dev/null || true
    wait_gone "$COLLECT_PID_FILE"
    ok "collector stopped"
  fi
  rm -f -- "$COLLECT_PID_FILE" "$SERVE_PID_FILE"
  say ""
  ok "stopped"
}

# --- status -----------------------------------------------------------------------------
do_status() {
  if is_running "$SERVE_PID_FILE" && is_running "$COLLECT_PID_FILE"; then
    local port code
    port=$(read_port); [ -z "$port" ] && port="?"
    code=$(http_code "http://127.0.0.1:${port}/")
    case "$code" in
      2*|3*)
        echo "  ${C_GREEN}${C_BOLD}● up${C_RESET} — http://127.0.0.1:${port}  ${C_DIM}(http $code)${C_RESET}"
        echo "  ${C_DIM}collector $(cat "$COLLECT_PID_FILE") · server $(cat "$SERVE_PID_FILE") · logs $LOG_DIR${C_RESET}"
        ;;
      *)
        echo "  ${C_YELLOW}${C_BOLD}◐ starting…${C_RESET} — daemons alive, HTTP not answering yet (port ${port})"
        ;;
    esac
  elif is_running "$SERVE_PID_FILE" || is_running "$COLLECT_PID_FILE"; then
    warn "partially running (one daemon missing) — try 'spark.sh stop' then 'spark.sh start'"
  else
    echo "  ${C_DIM}○ not running${C_RESET}"
  fi
}

# --- logs -----------------------------------------------------------------------------
do_logs() {
  local n="${1:-40}"
  mkdir -p -- "$LOG_DIR"
  say "  ${C_BOLD}— spark-serve.log${C_RESET} ${C_DIM}(last $n)${C_RESET}"
  tail -n "$n" "$LOG_SERVE" 2>/dev/null | sed 's/^/  /'
  say ""
  say "  ${C_BOLD}— spark-collect.log${C_RESET} ${C_DIM}(last $n)${C_RESET}"
  tail -n "$n" "$LOG_COLLECT" 2>/dev/null | sed 's/^/  /'
}

# --- help -----------------------------------------------------------------------------
usage() {
  say "  ${C_BOLD}spark.sh${C_RESET} — Spark Dashboard runtime manager"
  say ""
  say "  ${C_BOLD}Usage${C_RESET}  $(basename "$SELF") <command> [args]"
  say ""
  say "  ${C_CYAN}start [PORT]${C_RESET}   Start collector + server as background daemons"
  say "                   ${C_DIM}port defaults to the last one used, else 8090${C_RESET}"
  say "  ${C_CYAN}stop${C_RESET}         Stop both processes (SIGTERM, then SIGKILL if needed)"
  say "  ${C_CYAN}status${C_RESET}       Show running state, port, and liveness"
  say "  ${C_CYAN}logs [N]${C_RESET}     Print the last N log lines from both processes"
  say ""
  say "  ${C_DIM}state & logs live in ${STATE_DIR}${C_RESET}"
  exit 0
}

case "${1:-}" in
  start)  shift; do_start "${1:-}" ;;
  stop)   do_stop ;;
  status) do_status ;;
  logs)   shift; do_logs "${1:-40}" ;;
  -h|--help|help) usage ;;
  "")
    # bare invocation: convenience shortcut → status
    do_status
    echo "  ${C_DIM}hint: $(basename "$SELF") --help${C_RESET}"
    ;;
  *)
    echo "unknown command: $1" >&2
    echo "try: $(basename "$SELF") --help" >&2
    exit 2
    ;;
esac
