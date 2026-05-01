#!/bin/bash
set -e

# ═══════════════════════════════════════════════════════════════════════
# run-all-local.sh — Start everything for fully local Omi desktop
#
# What it starts (all LOCAL_MODE=1, no cloud):
#   1. Ollama (if not running)
#   2. Python backend on port 8080
#   3. Rust backend on port 10201
#   4. Desktop app (build + launch)
#
# Prerequisites:
#   - Ollama with a model pulled (e.g. ollama pull llama3)
#   - Python 3.11+ with faster-whisper installed
#   - Rust toolchain (cargo)
#   - Xcode command-line tools (xcrun)
#   - codesign identity (any Apple Development cert)
#
# Usage:
#   ./run-all-local.sh              # Full start (backends + build + app)
#   ./run-all-local.sh --backends   # Only start backends, skip app build
#   ./run-all-local.sh --app        # Only build + launch app (backends already running)
#   ./run-all-local.sh --stop       # Stop everything
#   ./run-all-local.sh --status     # Check what's running
# ═══════════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ─── Colours ───────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${BLUE}[local]${RESET}  $*"; }
ok()      { echo -e "${GREEN}[local]${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[local]${RESET}  $*"; }
fail()    { echo -e "${RED}[local]${RESET}  $*" >&2; }
bold()    { echo -e "${BOLD}$*${RESET}"; }
step()    { echo -e "${CYAN}[local]${RESET}  ▶ $*"; }

# ─── Config ────────────────────────────────────────────────────────────
PYTHON_PORT=8080
RUST_PORT=10201
BINARY_NAME="Omi Computer"
APP_NAME="Omi Dev"
BUNDLE_ID="com.omi.desktop-dev"
BUILD_DIR="build"
APP_BUNDLE="$BUILD_DIR/$APP_NAME.app"
APP_PATH="/Applications/$APP_NAME.app"
PID_DIR="/tmp/omi-local"

mkdir -p "$PID_DIR"

# ─── Env file ─────────────────────────────────────────────────────────
ensure_env() {
    local env_file="$SCRIPT_DIR/Backend-Rust/.env"
    if [ ! -f "$env_file" ]; then
        step "Writing .env for LOCAL_MODE=1"
        cat > "$env_file" <<'ENVEOF'
# Auto-generated for LOCAL_MODE=1 — fully offline, no cloud
LOCAL_MODE=1
BIND_LOCALHOST=1
PORT=10201

# Local LLM (Ollama)
LOCAL_LLM_BASE_URL=http://localhost:11434/v1
LOCAL_LLM_MODEL=llama3
LOCAL_LLM_API_KEY=sk-dummy

# Local storage
LOCAL_DB_PATH=./omi_local.db

# Local encryption (32+ bytes; matches backend test fixtures)
ENCRYPTION_SECRET=omi_ZwB2ZNqB2HHpMK6wStk7sTpavJiPTFg7gXUHnc4tFABPU6pZ2c2DKgehtfgi4RZv

# Dummy OpenAI key so import-time client construction doesn't fail in LOCAL_MODE
OPENAI_API_KEY=sk-dummy

# Dummy Typesense settings so import-time client construction doesn't fail
TYPESENSE_HOST=localhost
TYPESENSE_HOST_PORT=8108
TYPESENSE_API_KEY=typesense-dummy
ENVEOF
        ok "Created $env_file"
    fi
}

# ─── Checks ───────────────────────────────────────────────────────────
check_ollama() {
    if curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
        return 0
    fi
    return 1
}

check_port() {
    lsof -i ":$1" -sTCP:LISTEN > /dev/null 2>&1
}

wait_for_port() {
    local port=$1 name=$2 max=${3:-30} count=0
    while [ $count -lt $max ]; do
        if check_port "$port"; then
            ok "$name ready on port $port"
            return 0
        fi
        sleep 1
        count=$((count + 1))
    done
    warn "$name not responding on port $port after ${max}s"
    return 1
}

python_venv_needs_rebuild() {
    local venv="$1"
    if [ ! -x "$venv/bin/python" ]; then
        return 0
    fi

    if ! "$venv/bin/python" - <<'PY'
import importlib.metadata as m
import sys

try:
    lco = m.version("langchain-openai")
    lcc = m.version("langchain-core")
    openai = m.version("openai")
except Exception:
    sys.exit(1)

if lco != "0.3.18":
    sys.exit(2)
if not lcc.startswith("0.3."):
    sys.exit(3)
if int(openai.split(".", 1)[0]) >= 2:
    sys.exit(4)
PY
    then
        return 0
    fi

    return 1
}

