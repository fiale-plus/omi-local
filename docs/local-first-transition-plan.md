# Omi local-first transition plan

## Goal

Make the **desktop Omi experience realistically runnable in a fully local, single-user mode** without depending on Firebase, Firestore, Pinecone, Deepgram cloud, or Omi-hosted APIs.

This plan is intentionally implementation-oriented so it can be split across multiple coding agents/models.

## Scope

### In scope
- macOS desktop app (`desktop/`)
- Rust desktop backend (`desktop/Backend-Rust/`)
- Python backend paths still required by the desktop app (`backend/`), especially transcription and conversation processing
- Local replacements for auth, storage, vectors, model routing, and file storage

### Out of scope for the first local release
- Mobile parity
- Multi-user or hosted deployments
- Billing / Stripe
- Agent VM provisioning on GCE
- Cloud webhooks, app marketplace analytics, Crisp, PostHog, Sentry webhook workflows
- 100% feature parity with the hosted product on day one

---

## Executive summary

The repo is **not one step away** from “fully local”, but it is also **not a ground-up rewrite**.

The fastest realistic path is:

1. **Keep the desktop app mostly intact**.
2. **Promote `desktop/Backend-Rust` into the primary local API/control plane**.
3. **Introduce a real `LOCAL_MODE`** across app + Rust backend + Python worker.
4. **Replace cloud infrastructure behind stable APIs**, instead of rewriting the Swift UI first.
5. **Shrink the Python backend to the flows the desktop app still needs** (`/v4/listen`, `/v2/voice-message/transcribe*`, conversation processing), then optionally retire more of it later.

That gives a phased path where the app only needs moderate changes, while the backend migration happens behind compatibility boundaries.

---

## Current-state audit

### 1) The desktop app is not fully local today

Evidence:
- `desktop/Desktop/Sources/APIClient.swift`
  - comment says the **Python backend is the single source of truth for data CRUD**
  - `OMI_PYTHON_API_URL` defaults to `https://api.omi.me/`
- `desktop/Desktop/Sources/TranscriptionService.swift`
  - conversation capture uses **Python `/v4/listen`**
  - push-to-talk uses **Python `/v2/voice-message/transcribe*`**
- `desktop/Desktop/Sources/AuthService.swift`
  - auth depends on **Firebase Auth**
- `desktop/Auth-Python/README.md`
  - OAuth broker creates **Firebase custom tokens**
- `desktop/run.sh`
  - local dev still expects **Firebase project config**

### 2) The Rust desktop backend is a strong leverage point

Evidence:
- `desktop/Backend-Rust/src/main.rs`
  - already exposes many desktop-facing routes
- `desktop/Backend-Rust/src/routes/`
  - has route modules for conversations, memories, action items, goals, chat, personas, staged tasks, screen activity, etc.
- `desktop/Backend-Rust/src/services/firestore.rs`
  - already ports a large amount of Python data access into Rust

Implication:
- The best local architecture is **not** “run the cloud Python backend forever on localhost”.
- The best local architecture is to **make the Rust backend the first-class local backend** and only keep Python for the flows that have not been ported yet.

### 3) Both backends are deeply cloud-coupled

#### Firebase / Firestore
- Python: `backend/main.py`, `backend/dependencies.py`, `backend/database/_client.py`
- Rust: `desktop/Backend-Rust/src/auth.rs`, `desktop/Backend-Rust/src/main.rs`, `desktop/Backend-Rust/src/services/firestore.rs`

#### Redis
- Python: `backend/database/redis_db.py`
- Rust: `desktop/Backend-Rust/src/services/redis.rs`

#### Pinecone
- Python: `backend/database/vector_db.py`
- Rust: `desktop/Backend-Rust/src/routes/screen_activity.rs`

#### Google Cloud Storage
- Python: `backend/utils/other/storage.py`

#### Deepgram
- Python streaming path: `backend/utils/stt/streaming.py`
- Desktop app assumes Python transcription routes in `desktop/Desktop/Sources/TranscriptionService.swift`

#### Hosted conversation processing dependency
- `docs/doc/developer/backend/listen_pusher_pipeline.mdx`
  - explicitly says **listen never processes conversations locally**
  - **pusher is required** for conversation processing

This last point is the biggest blocker for a true local mode.

---

## Recommended target architecture

## Local mode definition

Add a product-level mode:

- `LOCAL_MODE=true`
- single user
- loopback-only backend exposure by default
- no Firebase requirement
- no cloud storage requirement
- no remote vector DB requirement
- local model endpoints configurable

