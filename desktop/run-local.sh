#!/bin/bash
#
# run-local.sh — Fully offline local Omi (no Firebase, no cloud, no account)
#
# Prerequisites:
#   1. Ollama running locally (https://ollama.com) — "brew install ollama" or download
#   2. A model downloaded in Ollama, e.g.:  ollama pull llama3
#
# What it starts:
#   - Rust backend on port 10201 (LOCAL_MODE=1)
#   - Python backend on port 8080 (LOCAL_MODE=1)
#   - Pre-downloads faster-whisper small model on first run (~500MB)
#
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# ─── Colours ───────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
BLUE='\033[0;34m'; CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${BLUE}[local]${RESET}  $*"; }
ok()      { echo -e "${GREEN}[local]${RESET}  $*"; }
warn()    { echo -e "${YELLOW}[local]${RESET}  $*"; }
fail()    { echo -e "${RED}[local]${RESET}  $*" >&2; }
bold()    { echo -e "${BOLD}$*${RESET}"; }

# ─── Check Ollama ──────────────────────────────────────────────────────────
check_ollama() {
    if curl -s http://localhost:11434/api/tags > /dev/null 2>&1; then
        OLLAMA_RUNNING=true
    else
        OLLAMA_RUNNING=false
    fi
}

# ─── Check / pull faster-whisper model ────────────────────────────────────
check_whisper() {
    info "Checking faster-whisper model (first run ~500MB)..."
    python3 - <<'EOF'
import sys
try:
    from faster_whisper import WhisperModel
    model = WhisperModel("small", device="cpu", compute_type="int8")
    print("ok")
except ImportError:
    print("missing")
    sys.exit(1)
except Exception as e:
    print(f"error:{e}")
    sys.exit(1)
EOF
}

# ─── Build env file for local mode ────────────────────────────────────────
write_local_env() {
    local env_file="$SCRIPT_DIR/Backend-Rust/.env.local"
    info "Writing local env: $env_file"
    cat > "$env_file" <<'ENVEOF'
# Local mode — fully offline, no Firebase, no cloud
LOCAL_MODE=1
BIND_LOCALHOST=1

# Rust backend port
PORT=10201

# Local LLM (Ollama default)
LOCAL_LLM_BASE_URL=http://localhost:11434/v1
LOCAL_LLM_MODEL=llama3
LOCAL_LLM_API_KEY=sk-dummy

# Local storage paths
LOCAL_DB_PATH=./omi_local.db

# Python backend (runs locally too)
PYTHON_BACKEND_PORT=8080
ENVEOF
    ok "Local .env written: $env_file"
    echo "  To customise model, edit: LOCAL_LLM_MODEL=llama3"
    echo ""
}

# ─── Start Rust backend ────────────────────────────────────────────────────
start_rust_backend() {
    local env_file="$SCRIPT_DIR/Backend-Rust/.env.local"
    (
        # Load the local env and start the Rust backend
        set -e
        cd "$SCRIPT_DIR/Backend-Rust"
        # Merge with existing .env if present, .env.local takes precedence
        export $(grep -v '^#' "$env_file" | xargs) 2>/dev/null || true
        if [ -f ".env" ]; then
            set -a && source <(grep -v '^#' .env | grep -v "^LOCAL_" | xargs) 2>/dev/null; set +a
        fi
        export $(grep -v '^#' "$env_file" | xargs) 2>/dev/null
        echo "$LOCAL_LLM_BASE_URL" | grep -q "ollama" && OLLAMA_MODEL="$LOCAL_LLM_MODEL" || OLLAMA_MODEL=""
        echo "LOCAL_MODE=1 PORT=10201 cargo run" | sed 's/.*/  /' >&2
        exec env LOCAL_MODE=1 BIND_LOCALHOST=1 PORT=10201 \
            LOCAL_LLM_BASE_URL="${LOCAL_LLM_BASE_URL:-http://localhost:11434/v1}" \
            LOCAL_LLM_MODEL="${LOCAL_LLM_MODEL:-llama3}" \
            LOCAL_LLM_API_KEY="${LOCAL_LLM_API_KEY:-sk-dummy}" \
            LOCAL_DB_PATH="${LOCAL_DB_PATH:-./omi_local.db}" \
            cargo run
    ) &
    RUST_PID=$!
    echo $RUST_PID > /tmp/omi-local-rust.pid
    ok "Rust backend started (PID $RUST_PID, port 10201)"
}

# ─── Start Python backend ──────────────────────────────────────────────────
start_python_backend() {
    (
        set -e
        cd "$SCRIPT_DIR"
        export LOCAL_MODE=1
        # Use the existing run.sh but in local-skip mode — it will start
        # Python backend without auth and without tunnel
        exec python3 -m uvicorn backend.main:app \
            --host 127.0.0.1 --port 8080 \
            --ws-ping-interval 30 \
            --ws-ping-timeout 120 2>&1 | sed 's/^/[python] /'
    ) &
    PYTHON_PID=$!
    echo $PYTHON_PID > /tmp/omi-local-python.pid
    ok "Python backend started (PID $PYTHON_PID, port 8080)"
}

