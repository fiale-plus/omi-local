# Local / Airgap Cleanup Plan

## Objective

Turn `local/airgap-cleanup` into a codebase that can run the desktop experience **without runtime dependence on cloud services or cloud credentials**.

The target outcome is a **remote-free local stack**:

- no Firebase / Firestore
- no Deepgram cloud
- no Gemini / OpenAI / Anthropic cloud fallback
- no Stripe dependency in the local path
- no Crisp / PostHog / Sentry / Twilio / GCE Agent VM dependency in the local path
- no production API fallback in the desktop app
- no Cloudflare tunnel requirement for local mode

If a feature cannot be supported locally yet, it must be **stubbed, disabled, or hard-failed with a clear message** — never silently routed to a remote service.

---

## Current blocking issue

There is one immediate Python import bug that must be fixed early:

- `backend/database/chat.py`
  - the `_LOCAL_MODE` refactor left CRUD / decorator code structurally outside the intended `else:` block
  - decorators are evaluated at import time, so `LOCAL_MODE=1` can fail during import

Also review:

- `backend/main.py`
  - still needs its startup path fully guarded for `LOCAL_MODE`
  - `validate_stripe_price_ids()` may need to remain stubbed in local mode

---

## Parallel execution model

This cleanup should be done in **parallel worktrees** so the work can move fast without file conflicts.

Suggested lanes:

| Lane | Branch suggestion | Scope | Primary files |
|---|---|---|---|
| A | `airgap/backend-local` | Python backend import-clean + local-mode gating | `backend/database/chat.py`, `backend/main.py`, `backend/routers/auth.py`, `backend/routers/oauth.py` |
| B | `airgap/desktop-local` | Desktop app no-remote fallbacks | `desktop/run.sh`, `desktop/.env.example`, `desktop/Desktop/Sources/APIClient.swift`, `desktop/Desktop/Sources/TranscriptionService.swift`, `desktop/Desktop/Sources/AuthService.swift`, `desktop/Desktop/Sources/OmiApp.swift` |
| C | `airgap/rust-local` | Rust backend local-only startup + cloud gating | `desktop/Backend-Rust/src/main.rs`, `desktop/Backend-Rust/src/config.rs`, `desktop/Backend-Rust/src/auth.rs`, `desktop/Backend-Rust/src/routes/proxy.rs`, `desktop/Backend-Rust/src/routes/auth.rs` |
| D | `airgap/cloud-stub-removals` | remove or stub remaining cloud-only product features | payment, agent VM, Crisp, PostHog, Sentry, Twilio, updates, TTS, analytics |
| E | `airgap/audit-tests` | repo-wide remote dependency audit + smoke tests | grep/audit scripts, local-mode tests, import checks |

---

## Workstream details

### Lane A — Python backend local-mode cleanup

**Goal:** `LOCAL_MODE=1` should import and start cleanly without cloud initialization.

#### Must fix
- `backend/database/chat.py`
  - move the CRUD implementation fully inside the cloud branch, or restructure it so decorators never exist in local mode
  - remove the structural bug introduced by the refactor
- `backend/main.py`
  - keep Firebase initialization fully disabled in local mode
  - ensure startup does not require cloud-only imports or config
- `backend/routers/auth.py`
  - local mode must not require Firebase token creation / verification
- `backend/routers/oauth.py`
  - local mode should not depend on Firebase UI / Firebase token verification

#### Exit criteria
- `python3 -c "import os; os.environ['LOCAL_MODE']='1'; import main"` succeeds from `backend/`
- no cloud initialization happens on import in local mode
- local mode does not require Firebase credentials

---

### Lane B — Desktop app local-mode cleanup

**Goal:** the app should not fall back to remote production endpoints when `LOCAL_MODE=1`.

#### Must fix
- `desktop/run.sh`
  - remove any path that rewrites local config to production URLs in local mode
  - do not inject `https://api.omi.me` as a fallback for local mode
  - do not require cloudflared or a tunnel in local mode
- `desktop/.env.example`
  - make local-only behavior explicit
  - eliminate remote defaults from the local-mode path
- `desktop/Desktop/Sources/APIClient.swift`
  - remove production backend fallback for local mode