# ─── Start Ollama ────────────────────────────────────────────────────
start_ollama() {
    if check_ollama; then
        ok "Ollama is running"
        return 0
    fi

    step "Starting Ollama..."
    if command -v ollama > /dev/null 2>&1; then
        ollama serve > /dev/null 2>&1 &
        sleep 2
        if check_ollama; then
            ok "Ollama started"
            return 0
        fi
    fi

    fail "Ollama is not running and could not be started."
    echo "  Install: brew install ollama"
    echo "  Then:    ollama pull llama3"
    return 1
}

# ─── Start Python backend ─────────────────────────────────────────────
start_python() {
    if check_port $PYTHON_PORT; then
        info "Python backend already running on port $PYTHON_PORT"
        return 0
    fi

    # Find suitable Python (3.11+ preferred)
    local PY=""
    for candidate in python3.12 python3.11 python3; do
        if command -v "$candidate" > /dev/null 2>&1; then
            PY="$candidate"
            break
        fi
    done
    if [ -z "$PY" ]; then
        fail "No python3 found. Install Python 3.11+"
        return 1
    fi

    local BE="$SCRIPT_DIR/../backend"
    local VENV="$BE/.venv"

    # Rebuild venv if missing or if incompatible packages were installed previously.
    if [ -d "$VENV" ] && python_venv_needs_rebuild "$VENV"; then
        warn "Recreating Python virtual environment to fix dependency mismatch..."
        rm -rf "$VENV"
    fi

    # Create venv if missing
    if [ ! -d "$VENV" ]; then
        step "Creating Python virtual environment..."
        "$PY" -m venv "$VENV"
        "$VENV/bin/pip" install -q --upgrade pip
    fi

    # Install deps if stale
    if [ ! -f "$VENV/.deps_installed" ] || \
       [ "$BE/requirements.txt" -nt "$VENV/.deps_installed" ]; then
        step "Installing Python dependencies..."
        "$VENV/bin/pip" install -q -r "$BE/requirements.txt" 2>&1 | tail -3
        touch "$VENV/.deps_installed"
    fi

    step "Starting Python backend on port $PYTHON_PORT..."
    (
        cd "$BE"
        export LOCAL_MODE=1
        export LOCAL_LLM_BASE_URL="${LOCAL_LLM_BASE_URL:-http://localhost:11434/v1}"
        export LOCAL_LLM_MODEL="${LOCAL_LLM_MODEL:-llama3}"
        export LOCAL_LLM_API_KEY="${LOCAL_LLM_API_KEY:-sk-dummy}"
        export ENCRYPTION_SECRET="${ENCRYPTION_SECRET:-omi_ZwB2ZNqB2HHpMK6wStk7sTpavJiPTFg7gXUHnc4tFABPU6pZ2c2DKgehtfgi4RZv}"
        export OPENAI_API_KEY="${OPENAI_API_KEY:-sk-dummy}"
        export TYPESENSE_HOST="${TYPESENSE_HOST:-localhost}"
        export TYPESENSE_HOST_PORT="${TYPESENSE_HOST_PORT:-8108}"
        export TYPESENSE_API_KEY="${TYPESENSE_API_KEY:-typesense-dummy}"
        exec "$VENV/bin/python" -m uvicorn main:app \
            --host 127.0.0.1 --port $PYTHON_PORT \
            --ws-ping-interval 30 \
            --ws-ping-timeout 120 2>&1 | sed 's/^/[python] /'
    ) &
    local pid=$!
    echo $pid > "$PID_DIR/python.pid"
    info "Python backend PID: $pid"
    wait_for_port $PYTHON_PORT "Python backend"
}