## Recommended end-state shape

### A. Desktop app
- stays mostly the same UI-wise
- routes all backend traffic to localhost
- uses local session identity instead of Firebase
- hides or stubs cloud-only features

### B. Rust backend = primary local control plane
Responsible for:
- local auth/session
- local CRUD APIs
- settings
- chat/session/task/memory/conversation APIs
- local file + metadata orchestration
- routing to local model providers

### C. Python worker = transitional local processing service
Responsible initially for:
- `/v4/listen`
- `/v2/voice-message/transcribe`
- `/v2/voice-message/transcribe-stream`
- conversation processing pipeline currently tied to listen/pusher

Longer term, move more of this into Rust or into a dedicated local processing worker, but **do not block the first local release on full consolidation**.

---

## Cloud dependency replacement matrix

| Cloud dependency | Current usage | Recommended local replacement | Release priority |
|---|---|---|---|
| Firebase Auth | desktop auth + both backends | local loopback session + signed local JWT or static dev token | P0 |
| Firestore | primary data store | SQLite owned by local backend | P0 |
| Redis | throttling, cache, sharing, misc flags | in-process cache first; SQLite/in-memory for durable flags | P1 |
| Google Cloud Storage | speech profiles, audio blobs, uploads | local filesystem under Application Support | P0 |
| Pinecone | memory/search/screen embeddings | phase 1: degrade to lexical/FTS; phase 2: local vector store | P1 |
| Deepgram cloud | streaming STT | local Whisper/faster-whisper/whisper.cpp service | P0 |
| Hosted pusher | conversation processing | local processing worker / in-process queue | P0 |
| OpenAI / Anthropic / Gemini cloud | summaries, chat, extraction | OpenAI-compatible local gateway (LM Studio / Ollama / vLLM) + provider abstraction | P0 |
| Stripe / payments | plan/subscription | local unlimited plan stub | P2 |
| GCE Agent VM | remote automation worker | disable in local mode | P2 |

---

## Architectural decisions to make up front

## Decision 1: local mode is single-user only

**Recommend: yes.**

Reason:
- eliminates most auth complexity
- avoids multi-tenant data model work
- matches the practical “my Mac, my data” use case

Implementation rule:
- local mode always runs as one synthetic user, e.g. `local-user`

## Decision 2: do not share the app’s existing GRDB file as the backend source of truth initially

**Recommend: keep a separate backend-owned SQLite DB for phase 1.**

Reason:
- cleaner ownership
- less cross-process migration risk
- preserves current app cache/sync behavior
- easier for multiple agents to work independently

Later optimization:
- if desired, unify app cache and backend DB after local mode is stable

## Decision 3: keep API compatibility wherever possible

**Recommend: yes.**

Reason:
- minimizes Swift UI changes
- lets multiple models work in parallel on backend replacements
- preserves current acceptance surface

## Decision 4: do not chase full hosted parity in the first release

**Recommend: yes.**

First local release should target:
- sign in / bootstrap
- transcription
- conversations
- memories
- tasks / goals
- local chat over local data

Not required initially:
- payments
- app marketplace cloud behavior
- cloud analytics
- remote Agent VM

---

## Proposed implementation waves

## Wave 0 — planning guardrails and feature flag foundation

### Outcome
The repo can build in a `LOCAL_MODE` without changing normal hosted behavior.

### Tasks
1. Add a shared `LOCAL_MODE` contract to:
   - desktop app env loading
   - Rust backend config
   - Python backend config
2. Define a single source of truth document for:
   - local ports
   - local paths
   - local auth behavior
   - local model endpoints
3. Add a “cloud-only feature” flag list.

### Touchpoints
- `desktop/run.sh`
- `desktop/.env.example`
- `desktop/Backend-Rust/src/config.rs`
- new config file(s) if needed
- Python env/template files as needed

### Dependencies
- none

### Can run in parallel?
- yes, fully parallel with Wave 1 discovery work

---

## Wave 1 — local auth/bootstrap

### Outcome
The desktop app can start in local mode without Firebase, Google OAuth, or Auth-Python.

### Tasks
1. Add a local auth mode in the desktop app:
   - bypass Firebase restore/sign-in requirement
   - create/load a local identity record
2. Add Rust backend local auth extractor:
   - accept local loopback token/session
   - bypass Firebase key fetching in local mode
3. Disable Auth-Python startup in local mode.
4. Define local-only security posture:
   - bind to `127.0.0.1`
   - rotate a local secret on first launch
   - optionally store in Keychain