- `desktop/Desktop/Sources/TranscriptionService.swift`
  - remove fallback to `https://api.omi.me`
  - local mode should target local transcription only
- `desktop/Desktop/Sources/AuthService.swift`
  - local mode must bypass Firebase entirely
- `desktop/Desktop/Sources/OmiApp.swift`
  - startup should not force Firebase init in local mode

#### Exit criteria
- a local-mode launch never touches remote production URLs
- local auth / API routing only uses local endpoints or clear stubs
- the app can boot with only local services configured

---

### Lane C — Rust backend local-only startup

**Goal:** the Rust desktop backend should start in local mode without cloud credentials or cloud proxy requirements.

#### Must fix
- `desktop/Backend-Rust/src/main.rs`
  - do not require Firebase project IDs in local mode
  - do not initialize cloud Firestore in local mode
- `desktop/Backend-Rust/src/config.rs`
  - local mode should be the authoritative gate for cloud features
- `desktop/Backend-Rust/src/auth.rs`
  - avoid startup/network dependency on Google public keys in local mode
  - dev auth should be self-contained
- `desktop/Backend-Rust/src/routes/proxy.rs`
  - cloud proxy routes should be disabled or stubbed in local mode
  - no Gemini / Deepgram cloud proxying in a remote-free build path
- `desktop/Backend-Rust/src/routes/auth.rs`
  - OAuth flows should not assume Firebase / hosted auth in local mode

#### Exit criteria
- Rust backend boots with `LOCAL_MODE=1` and no cloud creds
- no cloud proxy routes are reachable in local mode
- any model/STT calls go to local services only

---

### Lane D — Remove or stub remaining cloud-only product features

**Goal:** anything not needed for a local single-user experience should be removed or hard-stubbed.

#### Likely cloud-only surfaces
- payments / Stripe
- Crisp support
- PostHog analytics
- Sentry reporting / webhooks
- Twilio
- GCE Agent VM provisioning
- remote updates / appcast fetches
- cloud TTS providers
- cloud app marketplace / remote webhook behavior

#### Rule
If the feature is not local-first, it should either:
1. be deleted from the local build path,
2. be hidden behind a strict local-mode stub, or
3. be disabled with a clear "not available locally" error.

#### Exit criteria
- no local-mode execution path depends on any of the above services
- no code path silently sends user data to cloud providers

---

### Lane E — Final audit + tests

**Goal:** prove the tree is actually remote-free for local execution.

#### Audit commands
Run a repo-wide search for remaining runtime cloud refs, especially:

- `firebase_admin`
- `firestore`
- `api.omi.me`
- `generativelanguage.googleapis.com`
- `api.deepgram.com`
- `fal-ai`
- `openai`
- `anthropic`
- `stripe`
- `twilio`
- `crisp`
- `posthog`
- `sentry`
- `compute.googleapis.com`
- `storage.googleapis.com`

#### Tests / smoke checks
- backend Python import check in `LOCAL_MODE=1`
- backend runtime smoke test without cloud creds
- Rust backend local-mode boot check
- desktop local-mode boot check
- grep audit should show only intentional docs/comments/tests, not runtime code paths

---

## Suggested merge order

1. **Lane A first** — unblock Python import safety
2. **Lane C second** — remove Rust startup cloud dependencies
3. **Lane B in parallel** — desktop routing and startup cleanup
4. **Lane D next** — stub/delete any remaining cloud-only features
5. **Lane E last** — audit and verify the tree is remote-free

---

## Definition of done

The cleanup is complete when all of the following are true:

- `LOCAL_MODE=1` works end-to-end without Firebase, Firestore, or production API defaults
- no app startup path falls back to `api.omi.me`
- no local-mode path requires Deepgram cloud, Gemini cloud, OpenAI cloud, Anthropic cloud, Stripe, Crisp, PostHog, Sentry, Twilio, or GCE VM provisioning
- the local stack can run with SQLite + local filesystem + local model gateway only
- grep-based audit confirms the remote dependencies are gone from runtime code paths

---

## Notes for Hermes / parallel agents

This plan is intended to be split into independent worktrees. Each lane above can be assigned to a separate agent as long as they avoid overlapping files.

If a feature has no local replacement yet, prefer a **stub or hard disable** rather than reintroducing a remote fallback.