# ─── Start Rust backend ──────────────────────────────────────────────
start_rust() {
    if check_port $RUST_PORT; then
        info "Rust backend already running on port $RUST_PORT"
        return 0
    fi

    step "Starting Rust backend on port $RUST_PORT..."
    (
        cd "$SCRIPT_DIR/Backend-Rust"
        # Load .env if present
        if [ -f ".env" ]; then
            set -a && source <(grep -v '^#' .env | grep -v '^$') 2>/dev/null; set +a
        fi
        exec env \
            LOCAL_MODE=1 \
            BIND_LOCALHOST=1 \
            PORT=$RUST_PORT \
            LOCAL_LLM_BASE_URL="${LOCAL_LLM_BASE_URL:-http://localhost:11434/v1}" \
            LOCAL_LLM_MODEL="${LOCAL_LLM_MODEL:-llama3}" \
            LOCAL_LLM_API_KEY="${LOCAL_LLM_API_KEY:-sk-dummy}" \
            LOCAL_DB_PATH="${LOCAL_DB_PATH:-./omi_local.db}" \
            cargo run 2>&1 | sed 's/^/[rust]   /'
    ) &
    local pid=$!
    echo $pid > "$PID_DIR/rust.pid"
    info "Rust backend PID: $pid"
    wait_for_port $RUST_PORT "Rust backend" 60
}

# ─── Build desktop app ───────────────────────────────────────────────
build_app() {
    step "Building desktop app..."

    # Kill existing dev app
    pkill -f "$APP_NAME.app" 2>/dev/null || true
    sleep 0.5

    # Build Swift
    step "  swift build -c debug..."
    xcrun swift build -c debug --package-path Desktop 2>&1 | tail -3

    # Create app bundle
    step "  Creating app bundle..."
    mkdir -p "$APP_BUNDLE/Contents/MacOS"
    mkdir -p "$APP_BUNDLE/Contents/Resources"
    mkdir -p "$APP_BUNDLE/Contents/Frameworks"

    cp -f "Desktop/.build/debug/$BINARY_NAME" "$APP_BUNDLE/Contents/MacOS/$BINARY_NAME"
    install_name_tool -add_rpath "@executable_path/../Frameworks" "$APP_BUNDLE/Contents/MacOS/$BINARY_NAME" 2>/dev/null || true

    # Copy Sparkle framework if present
    local sparkle="Desktop/.build/arm64-apple-macosx/debug/Sparkle.framework"
    if [ -d "$sparkle" ]; then
        cp -Rf "$sparkle" "$APP_BUNDLE/Contents/Frameworks/"
    fi

    # Copy Info.plist
    if [ -f "Desktop/Desktop/Info.plist" ]; then
        cp -f "Desktop/Desktop/Info.plist" "$APP_BUNDLE/Contents/Info.plist"
    fi

    # Copy app icon
    if [ -f "desktop/omi_icon.icns" ]; then
        cp -f "desktop/omi_icon.icns" "$APP_BUNDLE/Contents/Resources/OmiIcon.icns" 2>/dev/null || true
    fi

    # Write .env for the app
    cat > "$APP_BUNDLE/Contents/Resources/.env" <<ENVEOF
LOCAL_MODE=1
OMI_API_URL=http://localhost:$RUST_PORT
OMI_PYTHON_API_URL=http://localhost:$PYTHON_PORT
OMI_AUTH_URL=http://localhost:$PYTHON_PORT
ENVEOF

    # Sign
    step "  Signing app bundle..."
    local identity
    identity=$(security find-identity -v -p codesigning | grep "Apple Development" | head -1 | sed 's/.*"\(.*\)"/\1/')
    if [ -n "$identity" ]; then
        codesign --force --options runtime --sign "$identity" "$APP_BUNDLE" 2>/dev/null || true
    else
        warn "No signing identity — app may have permission issues"
    fi

    # Install
    step "  Installing to $APP_PATH..."
    ditto "$APP_BUNDLE" "$APP_PATH" 2>/dev/null || true

    ok "Desktop app built"
}

# ─── Launch desktop app ───────────────────────────────────────────────
launch_app() {
    step "Launching desktop app with LOCAL_MODE=1..."

    # Launch with LOCAL_MODE set in both ProcessInfo and .env
    (
        export LOCAL_MODE=1
        open "$APP_PATH" --args --LOCAL_MODE=1 2>/dev/null || \
            "$APP_PATH/Contents/MacOS/$BINARY_NAME" --LOCAL_MODE=1 &
    )

    ok "Desktop app launched"
}

