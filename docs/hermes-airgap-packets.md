# Hermes Airgap Cleanup Packets

These packets break the `local/airgap-cleanup` branch into small, parallel-friendly chunks for Hermes.

## North Star

Deliver a **remote-free local desktop stack**:

- no Firebase / Firestore runtime dependency
- no Deepgram cloud runtime dependency
- no production API fallback in the desktop app
- no cloud LLM fallback in local mode
- no Stripe / Crisp / PostHog / Sentry / Twilio / GCE Agent VM dependency in the local path
- local mode should either work locally or fail loudly with a clear message

If a feature cannot be made local yet, it must be **stubbed or hard-disabled**. Do not silently route user data to remote services.

---

## Packet index

| Packet | Title | Parallel? | Depends on |
|---|---|---:|---|
| A | Python backend local-mode import cleanup | yes | none |
| B | Rust backend local-mode gating | yes | none |
| C | Desktop app remote-fallback removal | yes | none |
| D | Cloud-only feature stubs and removals | yes | A/B/C |
| E | Final audit and smoke tests | no | A/B/C/D |

---

## Packet A — Python backend local-mode import cleanup

**Goal:** `LOCAL_MODE=1` should import and start the Python backend without touching cloud-only initialization paths.

### Scope

- `backend/database/chat.py`
- `backend/main.py`
- `backend/routers/auth.py`
- `backend/routers/oauth.py`
- any directly related import-time helpers needed to keep these files import-safe

### Checklist

- [ ] Fix `backend/database/chat.py` so the local-mode branch is structurally valid and import-safe
- [ ] Ensure decorators in `chat.py` never evaluate in `LOCAL_MODE=1`
- [ ] Verify `backend/main.py` does not require Firebase initialization in local mode
- [ ] Ensure `validate_stripe_price_ids()` is safely stubbed or gated in local mode
- [ ] Remove any local-mode runtime dependency on Firebase token verification in auth/oauth routes
- [ ] Keep local-mode imports free of cloud-only side effects

### Exit criteria

- [ ] `python3 -c "import os; os.environ['LOCAL_MODE']='1'; import main"` succeeds from `backend/`
- [ ] No cloud initialization occurs at import time in local mode
- [ ] No Firebase credentials are required just to boot the local backend

### Notes

- This is the current highest-priority blocker because it prevents the backend from being import-clean in local mode.
- Prefer the smallest fix that makes the module structurally valid and maintainable.

---

## Packet B — Rust backend local-mode gating

**Goal:** the Rust desktop backend should start in local mode without cloud credentials or cloud proxy requirements.

### Scope

- `desktop/Backend-Rust/src/main.rs`
- `desktop/Backend-Rust/src/config.rs`
- `desktop/Backend-Rust/src/auth.rs`
- `desktop/Backend-Rust/src/routes/proxy.rs`
- `desktop/Backend-Rust/src/routes/auth.rs`

### Checklist

- [ ] Make `LOCAL_MODE` the primary gate for cloud startup behavior
- [ ] Prevent local mode from requiring Firebase project IDs or Firestore initialization
- [ ] Remove or disable Google public key fetching during local-mode startup
- [ ] Ensure auth verification in local mode is self-contained
- [ ] Disable or stub cloud proxy routes in local mode
- [ ] Prevent Gemini / Deepgram cloud proxying in the local-only path
- [ ] Ensure local model calls route only to configured local services

### Exit criteria

- [ ] Rust backend boots with `LOCAL_MODE=1` and no cloud credentials
- [ ] No cloud proxy routes are usable in local mode
- [ ] The local backend does not need Firestore to come up

### Notes

- This packet is independent from Packet A, but the final app will only be truly local if both land.
- Keep the local-mode behavior explicit and fail-closed.

---

## Packet C — Desktop app remote-fallback removal

**Goal:** the macOS app should never fall back to production URLs when local mode is active.

### Scope