### Touchpoints
- `desktop/Desktop/Sources/AuthService.swift`
- `desktop/Desktop/Sources/OmiApp.swift`
- `desktop/run.sh`
- `desktop/Auth-Python/`
- `desktop/Backend-Rust/src/auth.rs`
- `desktop/Backend-Rust/src/main.rs`

### Dependencies
- Wave 0

### Risks
- sign-in assumptions are scattered through the app
- analytics/user profile code may assume Firebase UID shape

### Acceptance criteria
- fresh local-mode launch works without Firebase config
- app receives an authenticated session against localhost
- switching back to hosted mode still works

---

## Wave 2 — local storage foundation (Firestore/GCS replacement)

### Outcome
Core desktop data can persist locally without Firestore or GCS.

### Tasks
1. Define backend-owned SQLite schema for local mode.
2. Introduce storage interfaces / repositories in Rust for:
   - users/profile/settings
   - conversations
   - memories
   - action items / staged tasks
   - goals
   - chat sessions/messages
3. Add local filesystem storage adapter for:
   - speech profiles
   - audio chunks / recordings
   - uploaded assets required in local mode
4. Add migration/bootstrap for creating the local DB and folders.

### Touchpoints
- `desktop/Backend-Rust/src/services/firestore.rs` → extract trait boundary
- `desktop/Backend-Rust/src/routes/*`
- new Rust storage modules for SQLite/filesystem
- Python `backend/utils/other/storage.py` for temporary local worker compatibility

### Dependencies
- Wave 0
- Wave 1 for auth-linked user identity

### Parallelism
- repository extraction per domain can be parallelized
- filesystem adapter can run in parallel with SQLite schema work

### Acceptance criteria
- CRUD flows for profile/settings/conversations/memories/tasks/goals work locally with no Firestore/GCS

---

## Wave 3 — make Rust backend the default local CRUD API

### Outcome
The desktop app no longer needs the Python backend for normal CRUD in local mode.

### Tasks
1. Audit APIClient calls and map them to Rust routes.
2. Fill route gaps in `desktop/Backend-Rust/src/routes/` for the local-mode-required surface.
3. Stub unsupported cloud-only endpoints with explicit local-mode responses.
4. Point `OMI_PYTHON_API_URL` consumers away from Python where equivalent Rust routes exist.

### Required route categories for the first local release
- conversations
- memories
- action items / staged tasks
- goals
- user profile/settings
- chat sessions / messages
- knowledge graph only if already needed by desktop UX

### Defer or stub
- payments/subscription checkout
- cloud app marketplace extras
- Agent VM provisioning
- cloud message share flows if they require hosted infra

### Touchpoints
- `desktop/Desktop/Sources/APIClient.swift`
- `desktop/Backend-Rust/src/routes/*`
- `desktop/Backend-Rust/src/main.rs`

### Dependencies
- Wave 2

### Acceptance criteria
- normal dashboard/tasks/memories/goals/chat CRUD stays entirely local

---

## Wave 4 — local transcription path

### Outcome
The app can transcribe in local mode without Deepgram cloud.

### Recommended pragmatic split
#### Phase 4A: PTT first
Implement local replacements for:
- `POST /v2/voice-message/transcribe`
- `WS /v2/voice-message/transcribe-stream`

#### Phase 4B: continuous conversation capture
Implement local replacement for:
- `WS /v4/listen`

### Recommended approach
- use a configurable local STT provider abstraction
- default to a local Whisper-family backend
- keep response contracts compatible with existing Swift handlers

### Good implementation options
- external local service using `faster-whisper` or `whisper.cpp`
- direct on-device reuse of existing Whisper-related code paths where feasible
  - evidence exists in `sdks/swift/`
  - mobile app also already has local/on-device Whisper support in `app/lib/services/sockets/on_device_whisper_provider.dart`

### Important realism note
`/v4/listen` today is not just STT. It also carries:
- speech profile handling
- speaker assignment hooks
- conversation lifecycle events
- memory events

So **PTT local transcription is the easy win**. Continuous capture is a larger work item.

### Touchpoints
- `desktop/Desktop/Sources/TranscriptionService.swift`
- `backend/routers/transcribe.py`
- `backend/utils/stt/streaming.py`
- new local STT adapter modules

### Dependencies
- Wave 1 for local auth
- Wave 2 for local file/audio persistence

### Acceptance criteria
- push-to-talk works end-to-end locally
- continuous conversation mode can at minimum transcribe and rotate conversations locally