# ─── Stop everything ──────────────────────────────────────────────────
stop_all() {
    info "Stopping all local services..."

    # Stop desktop app
    pkill -f "$APP_NAME.app" 2>/dev/null && ok "Stopped desktop app" || true

    # Stop backends by PID
    for name in python rust; do
        local pidfile="$PID_DIR/$name.pid"
        if [ -f "$pidfile" ]; then
            local pid=$(cat "$pidfile")
            if kill -0 "$pid" 2>/dev/null; then
                kill "$pid" 2>/dev/null && ok "Stopped $name backend (PID $pid)" || true
            fi
            rm -f "$pidfile"
        fi
    done

    # Kill stray processes
    pkill -f "uvicorn.*main:app.*8080" 2>/dev/null || true
    pkill -f "cargo run.*Backend-Rust" 2>/dev/null || true

    ok "All stopped"
}

# ─── Status ──────────────────────────────────────────────────────────
show_status() {
    echo ""
    bold "Local Omi Status"
    echo "─────────────────────"

    # Ollama
    if check_ollama; then
        ok "Ollama: running"
    else
        fail "Ollama: not running"
    fi

    # Python backend
    if check_port $PYTHON_PORT; then
        ok "Python backend: running on :$PYTHON_PORT"
    else
        fail "Python backend: not running"
    fi

    # Rust backend
    if check_port $RUST_PORT; then
        ok "Rust backend: running on :$RUST_PORT"
    else
        fail "Rust backend: not running"
    fi

    # Desktop app
    if pgrep -f "$APP_NAME.app" > /dev/null 2>&1; then
        ok "Desktop app: running"
    else
        fail "Desktop app: not running"
    fi

    echo ""
}

# ─── Summary ─────────────────────────────────────────────────────────
print_summary() {
    echo ""
    bold "═══════════════════════════════════════════════════"
    bold "  Local Omi — fully offline"
    bold "═══════════════════════════════════════════════════"
    echo ""
    echo -e "  ${CYAN}Python backend${RESET}  http://localhost:$PYTHON_PORT"
    echo -e "  ${CYAN}Rust backend${RESET}    http://localhost:$RUST_PORT"
    echo -e "  ${CYAN}Ollama${RESET}          http://localhost:11434"
    echo -e "  ${CYAN}Desktop app${RESET}     $APP_PATH"
    echo ""
    echo -e "  ${GREEN}Smoke tests:${RESET}"
    echo -e "    curl http://localhost:$PYTHON_PORT/docs"
    echo -e "    curl http://localhost:$RUST_PORT/health"
    echo ""
    echo -e "  ${YELLOW}To stop:${RESET}  ./run-all-local.sh --stop"
    echo -e "  ${YELLOW}Status:${RESET}   ./run-all-local.sh --status"
    echo ""
    bold "═══════════════════════════════════════════════════"
    echo ""
}

# ─── Main ────────────────────────────────────────────────────────────
main() {
    echo ""
    bold "═══════════════════════════════════════════════════"
    bold "  Local Omi — fully offline, no cloud"
    bold "═══════════════════════════════════════════════════"
    echo ""

    # 0. Ensure env
    ensure_env

    # 1. Ollama
    start_ollama || exit 1

    # 2. Python backend
    start_python

    # 3. Rust backend
    start_rust

    # 4. Build + launch app (unless --backends)
    if [ "${SKIP_APP:-0}" != "1" ]; then
        build_app
        launch_app
    fi

    # 5. Summary
    print_summary

    # Stay alive if only running backends
    if [ "${SKIP_APP:-0}" = "1" ]; then
        info "Backends running. Press Ctrl+C to stop."
        wait
    fi
}

# ─── Entry point ─────────────────────────────────────────────────────
case "${1:-}" in
    --stop|-s)     stop_all ;;
    --status)      show_status ;;
    --backends|-b) SKIP_APP=1 main ;;
    --app|-a)      build_app && launch_app && print_summary ;;
    --help|-h)
        echo "Usage: ./run-all-local.sh [option]"
        echo ""
        echo "  (no flag)   Start everything: Ollama + backends + build + app"
        echo "  --backends  Only start Ollama + Python + Rust backends"
        echo "  --app       Only build and launch the desktop app"
        echo "  --stop      Stop all services"
        echo "  --status    Show what's running"
        echo "  --help      This help"
        ;;
    *)             main ;;
esac
