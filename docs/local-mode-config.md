# Local Mode Configuration

Local Mode (`LOCAL_MODE`) provides a zero-setup way to run the Omi Desktop app by skipping all local service startup. Instead, the app connects directly to URLs you provide in your `.env` file.

## When to Use Local Mode

- You want to run the app without building/starting the Rust backend, Python auth service, or Cloudflare tunnel
- You have a remote backend already running (e.g., production, a dev VPS, or Cloud Run)
- You want faster iteration when working on UI changes that don't require backend changes

## How It Works

Set `LOCAL_MODE=1` in `desktop/Backend-Rust/.env`. When run.sh detects this flag, it:

1. Sets `OMI_SKIP_BACKEND=1` — skips Rust backend build and startup
2. Sets `OMI_SKIP_AUTH=1` — skips Python auth service startup
3. Sets `OMI_SKIP_TUNNEL=1` — skips Cloudflare tunnel creation

The app then reads the service URLs from your `.env` file and connects directly.

## Required Environment Variables

When using Local Mode, ensure your `.env` file has these three URLs set:

```bash
# Backend API — the Rust desktop-backend server
OMI_API_URL=https://your-backend-url.run.app

# Python backend — handles subscriptions, payments, transcription, etc.
OMI_PYTHON_API_URL=https://api.omi.me

# Auth backend — handles OAuth sign-in
OMI_AUTH_URL=https://your-auth-url.run.app/
```

## Enabling Local Mode

1. Add `LOCAL_MODE=1` to your `desktop/Backend-Rust/.env`:

```bash
# ─── Local Mode ──────────────────────────────────────────────────────
LOCAL_MODE=1

# ─── Required ────────────────────────────────────────────────────────
OMI_API_URL=https://your-backend-url.run.app
OMI_PYTHON_API_URL=https://api.omi.me
OMI_AUTH_URL=https://your-auth-url.run.app/
```

2. Run the app:

```bash
cd desktop
./run.sh
```

The script will print a clear banner indicating Local Mode is active:

```
==========================================
  LOCAL MODE — using URLs from .env
==========================================

  Skipping: Rust backend, Python auth, Cloudflare tunnel
  Using OMI_API_URL=https://your-backend-url.run.app

==========================================
```

## Comparing Dev Modes

| Mode | Backend | Auth | Tunnel | Use Case |
|------|---------|------|--------|----------|
| Full local | Builds & runs locally | Runs on :10200 | Creates tunnel | Full local dev |
| `--yolo` | Uses prod Cloud Run | Uses prod auth | Skipped | Quick test, no setup |
| `LOCAL_MODE=1` | Uses your `.env` URL | Uses your `.env` URL | Skipped | Iterating on UI with remote services |

## Relationship to Individual Skip Flags

`LOCAL_MODE=1` is equivalent to setting all three skip flags:

```bash
OMI_SKIP_BACKEND=1
OMI_SKIP_AUTH=1
OMI_SKIP_TUNNEL=1
```

You can also set these individually without `LOCAL_MODE` if you only want to skip specific services while keeping others running locally.