---

## Wave 5 — local conversation processing (replace pusher dependency)

### Outcome
Conversation completion no longer depends on hosted pusher.

### Why this is the hardest backend item
The repo explicitly documents that listen currently **never processes conversations locally** and requires pusher.

### Tasks
1. Extract conversation-processing entrypoints from the current pusher pipeline.
2. Create a local job runner / queue.
3. Replace pusher opcodes with local in-process or localhost job dispatch.
4. Reimplement these outputs locally:
   - title
   - overview
   - category / emoji
   - action item extraction
   - memory extraction
5. Write results to local DB instead of Firestore/Pinecone/GCS.

### Suggested implementation strategy
- keep the Python processing code first
- replace storage/model dependencies underneath it
- only port to Rust after parity is acceptable

This is more realistic than rewriting the whole processing stack in Rust immediately.

### Touchpoints
- `backend/pusher/`
- `backend/utils/conversations/process_conversation.py`
- `backend/utils/llm/conversation_processing.py`
- `backend/database/vector_db.py`
- `backend/utils/other/storage.py`
- `docs/doc/developer/backend/listen_pusher_pipeline.mdx` (must update when behavior changes)

### Dependencies
- Wave 2
- Wave 4B strongly related

### Acceptance criteria
- a completed local conversation produces summary + tasks + memories without hosted pusher

---

## Wave 6 — local model routing

### Outcome
All AI calls required by local mode can use local models.

### Tasks
1. Define provider abstraction for:
   - chat/completions
   - extraction/structured outputs
   - embeddings
2. Support OpenAI-compatible local endpoints first.
3. Add local config UI or env wiring for:
   - base URL
   - model names
   - embedding model
   - timeout / context settings
4. Keep hosted providers optional, not required.

### Recommended default shape
- OpenAI-compatible local gateway endpoint
- model role mapping:
  - chat model
  - extraction model
  - embedding model

### Why this matters
- Python already has several OpenAI-compatible call sites via LangChain
- Rust chat code is currently Gemini-centric and will need abstraction work

### Touchpoints
- Python: `backend/utils/llm/clients.py`
- Rust: `desktop/Backend-Rust/src/llm/client.rs`, `desktop/Backend-Rust/src/routes/chat.rs`
- app settings only if exposed to users

### Dependencies
- can start in parallel with Waves 4 and 5

### Acceptance criteria
- no required cloud LLM credentials for local mode

---

## Wave 7 — local search/vector layer

### Outcome
Memories/conversations/screen activity retrieval works locally without Pinecone.

### Pragmatic rollout
#### Phase 7A
- lexical/FTS fallback only
- ship local mode without perfect semantic search

#### Phase 7B
- add local vector store
- backfill embeddings

### Recommendation
Do **not** block the first local release on perfect Pinecone replacement.

### Touchpoints
- `backend/database/vector_db.py`
- `desktop/Backend-Rust/src/routes/screen_activity.rs`
- local retrieval paths in chat/context services

### Dependencies
- Wave 2
- Wave 6 for local embeddings

### Acceptance criteria
- first release: search remains useful locally
- second release: semantic retrieval restored

---

## Wave 8 — app UX cleanup for local mode

### Outcome
The desktop app behaves like a coherent local product instead of a cloud product pointed at localhost.

### Tasks
1. Add local mode onboarding.
2. Hide or relabel:
   - subscriptions/payments
   - cloud upgrade prompts
   - Agent VM / hosted-only features
3. Add local model status/settings surface.
4. Add diagnostics screen:
   - local backend reachable
   - local worker reachable
   - STT model ready
   - LLM configured
   - embedding index status

### Touchpoints
- `desktop/Desktop/Sources/Onboarding*`
- `desktop/Desktop/Sources/MainWindow/Pages/SettingsPage.swift`
- subscription/billing related Swift flows

### Dependencies
- Waves 1 through 6

### Acceptance criteria
- a new user can understand and complete local setup without cloud assumptions

---

## Recommended task packets for multi-model execution

### Capsule rules
- One owner, one file cluster, one outcome.
- If a packet spans two concerns or more than ~3 files, split it before assigning.
- Every handoff should carry: goal, files, dependencies, non-goals, and done criteria.

### Packet A — config and local-mode scaffolding
**Good for:** GPT-5.4-mini / GLM 5.1  
**Depends on:** none  
**Files:** `desktop/run.sh`, `desktop/.env.example`, `desktop/Backend-Rust/src/config.rs`, Python env/template files

