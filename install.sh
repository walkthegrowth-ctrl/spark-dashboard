#!/usr/bin/env bash
#
# Spark Dashboard — one-shot installer
#
#   curl -fsSL https://github.com/walkthegrowth-ctrl/spark-dashboard/releases/latest/download/install.sh | sh
#
#   curl -fsSL <url> | sh -s -- --dest /opt/spark --port 11020
#
# What it does:
#   1. Verifies the platform (aarch64-linux) and required tools
#   2. Downloads the release bundle (with a live progress meter)
#   3. Verifies the bundle against its published checksum
#   4. Extracts it to the install directory (default: ~/.spark)
#
# It does NOT start the dashboard. Run afterwards:  <dest>/spark.sh start
#
# Options:
#   --dest DIR        Install directory (default: ~/.spark)
#   --version TAG     Install a specific release instead of latest (e.g. v1.0.0)
#   --port PORT       Remember a preferred port as the default for `spark.sh start`
#   --uninstall       Remove the installation
#   --purge-db        With --uninstall: also delete the local database
#   --reinstall       Re-download over an existing installation
#   -h, --help        Show this help
#
set -euo pipefail

REPO="walkthegrowth-ctrl/spark-dashboard"
ASSET="spark-dashboard-aarch64-linux.zip"
CHECKSUM_NAME="spark-dashboard-aarch64-linux.zip.sha256"

# --- terminal cosmetics -------------------------------------------------------
if [ -t 1 ]; then
  C_CYAN=$'\033[36m'; C_GREEN=$'\033[32m'; C_YELLOW=$'\033[33m'
  C_RED=$'\033[31m';  C_DIM=$'\033[2m';   C_BOLD=$'\033[1m'; C_RESET=$'\033[0m'
else
  C_CYAN=""; C_GREEN=""; C_YELLOW=""; C_RED=""; C_DIM=""; C_BOLD=""; C_RESET=""
fi

# --- argument parsing ---------------------------------------------------------
DEST="${HOME}/.spark"
VERSION=""
PORT=""
ACTION="install"
PURGE_DB=0
REINSTALL=0

usage() {
  sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --dest)       DEST="${2:?--dest needs a value}"; shift 2 ;;
    --version)    VERSION="${2:?--version needs a value}"; shift 2 ;;
    --port)       PORT="${2:?--port needs a value}"; shift 2 ;;
    --uninstall)  ACTION="uninstall"; shift ;;
    --purge-db)   PURGE_DB=1; shift ;;
    --reinstall)  REINSTALL=1; shift ;;
    -h|--help)    usage ;;
    *) echo "unknown option: $1 (try --help)" >&2; exit 2 ;;
  esac
done