# ─── Wait for services ─────────────────────────────────────────────────────
wait_for_services() {
    local max_wait=30
    local count=0
    info "Waiting for services to come up..."
    while [ $count -lt $max_wait ]; do
        if curl -s http://127.0.0.1:10201/health > /dev/null 2>&1; then
            ok "Rust backend ready at http://127.0.0.1:10201"
            break
        fi
        sleep 1
        count=$((count + 1))
    done
    if [ $count -ge $max_wait ]; then
        warn "Rust backend not responding yet — check logs"
    fi

    count=0
    while [ $count -lt $max_wait ]; do
        if curl -s http://127.0.0.1:8080/health > /dev/null 2>&1; then
            ok "Python backend ready at http://127.0.0.1:8080"
            break
        fi
        sleep 1
        count=$((count + 1))
    done
    if [ $count -ge $max_wait ]; then
        warn "Python backend not responding yet — check logs"
    fi
}

# ─── Print next steps ───────────────────────────────────────────────────────
next_steps() {
    echo ""
    bold "═══════════════════════════════════════════════════"
    bold "  Local Omi is running"
    bold "═══════════════════════════════════════════════════"
    echo ""
    echo -e "  ${CYAN}Rust backend${RESET}   http://127.0.0.1:10201"
    echo -e "  ${CYAN}Python backend${RESET}  http://127.0.0.1:8080"
    echo -e "  ${CYAN}Ollama${RESET}          http://localhost:11434"
    echo ""
    echo -e "  ${GREEN}Smoke test:${RESET}"
    echo -e "    curl http://127.0.0.1:10201/v4/local/status"
    echo ""
    echo -e "  ${YELLOW}To stop:${RESET}"
    echo -e "    ./run-local.sh --stop"
    echo ""
    bold "═══════════════════════════════════════════════════"
    echo ""
}

# ─── Stop services ─────────────────────────────────────────────────────────
stop_services() {
    info "Stopping local Omi services..."
    for pidfile in /tmp/omi-local-rust.pid /tmp/omi-local-python.pid; do
        if [ -f "$pidfile" ]; then
            pid=$(cat "$pidfile")
            if kill -0 "$pid" 2>/dev/null; then
                kill "$pid" && ok "Stopped PID $pid" || warn "PID $pid already gone"
            fi
            rm -f "$pidfile"
        fi
    done
    # Also kill any stray cargo/uvicorn processes on our ports
    pkill -f "cargo run.*Backend-Rust" 2>/dev/null || true
    pkill -f "uvicorn backend.main.*8080" 2>/dev/null || true
    ok "All services stopped"
}

# ─── Main ──────────────────────────────────────────────────────────────────
main() {
    echo ""
    bold "═══════════════════════════════════════════════════"
    bold "  Local Omi — fully offline, no cloud, no account"
    bold "═══════════════════════════════════════════════════"
    echo ""

    # Check Ollama
    check_ollama
    if [ "$OLLAMA_RUNNING" = false ]; then
        warn "Ollama is not running."
        echo "  Start it with: brew services start ollama"
        echo "  Or download from: https://ollama.com"
        echo "  Then pull a model:  ollama pull llama3"
        echo ""
        if [ "$1" = "--setup" ]; then
            echo "  Attempting to start Ollama..."
            brew services start ollama 2>/dev/null || ollama serve 2>/dev/null &
            sleep 3
            check_ollama
            if [ "$OLLAMA_RUNNING" = false ]; then
                fail "Could not start Ollama. Please start it manually."
                exit 1
            fi
            ok "Ollama is now running"
        else
            exit 1
        fi
    fi

    # Show Ollama models
    info "Ollama is running. Available models:"
    curl -s http://localhost:11434/api/tags | python3 -c \
        "import sys,json; [print(f'    - {m[\"name\"]}') for m in json.load(sys.stdin).get('models',[])]" 2>/dev/null \
        || echo "    (could not list models)"
    echo ""

    # Check faster-whisper
    if ! python3 -c "from faster_whisper import WhisperModel" 2>/dev/null; then
        warn "faster-whisper not installed. Installing..."
        pip3 install faster-whisper==1.0.8 --quiet 2>&1 | tail -3 || true
    fi

    local whisper_check=$(check_whisper 2>&1)
    if [ "$whisper_check" = "missing" ]; then
        info "Downloading faster-whisper small model (~500MB, one-time)..."
        python3 -c "from faster_whisper import WhisperModel; WhisperModel('small', device='cpu', compute_type='int8')" 2>&1 | \
            sed 's/^/    /'
        ok "Whisper model ready"
    elif echo "$whisper_check" | grep -q "^error:"; then
        warn "Whisper check failed: $whisper_check"
    else
        ok "Whisper model already cached"
    fi
    echo ""

    # Write local env
    write_local_env

    # Stop any existing services
    stop_services > /dev/null 2>&1 || true

    # Start backends
    info "Starting Rust backend..."
    start_rust_backend
    info "Starting Python backend..."
    start_python_backend

    # Wait
    wait_for_services

    # Next steps
    next_steps

    # Stay attached to backends
    info "Press Ctrl+C to stop all services"
    echo ""
    wait
}

# ─── Entry point ───────────────────────────────────────────────────────────
case "${1:-}" in
    --stop|-s)  stop_services ;;
    --help|-h)  echo "Usage: ./run-local.sh [--stop]"
                echo "       ./run-local.sh --setup   (first run: install deps + start Ollama)"
                echo "       ./run-local.sh           (normal start)" ;;
    *)          main "$@" ;;
esac