**Goal:** add a single `LOCAL_MODE` contract across desktop app, Rust backend, and Python worker.
**Non-goals:** auth, storage, or route rewrites.
**Done:** app/backend/worker all read the same mode, ports, and path contracts.

### Packet B — desktop auth bypass / local identity
**Good for:** GPT-5.4-mini / GLM 5.1  
**Depends on:** Packet A  
**Files:** `desktop/Desktop/Sources/AuthService.swift`, `desktop/Desktop/Sources/OmiApp.swift`, `desktop/run.sh`, `desktop/Auth-Python/`, `desktop/Backend-Rust/src/auth.rs`, `desktop/Backend-Rust/src/main.rs`

**Goal:** boot local mode without Firebase/Auth-Python and create a local identity record.
**Non-goals:** hosted auth redesign.
**Done:** fresh local launches authenticate against localhost and hosted mode still works.

### Packet C — Rust backend auth and localhost security
**Good for:** GPT-5.4-mini / GLM 5.1  
**Depends on:** Packet A  
**Files:** `desktop/Backend-Rust/src/auth.rs`, `desktop/Backend-Rust/src/main.rs`

**Goal:** accept local loopback auth and lock the backend to localhost.
**Non-goals:** storage migrations or app UI changes.
**Done:** local token/session extraction works, and non-loopback access is rejected.

### Packet D — Rust repository abstraction + SQLite bootstrap
**Good for:** MiniMax 2.7  
**Depends on:** Packet A, C  
**Split before assignment:** yes — D1 = trait/schema extraction, D2 = SQLite + migrations.
**Files:** `desktop/Backend-Rust/src/services/firestore.rs`, `desktop/Backend-Rust/src/routes/*`, new Rust storage modules

**Goal:** extract backend storage traits and stand up backend-owned SQLite.
**Non-goals:** route parity or filesystem blobs.
**Done:** profile/settings/conversations/memories/tasks/goals persist locally.

### Packet E — filesystem storage adapter
**Good for:** GLM 5.1 / GPT-5.4-mini  
**Depends on:** Packet A  
**Files:** `backend/utils/other/storage.py`, local storage helpers in Rust if needed

**Goal:** replace GCS-backed file flows with local filesystem storage.
**Non-goals:** vector search or conversation processing.
**Done:** speech profiles, audio, and uploads land on disk with stable paths.

### Packet F — local CRUD route parity in Rust
**Good for:** MiniMax 2.7  
**Depends on:** Packet D, E  
**Split before assignment:** yes — F1 = route audit, F2 = missing endpoints + stubs.
**Files:** `desktop/Backend-Rust/src/routes/*`, `desktop/Desktop/Sources/APIClient.swift`, `desktop/Backend-Rust/src/main.rs`

**Goal:** keep the normal desktop CRUD surface local in Rust.
**Non-goals:** payments, Agent VM, or other cloud-only features.
**Done:** conversations, memories, tasks, goals, sessions, and settings work locally.

### Packet G — local PTT transcription
**Good for:** MiniMax 2.7 or strong GLM run  
**Depends on:** Packet A, B, C, E  
**Files:** `desktop/Desktop/Sources/TranscriptionService.swift`, `backend/routers/transcribe.py`, `backend/utils/stt/streaming.py`

**Goal:** make push-to-talk transcription work locally.
**Non-goals:** full continuous-capture parity.
**Done:** `/v2/voice-message/transcribe*` works against a local STT backend and the Swift client stays compatible.

### Packet H — local continuous `/v4/listen`
**Good for:** MiniMax 2.7  
**Depends on:** Packet G, D, E  
**Split before assignment:** yes — H1 = websocket transport, H2 = event/rotation semantics.
**Files:** `backend/routers/listen.py`, `backend/utils/stt/streaming.py`, listen pipeline modules

**Goal:** replace the hosted listen transport with a local WebSocket path.
**Non-goals:** perfect diarization parity.
**Done:** local conversation rotation and listen events work over localhost.

### Packet I — local conversation processing / pusher replacement
**Good for:** MiniMax 2.7 only  
**Depends on:** Packet H, D, E, provider abstraction  
**Split before assignment:** yes — I1 = queue/runner, I2 = summaries/tasks/memories/writeback.
**Files:** `backend/pusher/`, `backend/utils/conversations/process_conversation.py`, `backend/utils/llm/conversation_processing.py`, `backend/database/vector_db.py`