case "$DEST" in
  /*) : ;;
  *) DEST="$(pwd)/$DEST" ;;
esac

# --- small helpers -------------------------------------------------------------
say()  { printf '%s\n' "$*"; }
step() { printf '\n%s▸ %s%s' "$C_CYAN" "$*" "$C_RESET"; }
ok()   { printf '  %s✓ %s%s\n' "$C_GREEN" "$*" "$C_RESET"; }
warn() { printf '%s  ⚠ %s%s\n' "$C_YELLOW" "$*" "$C_RESET" >&2; }
die()  { printf '\n%s✗ %s%s\n' "$C_RED" "$*" "$C_RESET" >&2; exit 1; }

bytes_human() {
  local b="${1:-0}"
  case "$b" in (''|*[!0-9]) b=0 ;; esac
  if   [ "$b" -ge 1073741824 ]; then awk -v b="$b" 'BEGIN{printf "%.1f GiB", b/1073741824}'
  elif [ "$b" -ge 1048576 ];  then awk -v b="$b" 'BEGIN{printf "%.1f MiB", b/1048576}'
  elif [ "$b" -ge 1024 ];     then awk -v b="$b" 'BEGIN{printf "%.0f KiB", b/1024}'
  else printf '%d B' "$b"
  fi
}

banner() {
  say "  ${C_BOLD}Spark Dashboard${C_RESET} ${C_DIM}·${C_RESET} installer"
}

file_size() { # $1 = path -> number of bytes (0 if unreadable)
  local n
  n=$(wc -c < "$1" 2>/dev/null || true)
  case "$n" in (''|*[!0-9]) echo 0 ;; (*) echo "$n" ;; esac
}

# --- download meter state ------------------------------------------------------
BAR_WIDTH=34
SPIN='⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏'
_spin_i=0
_bar_prev=0
_bar_t0=""
_bar_tprev=""

_bar_draw() { # $1 = bytes done   $2 = total bytes or "" for unknown
  local done=${1:-0} total=${2:-} now fill pct speed eta f t line
  now=$(date +%s%N 2>/dev/null || date +%s)
  case "$now" in *[!0-9]*) now=$RANDOM ;; esac
  [ -z "$_bar_t0" ] && _bar_t0=$now
  _spin_i=$(( (_spin_i + 1) % 10 ))

  if [ -n "$total" ] && [ "$total" -gt 0 ]; then
    fill=$(( done * BAR_WIDTH / total ))
    [ "$fill" -gt "$BAR_WIDTH" ] && fill=$BAR_WIDTH
    [ "$fill" -lt 0 ] && fill=0
    pct=$(( (done * 100) / total ))
    f=$(printf '%*s' "$fill" '' | tr ' ' '▓')
    t=$(printf '%*s' $(( BAR_WIDTH - fill )) '' | tr ' ' '·')
    speed=""
    if [ -n "$_bar_tprev" ] && [ "($_bar_tprev - $_bar_t0)" -ge 1 ]; then
      local dt=$(( (_bar_tprev - _bar_t0) / 1000000 ))
      case "$dt" in 0|'') dt=1 ;; esac
      speed=$(( (done - _bar_prev) / dt ))
      [ "$speed" -lt 0 ] && speed=0
    fi
    eta=""
    if [ -n "$speed" ] && [ "$speed" -gt 0 ]; then
      eta=$(( (total - done) / speed ))
      [ "$eta" -gt 0 ] && eta="  ${C_DIM}~${eta}s left${C_RESET}"
    fi
    local rate
    rate=$( [ -n "$speed" ] && bytes_human "$speed" || : )
    line="  ${f}${C_DIM}${t}${C_RESET} ${pct}%  ${C_DIM}$(bytes_human "$done") / $(bytes_human "$total")${C_RESET}"
    [ -n "$rate" ] && line="$line ${C_DIM}${rate}/s${C_RESET}"
    [ -n "$eta" ] && line="$line$eta"
  else
    line="  ${SPIN:$_spin_i:1}  ${C_DIM}downloading… $(bytes_human "$done")${C_RESET}"
  fi
  _bar_prev=$done
  _bar_tprev=$now
  printf '\r%s' "$line"
  return 0
}

download() { # $1 = url   $2 = dest
  local url=$1 out=$2 total
  total=$(curl -fsIL --retry 2 "$url" 2>/dev/null \
          | awk 'BEGIN{IGNORECASE=1} /content-length: [0-9]+$/ {v=$2} END{print v}' || true)
  case "$total" in (''|*[!0-9]*) total="" ;; esac

  # redraw loop in background: poll file size, draw the bar, until done
  (
    while :; do
      sleep 0.15
      _bar_draw "$(file_size "$out")" "${total:-}" || exit 0
    done
  ) &
  DRAW_PID=$!

  local rc=0
  curl -fSL --retry 3 --retry-delay 2 --connect-timeout 15 \
       -o "$out" "$url" --silent --show-error || rc=$?
  kill "$DRAW_PID" 2>/dev/null || true
  wait "$DRAW_PID" 2>/dev/null || true

  if [ $rc -eq 0 ]; then
    _bar_draw "$(file_size "$out")" "${total:-}"
    printf '\n  %s ✓ %s done%s\n' "$C_GREEN" "$(bytes_human "$(file_size "$out")")" "$C_RESET"
  else
    printf '\n'
  fi
  return $rc
}

# --- uninstall ------------------------------------------------------------------
do_uninstall() {
  banner
  say ""
  step "stopping dashboard (if running)"
  if [ -x "$DEST/spark.sh" ]; then
    "$DEST/spark.sh" stop >/dev/null 2>&1 || true
  fi
  ok "stopped"
  step "removing $DEST"
  rm -rf -- "$DEST"
  ok "removed"
  if [ "$PURGE_DB" -eq 1 ] && [ -e "${HOME}/.local/share/spark-dashboard" ]; then
    step "removing local database"
    rm -rf -- "${HOME}/.local/share/spark-dashboard"
    ok "removed"
  else
    say "  database kept at ${HOME}/.local/share/spark-dashboard"
    say "  (remove it with: sh -s -- --uninstall --purge-db)"
  fi
  say ""
  ok "uninstalled"
  exit 0
}

# ============================================================================
[ "$ACTION" = "uninstall" ] && do_uninstall

banner
say ""
TMP=""
TMP=$(mktemp -d "${TMPDIR:-/tmp}/spark-install.XXXXXX")
trap 'rm -rf -- "$TMP"' EXIT

# --- preflight ------------------------------------------------------------------
step "checking platform"
ARCH=$(uname -m 2>/dev/null || echo unknown)
OS=$(uname -s 2>/dev/null || echo unknown)
if [ "$ARCH" != "aarch64" ] || [ "$OS" != "Linux" ]; then
  die "unsupported platform ($OS/$ARCH) — the bundle is aarch64-linux only"
fi
ok "$OS/$ARCH"

for tool in curl unzip sha256sum; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done
ok "curl · unzip · sha256sum available"

case "$PORT" in ('') : ;; (''|*[!0-9]) : ;; esac
if [ -n "$PORT" ]; then
  case "$PORT" in *[!0-9]*) die "--port must be numeric (got: $PORT)" ;; esac
fi

if [ ! "$REINSTALL" ] && [ -e "$DEST" ]; then
  die "$DEST already exists — pass --reinstall to replace it, or --uninstall to remove it first"
fi
[ "$PURGE_DB" -eq 1 ] && [ "$ACTION" = "install" ] && warn "--purge-db only applies to --uninstall (ignored)"

# --- resolve download base --------------------------------------------------------
# For a local test / self-host: set SPARK_DASHBOARD_REPO_BASE to a base URL
# that serves <base>/<asset> and <base>/<checksum>. Otherwise GitHub Releases.
BASE="${SPARK_DASHBOARD_REPO_BASE:-}"
if [ -z "$BASE" ]; then
  if [ -n "$VERSION" ]; then
    BASE="https://github.com/$REPO/releases/download/$VERSION"; LABEL="$VERSION"
  else
    BASE="https://github.com/$REPO/releases/latest/download"; LABEL="latest"
  fi
else
  [ -n "$VERSION" ] && BASE="$BASE/$VERSION"
  LABEL="custom"
fi
ZIP_URL="$BASE/$ASSET"
SUM_URL="$BASE/$CHECKSUM_NAME"
echo "  ${C_DIM}releases: $BASE${C_RESET}"

# --- download ----------------------------------------------------------------------
step "downloading bundle ($LABEL)"
TMP_ZIP="$TMP/$ASSET"
TMP_SUM="$TMP/$CHECKSUM_NAME"
download "$SUM_URL" "$TMP_SUM" || die "could not fetch the published checksum ($SUM_URL)"

EXPECTED=$(awk 'NF>=2{v=$1} END{print tolower(v)}' "$TMP_SUM")
case "$EXPECTED" in
  ''|*[!0-9a-f]*) die "server returned an unexpected checksum: '$EXPECTED'" ;;
esac
[ ${#EXPECTED} -eq 64 ] || die "published checksum is not 64 hex chars"

download "$ZIP_URL" "$TMP_ZIP" || die "download failed ($ZIP_URL)"

# --- verify ------------------------------------------------------------------------
step "verifying checksum"
ACTUAL=$(sha256sum "$TMP_ZIP" | awk '{print tolower($1)}')
if [ "$ACTUAL" != "$EXPECTED" ]; then
  die "checksum mismatch — expected $EXPECTED, got $ACTUAL. Refusing to install."
fi
ok "sha256 $ACTUAL"

# --- extract -----------------------------------------------------------------------
DEST_ABSPATH=$(cd "$(dirname -- "$DEST")" && pwd)/$(basename -- "$DEST")
step "extracting to $DEST_ABSPATH"

unzip -q "$TMP_ZIP" -d "$TMP/stage"
if [ ! -f "$TMP/stage/spark.sh" ] || [ ! -x "$TMP/stage/spark-serve" ] || [ ! -x "$TMP/stage/spark-collect" ]; then
  die "bundle looks malformed — spark.sh / spark-serve / spark-collect not all present"
fi
chmod 0755 -- "$TMP/stage/spark.sh" "$TMP/stage/spark-serve" "$TMP/stage/spark-collect"
mkdir -p -- "$(dirname -- "$DEST_ABSPATH")"
rm -rf -- "$DEST_ABSPATH"
mv -- "$TMP/stage" "$DEST_ABSPATH"
ok "unpacked $(bytes_human "$(file_size "$TMP_ZIP")") to $DEST"

if [ -n "$PORT" ]; then
  echo "PORT_DEFAULT=$PORT" > "$DEST/.install-port"
  ok "remembered preferred port: $PORT"
else
  ok "installed"
fi

# --- done ---------------------------------------------------------------------------
DEST_ABS=$(cd "$DEST" && pwd)
say ""
say "  ${C_GREEN}${C_BOLD}✓ Spark Dashboard is ready${C_RESET}"
say ""
if [ -n "$PORT" ]; then
  say "  ${C_CYAN}→${C_RESET}  $DEST_ABS/spark.sh start          ${C_DIM}# dashboard on :$PORT${C_RESET}"
  say "  ${C_CYAN}→${C_RESET}  $DEST_ABS/spark.sh start 8090    ${C_DIM}# …or any other port${C_RESET}"
else
  say "  ${C_CYAN}→${C_RESET}  $DEST_ABS/spark.sh start          ${C_DIM}# dashboard on :8090${C_RESET}"
  say "  ${C_CYAN}→${C_RESET}  $DEST_ABS/spark.sh start 11020   ${C_DIM}# …or any other port${C_RESET}"
fi
say "  ${C_CYAN}→${C_RESET}  $DEST_ABS/spark.sh status        ${C_DIM}# check it is running${C_RESET}"
say "  ${C_CYAN}→${C_RESET}  $DEST_ABS/spark.sh stop         ${C_DIM}# stop it${C_RESET}"
say ""
say "  ${C_DIM}history database: ${HOME}/.local/share/spark-dashboard (kept across reinstalls)"
say "  ${C_DIM}repo: https://github.com/$REPO"
say "  ${C_DIM}uninstall: re-run this installer with --uninstall${C_RESET}"
