#!/usr/bin/env bash
#
# make-bundle.sh — build and publish a Spark Dashboard release bundle
#
#   tools/make-bundle.sh [--version v1.0.0] [--release [body...]] [--skip-build] [--no-publish]
#
# Steps:
#   1. Build the backend (release) and frontend
#   2. Assemble a self-contained bundle (binaries + static/ + default.toml + spark.sh)
#   3. Write SHA256SUMS and zip it -> dist/spark-dashboard-aarch64-linux.zip
#   4. Publish to GitHub Releases (via `gh` if available, otherwise prints
#      the exact curl steps to finish by hand)
#
set -euo pipefail

REPO_OWNER="walkthegrowth-ctrl"
REPO_NAME="spark-dashboard"
ASSET="spark-dashboard-aarch64-linux.zip"
SUMS_NAME="${ASSET}.sha256"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BACKEND="$ROOT/backend"
FRONTEND="$ROOT/frontend"
DIST="$ROOT/dist"

VERSION=""
RELEASE_BODY=""
RELEASE=0
SKIP_BUILD=0
NO_PUBLISH=0

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version)    VERSION="${2:?--version needs a value}"; shift 2 ;;
    --release)    RELEASE=1; shift; [ $# -gt 0 ] && [[ "${1:-}" == --* ]] || { RELEASE_BODY="$1"; shift; } ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    --no-publish) NO_PUBLISH=1; shift ;;
    -h|--help)    usage ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

[ "$NO_PUBLISH" -eq 1 ] && RELEASE=0 && RELEASE_BODY=""

if [ "$RELEASE" -eq 1 ] && [ -z "$VERSION" ]; then
  echo "error: --release requires --version (e.g. --version v1.0.0)" >&2
  exit 2
fi

if [ "$(uname -m)" != "aarch64" ]; then
  echo "error: bundle must be built on aarch64 (running: $(uname -m))" >&2
  exit 1
fi

C_GREEN=$'\033[32m'; C_CYAN=$'\033[36m'; C_RESET=$'\033[0m'
step() { printf '\n%s▸ %s%s' "$C_CYAN" "$*" "$C_RESET"; }
ok()   { printf '  %s✓ %s%s\n' "$C_GREEN" "$*" "$C_RESET"; }

# --- platform guards -----------------------------------------------------------------
step "checking toolchain"
for t in cargo node npm unzip sha256sum; do
  command -v "$t" >/dev/null 2>&1 || { echo "error: $t not found" >&2; exit 1; }
done
ok "cargo · node · npm · unzip · sha256sum"

# --- build -----------------------------------------------------------------------------
if [ "$SKIP_BUILD" -eq 0 ]; then
  step "building backend (release)"
  ( cd "$BACKEND" && cargo build --release )
  ok "$(du -h "$BACKEND/target/release/spark-collect" | awk '{print $1, "spark-collect"}')  /  $(du -h "$BACKEND/target/release/spark-serve" | awk '{print $1, "spark-serve"}')"

  step "building frontend"
  ( cd "$FRONTEND" && npm run build )
  ok "frontend -> $BACKEND/static"
else
  step "skipping build (--skip-build)"
fi

for f in "$BACKEND/target/release/spark-collect" "$BACKEND/target/release/spark-serve" "$BACKEND/static/index.html"; do
  [ -e "$f" ] || { echo "error: expected build output missing: $f — run without --skip-build" >&2; exit 1; }
done

# --- assemble ----------------------------------------------------------------------------
STAGE="$DIST/stage"
step "assembling bundle"
rm -rf -- "$STAGE"
mkdir -p -- "$STAGE"
cp -f "$BACKEND/target/release/spark-collect" "$STAGE/"
cp -f "$BACKEND/target/release/spark-serve"   "$STAGE/"
cp -rf "$BACKEND/static"                      "$STAGE/static"
cp -f "$BACKEND/config/default.toml"          "$STAGE/default.toml"
cp -f "$ROOT/spark.sh"                        "$STAGE/spark.sh"
cp -f "$ROOT/LICENSE"                          "$STAGE/LICENSE"
chmod 0755 -- "$STAGE/spark.sh" "$STAGE/spark-collect" "$STAGE/spark-serve"

# sanity: the bundle must self-verify its own layout
[ -x "$STAGE/spark-serve" ] && [ -d "$STAGE/static" ] && [ -f "$STAGE/static/index.html" ] && [ -f "$STAGE/default.toml" ]
ok "stage: $(du -sh "$STAGE" | awk '{print $1}') ($(find "$STAGE" -type f | wc -l) files)"