**Goal:** process completed conversations locally instead of via hosted pusher.
**Non-goals:** rewriting the entire processing stack to Rust first.
**Done:** a completed local conversation produces summary/task/memory outputs and writes them locally.

### Packet J — model provider abstraction
**Good for:** MiniMax 2.7  
**Depends on:** Packet A  
**Split before assignment:** yes — J1 = provider interface, J2 = Rust/Python callsites.
**Files:** `backend/utils/llm/clients.py`, `desktop/Backend-Rust/src/llm/client.rs`, `desktop/Backend-Rust/src/routes/chat.rs`

**Goal:** support OpenAI-compatible local endpoints for chat, extraction, and embeddings.
**Non-goals:** changing the desktop UX yet.
**Done:** local mode can run without cloud LLM credentials.

### Packet K — local search fallback + vector phase 1
**Good for:** GPT-5.4-mini / GLM 5.1  
**Depends on:** Packet D, J  
**Files:** `backend/database/vector_db.py`, `desktop/Backend-Rust/src/routes/screen_activity.rs`, chat/context retrieval paths

**Goal:** keep retrieval useful without Pinecone.
**Non-goals:** full semantic parity on day one.
**Done:** lexical/FTS fallback works and vector backfill hooks exist.

### Packet L — local UX cleanup
**Good for:** GPT-5.4-mini / GLM 5.1  
**Depends on:** Packets B, F, G minimum  
**Files:** onboarding/settings Swift screens, subscription/billing flows

**Goal:** make the app feel like a coherent local product.
**Non-goals:** any hosted-only feature expansion.
**Done:** local onboarding, diagnostics, and cloud-only feature hiding are in place.

---

## Dependency graph

```text
A config scaffold
├─ B desktop local auth
├─ C rust local auth
├─ E filesystem adapter
├─ J model provider abstraction
└─ D rust repository abstraction
   ├─ F rust CRUD parity
   ├─ K local search fallback
   ├─ G local PTT transcription
   │  └─ H local /v4/listen
   │     └─ I local conversation processing
   └─ L local UX cleanup
```

Critical path for a usable first local release:

```text
A → B/C → D/E → F → G → L
```

Critical path for “true local continuous capture”:

```text
A → B/C → D/E → J → G → H → I → L
```

---

## First release definition (“Local MVP”)

Ship when all of the following are true:

1. Desktop app launches in `LOCAL_MODE` without Firebase/GCP.
2. User can record/transcribe push-to-talk locally.
3. Core local CRUD works:
   - conversations
   - memories
   - tasks
   - goals
4. Local chat can read local data.
5. Continuous conversation capture works at least in a simplified form **or** is explicitly marked beta.
6. No required cloud credentials remain for the default local path.

Nice to have, not blocking:
- semantic search parity
- speaker auto-assignment parity
- private cloud sync parity
- payment/subscription surfaces

---

## Major risks

### Risk 1: hidden Firebase assumptions in the app
Mitigation:
- add local-mode integration test checklist early
- centralize auth state branching in `AuthService` and startup

### Risk 2: trying to replace Firestore everywhere at once
Mitigation:
- extract repository boundaries by domain
- migrate only local-mode-required routes first

### Risk 3: blocking on perfect Deepgram parity
Mitigation:
- land PTT first
- treat continuous capture + diarization as separate milestones

### Risk 4: blocking on perfect Pinecone parity
Mitigation:
- ship lexical/FTS fallback first

### Risk 5: rewriting the Python processing pipeline too early
Mitigation:
- preserve Python logic, replace dependencies underneath it first

---

## Recommended execution order

### Wave order for implementation agents
1. Packet A
2. Packets B and C in parallel
3. Packets D and E in parallel
4. Packet F
5. Packet G
6. Packet L
7. Packet J
8. Packets H and K in parallel where possible
9. Packet I

This ordering gives a usable local desktop earlier, instead of waiting for full continuous-capture parity.

---

## Bottom line

A fully local Omi desktop is realistic, but **only if treated as a deliberate single-user product mode**, not as “run the cloud stack on localhost”.

The repo already contains the right leverage points:
- a substantial local Rust backend,
- local SQLite usage in the desktop app,
- existing local Whisper precedents elsewhere in the repo,
- and stable desktop API surfaces that can be preserved.

The hardest parts are:
1. removing Firebase assumptions,
2. replacing Firestore/GCS/Pinecone cleanly,
3. replacing the listen → pusher conversation-processing dependency.

If the work is split along the packets above, multiple models can implement it without stepping on each other, and you can reach a useful **Local MVP** before solving every cloud-era edge case.
