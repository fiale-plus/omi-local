# Airgap Audit Report

**Branch:** `local/airgap-cleanup`
**Date:** 2026-04-24

## Summary

The branch has been hardened to prevent LOCAL_MODE=1 from touching cloud services at import time or during startup. The key changes in this commit:

### Python backend fixes
- **`backend/main.py`** — restructured router imports: cloud-only routers (oauth, auth, transcribe, payment, mcp, developer, phone_calls, agent_tools, updates) are now gated behind an import try/except. In LOCAL_MODE, they are not imported or registered. Firebase initialization is fully conditional on `not LOCAL_MODE`.
- **`backend/dependencies.py`** — `firebase_admin` import is now gated. In LOCAL_MODE, `get_current_user_id()` returns a hardcoded dev user instead of calling Firebase Auth.
- **`backend/utils/other/endpoints.py`** — `firebase_admin` import is gated. In LOCAL_MODE, `verify_token()` and `get_user()` use local stubs. `redis` import is wrapped in try/except.

### Rust backend (already gated in prior commit)
- `desktop/Backend-Rust/src/main.rs` — Firestore is `Option<>`, skipped in LOCAL_MODE. Firebase Auth key fetch is skipped. Cloud proxy routes, webhooks, crisp, updates are disabled in LOCAL_MODE.

### Desktop app (already gated in prior commit)
- `desktop/Desktop/Sources/APIClient.swift` — no `api.omi.me` fallback; returns empty string if URL not set.
- `desktop/Desktop/Sources/TranscriptionService.swift` — no `api.omi.me` fallback.
- `desktop/run.sh` — LOCAL_MODE=1 now requires explicit `OMI_PYTHON_API_URL` and exits with an error if not set. No silent production fallback.

## Remaining cloud references

These are **intentionally left** because they are:
1. Only reachable in cloud mode (gated behind `not LOCAL_MODE`)
2. Comment/documentation only
3. In cloud-only services not used locally (pusher, agent-proxy, Auth-Python, migrations, scripts)

### Intentional cloud-only code (gated, not reachable in LOCAL_MODE)

| File | What | Why OK |
|---|---|---|
| `backend/routers/oauth.py` | `firebase_admin.auth` | Only imported in cloud mode |
| `backend/routers/auth.py` | `firebase_admin.auth` | Only imported in cloud mode |
| `backend/routers/transcribe.py` | `firebase_admin.auth` | Only imported in cloud mode |
| `backend/modal/job.py` | `firebase_admin` | Cloud-only cron job |
| `backend/agent-proxy/` | Firebase, GCE | Cloud-only proxy service |
| `backend/pusher/` | Firebase, Deepgram | Cloud-only pusher service |
| `desktop/Auth-Python/` | Firebase OAuth | Cloud-only auth broker |
| `desktop/Backend-Rust/src/routes/proxy.rs` | Gemini/Deepgram URLs | Disabled in LOCAL_MODE (routes not registered) |
| `desktop/Backend-Rust/src/routes/agent.rs` | GCE compute URLs | Cloud-only route |
| `desktop/Backend-Rust/src/routes/crisp.rs` | Crisp API | Disabled in LOCAL_MODE |
| `desktop/Backend-Rust/src/routes/webhooks.rs` | Sentry API | Disabled in LOCAL_MODE |
| `desktop/Backend-Rust/src/routes/stats.rs` | PostHog API | Cloud-only route |
| `desktop/Backend-Rust/src/routes/tts.rs` | ElevenLabs API | Cloud-only route |
| `desktop/Backend-Rust/src/routes/updates.rs` | GCS update feed | Cloud-only route |
| `desktop/Backend-Rust/src/llm/client.rs` | Gemini API | Only used when LOCAL_LLM_BASE_URL is NOT set |
| `desktop/Backend-Rust/src/services/firestore.rs` | Firestore API | Only constructed in cloud mode |

### Documentation/comments only (no runtime impact)

- `desktop/.env.example` — documents cloud URL formats as examples
- `desktop/run.sh` — help text and yolo-mode references
- `desktop/Desktop/Sources/APIClient.swift` — comment explaining original design
- `desktop/Desktop/Sources/TranscriptionService.swift` — error message text

### Scripts/migrations/tests (not part of local runtime)

- `backend/scripts/` — one-off cloud scripts
- `backend/migrations/` — Firestore data migrations
- `backend/tests/` — integration tests against cloud

## LOCAL_MODE=1 boot verification

The Python backend no longer requires any of the following to import and start:
- `firebase_admin`
- `stripe`
- `redis` (gracefully degrades)
- Any Google Cloud SDK

In LOCAL_MODE=1, the backend:
1. Skips Firebase initialization entirely
2. Does not import cloud-only routers (oauth, auth, transcribe, payment, mcp, developer)
3. Uses local stubs for auth verification (`get_current_user_id` returns "local-user")
4. Uses local stubs for token verification
5. Does not register cloud-only routes in the FastAPI app

## Definition of done checklist

- [x] `backend/main.py` — no unconditional `firebase_admin` import
- [x] `backend/dependencies.py` — gated `firebase_admin`
- [x] `backend/utils/other/endpoints.py` — gated `firebase_admin` and `redis`
- [x] `desktop/Desktop/Sources/APIClient.swift` — no `api.omi.me` fallback
- [x] `desktop/Desktop/Sources/TranscriptionService.swift` — no `api.omi.me` fallback
- [x] `desktop/run.sh` — LOCAL_MODE requires explicit URLs, no silent fallback
- [x] `desktop/Backend-Rust/src/main.rs` — Firestore/Firebase skipped in LOCAL_MODE
- [x] `desktop/Backend-Rust/src/main.rs` — cloud proxy routes disabled in LOCAL_MODE
- [x] `desktop/Backend-Rust/src/main.rs` — crisp/webhooks/updates disabled in LOCAL_MODE
- [x] `backend/database/chat.py` — import-safe with nop decorators in LOCAL_MODE
- [x] `backend/utils/subscription.py` — stripe import gated in LOCAL_MODE
- [x] `backend/utils/stripe.py` — stubbed in LOCAL_MODE
- [x] `backend/utils/twilio_service.py` — stubbed in LOCAL_MODE
- [x] `backend/utils/notifications.py` — stubbed in LOCAL_MODE
- [x] `backend/utils/pusher.py` — stubbed in LOCAL_MODE
- [x] `backend/utils/stt/streaming.py` — Deepgram disabled in LOCAL_MODE
- [x] `backend/utils/stt/pre_recorded.py` — Deepgram stubbed in LOCAL_MODE
- [x] `backend/utils/llm/clients.py` — cloud LLM providers replaced with LOCAL_LLM gateway