- `desktop/run.sh`
- `desktop/.env.example`
- `desktop/Desktop/Sources/APIClient.swift`
- `desktop/Desktop/Sources/TranscriptionService.swift`
- `desktop/Desktop/Sources/AuthService.swift`
- `desktop/Desktop/Sources/OmiApp.swift`
- any small helper touched by these files

### Checklist

- [ ] Remove production API fallbacks from local-mode startup paths
- [ ] Ensure `desktop/run.sh` does not rewrite local config to `api.omi.me`
- [ ] Keep `LOCAL_MODE=1` from starting tunnel/auth/backend services that are not needed locally
- [ ] Remove `https://api.omi.me` as an implicit transcription fallback
- [ ] Ensure the app uses only explicit local URLs or clear stubs in local mode
- [ ] Make Firebase bypass behavior deterministic in local mode
- [ ] Keep desktop startup logs clear about local-only mode

### Exit criteria

- [ ] A local-mode launch does not contact production endpoints
- [ ] `OMI_PYTHON_API_URL` is never silently rewritten to a remote default in local mode
- [ ] The app boots cleanly with only local services configured

### Notes

- This packet is ideal for parallel work because most of it is confined to desktop startup and client routing code.
- Favor explicit failure over hidden remote fallback.

---

## Packet D — Cloud-only feature stubs and removals

**Goal:** any feature that is not part of the local single-user experience should be removed from the local path or hard-stubbed.

### Scope

- payment / Stripe related code
- Crisp support code
- PostHog analytics code
- Sentry reporting / webhook code
- Twilio code
- GCE Agent VM provisioning code
- remote update / appcast code
- cloud TTS providers
- any other cloud-only product integrations that leak into local mode

### Checklist

- [ ] Identify cloud-only product features still reachable from local mode
- [ ] Decide per feature: delete, stub, or hard-disable
- [ ] Make local-mode behavior fail loudly when a feature is unavailable locally
- [ ] Remove any runtime code path that can silently ship data to a remote service
- [ ] Keep the local build path free of cloud-only operational dependencies

### Exit criteria

- [ ] No local-mode execution path depends on cloud-only product integrations
- [ ] Missing local support is represented as a clear stub or explicit error

### Notes

- This packet can be split further if Hermes wants to fan out on individual services.
- It is acceptable to leave cloud-only code in the repository if it is unreachable from the local path and clearly gated.

---

## Packet E — Final audit and smoke tests

**Goal:** prove the tree is remote-free for local execution.

### Scope

- repo-wide grep / audit
- backend import smoke test
- local-mode boot checks
- any small test harnesses needed to verify the absence of accidental remotes

### Checklist

- [ ] Search the repo for runtime cloud refs and classify each hit
- [ ] Confirm only intentional docs / comments / tests mention remote providers
- [ ] Run the Python local-mode import smoke test
- [ ] Run backend / desktop local-mode smoke checks where available
- [ ] Capture any remaining remote dependency as either intentional cloud-only or a bug
- [ ] Produce a final short audit summary with remaining blockers, if any

### Exit criteria

- [ ] Grep audit shows no unintended remote runtime paths
- [ ] Local-mode boot checks pass for the pieces that are expected to run locally
- [ ] The repo can be described as remote-free for the local desktop path

### Notes

- This packet should be last.
- Hermes should not start here; it should only be used to confirm the work from Packets A–D.

---

## Recommended execution order

1. Packet A
2. Packet B
3. Packet C
4. Packet D
5. Packet E

Packets A, B, and C can run in parallel because they touch mostly separate parts of the tree.

---

## Hermes-friendly definition of done

The cleanup is done when:

- local mode boots without cloud credentials
- no local-mode code path falls back to `api.omi.me`
- cloud model / transcription services are not reachable from the local path
- any remaining cloud integrations are explicitly marked cloud-only and excluded from local mode
- the final audit has no accidental remote runtime references left