# --- checksum + zip -----------------------------------------------------------------------
step "checksums + zip"
( cd "$STAGE" && sha256sum spark-collect spark-serve default.toml spark.sh > "$STAGE/SHA256SUMS" )
cp -f "$STAGE/SHA256SUMS" "$DIST/$SUMS_NAME"
# the published checksum file only needs the zip's own hash; recompute against the zip below
rm -f -- "$DIST/$ASSET"
( cd "$STAGE" && zip -q -r -X "../$ASSET" . )
ZIP_SIZE="$(du -h "$DIST/$ASSET" | awk '{print $1}')"
ok "built $DIST/$ASSET ($ZIP_SIZE)"

step "verifying the zip end-to-end"
VERIFY_DIR="$DIST/.verify"
rm -rf -- "$VERIFY_DIR"; mkdir -p -- "$VERIFY_DIR"
unzip -q "$DIST/$ASSET" -d "$VERIFY_DIR"
( cd "$VERIFY_DIR" && sha256sum -c SHA256SUMS --quiet )
[ -x "$VERIFY_DIR/spark-serve" ] && [ -f "$VERIFY_DIR/static/index.html" ]
ok "zip unpacks and SHA256SUMS pass"

# publish checksum (hash of the zip itself) for install.sh
ZIP_SHA="$(sha256sum "$DIST/$ASSET" | awk '{print $1}')"
printf '%s  %s\n' "$ZIP_SHA" "$ASSET" > "$DIST/$SUMS_NAME"
ok "published $DIST/$SUMS_NAME"

# --- publish -----------------------------------------------------------------------------
if [ $RELEASE -eq 0 ]; then
  step "publishing skipped"
  if [ -n "${GITHUB_TOKEN:-${GH_TOKEN:-}}" ]; then
    ok "dist ready — token detected: add --release to also create the GitHub release"
  else
    echo "  dist ready. To publish:  $0 --release --version <tag>"
    echo "  (needs GH_TOKEN / GITHUB_TOKEN in the environment)"
  fi
  exit 0
fi

TOKEN="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
if [ -z "$TOKEN" ]; then
  echo "error: --release needs GH_TOKEN (or GITHUB_TOKEN) in the environment" >&2
  echo "  files for a manual upload at https://github.com/$REPO_OWNER/$REPO_NAME/releases/new :"
  echo "    $DIST/$ASSET"
  echo "    $DIST/$SUMS_NAME"
  echo "    $ROOT/install.sh"
  exit 1
fi

step "publishing release to GitHub ($REPO_OWNER/$REPO_NAME)"
TAG="${VERSION#v}"; TAG="v$TAG"
API="https://api.github.com"
GH_AUTH="Authorization: Bearer ${TOKEN}"

gh_upload() { # $1 = release_id   $2 = file   $3 = asset name
  curl -fsSL -X POST \
    -H "$GH_AUTH" -H "Accept: application/vnd.github+json" \
    -H "Content-Type: application/octet-stream" \
    "https://uploads.github.com/repos/$REPO_OWNER/$REPO_NAME/releases/$1/assets?name=$3" \
    --data-binary "@$2" >/dev/null
}

DEFAULT_NOTES="Spark Dashboard ${TAG} — aarch64-linux.

## Install

    curl -fsSL https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/latest/download/install.sh | sh

Then start it:

    ~/.spark/spark.sh start

Stop:   ~/.spark/spark.sh stop
Status: ~/.spark/spark.sh status"

PAYLOAD=$(jq -n \
  --arg tag "$TAG" \
  --arg body "${RELEASE_BODY:-$DEFAULT_NOTES}" \
  '{tag_name:$tag, name:("Spark Dashboard "+$tag), body:$body, draft:false, prerelease:false}')

RELEASE_ID=$(curl -fsSL -X POST \
  -H "$GH_AUTH" -H "Accept: application/vnd.github+json" \
  -d "$PAYLOAD" \
  "$API/repos/$REPO_OWNER/$REPO_NAME/releases" | jq -r '.id')
case "$RELEASE_ID" in ''|null) echo "  ✗ failed to create the release" >&2; exit 1 ;; esac
ok "created release ${TAG} (id $RELEASE_ID)"

for f in "$DIST/$ASSET" "$DIST/$SUMS_NAME" "$ROOT/install.sh"; do
  step "uploading $(basename "$f")"
  gh_upload "$RELEASE_ID" "$f" "$(basename "$f")"
  ok "$(basename "$f")"
done

echo ""
if [ -t 1 ]; then
  C_GREEN=$'\033[32m'; C_RESET=$'\033[0m'; C_DIM=$'\033[2m'
  echo "  ${C_GREEN} ✓  Spark Dashboard $TAG is live${C_RESET}"
  echo "  ${C_DIM}    curl -fsSL https://github.com/$REPO_OWNER/$REPO_NAME/releases/latest/download/install.sh | sh${C_RESET}"
fi
