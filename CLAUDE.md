# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Mesh Optimizer is a 3D mesh optimization service that processes GLB, GLTF, OBJ, FBX, and ZIP files for web/mobile delivery. It provides both a web UI and API for asset processing.

## Build & Development Commands

```bash
# Build the entire workspace (API server + worker binary)
cargo build --release

# Build individual crates
cargo build -p mesh-api --release
cargo build -p mesh-worker --release

# Run locally (requires .env file - copy from .env.example)
cargo run -p mesh-api

# Deploy to production
./deploy.sh
```

## Architecture

### Rust Workspace (`crates/`)

**`crates/api`** - Axum-based HTTP server (`mesh-api` binary)
- `main.rs`: Routes, handlers, job orchestration, Stripe integration
- `db.rs`: Database abstraction (SQLite + encrypted JSON), transaction history, job persistence
- Key routes: `/optimize` (multipart upload), `/job/:id` (status polling), `/create-checkout-session`, `/webhook`, `/admin/*`

**`crates/worker`** - CLI mesh processing tool (`mesh-worker` binary, deployed as `mesh-optimizer`)
- Loads OBJ/GLB/GLTF/FBX models
- Uses `meshopt` crate for decimation (polygon reduction)
- Outputs optimized GLB files

### Blender Scripts (`scripts/`)

- `remesh.py`: Blender Python script for QuadriFlow remeshing + normal/diffuse baking. Invoked by API for complex optimization jobs.
- `glb_to_usdz.py`: GLB to USDZ conversion via Blender

### Web Frontend (`server/public/`)

- `index.html`: Main application UI (vanilla JS, inline CSS)
- Communicates with API via fetch, handles file uploads, job polling, Stripe checkout

### Configuration

- `server/pricing.json`: Hot-reloadable pricing tiers (base rate, bonuses, limits)
- `.env`: Required secrets (see Environment Variables below)

## Processing Modes

1. **Decimate**: Fast polygon reduction using `mesh-worker` (Rust)
2. **Remesh**: High-quality retopology via Blender's QuadriFlow + texture baking (`remesh.py`)

Jobs are queued, processed with semaphore-controlled concurrency, and results expire after 1 hour.

## Key Patterns

- **Job IDs**: UUIDs stored in `uploads/{batch_id}/`
- **Database**: Dual storage - SQLite for transactions/jobs, AES-256-GCM encrypted JSON for API keys/customers
- **Credit System**: API key-based, credits deducted per job, refunded on failure
- **Admin Endpoints**: Rate-limited (5/min/IP), require `X-Admin-Secret` header with timing-safe comparison

## Frontend UI Patterns (`server/public/index.html`)

- **Dropzones**: Main dropzone for 3D files uses fixed `min-height` to prevent layout shift after file drop. Filename/size displayed inside the dropzone (not below it).
- **Mini Dropzones**: MTL and texture inputs use `.dropzone-mini` class - same drag/drop + click behavior as main dropzone, but smaller.
- **OBJ-specific inputs**: The `#objInputs` section (MTL + textures dropzones) only appears when a `.obj` file is dropped. Hidden for GLB, GLTF, FBX, and ZIP files.
- **Contact Form**: Category dropdown + freeform subject field. Email subject formatted as `[Category Text] User Subject`. Enterprise inquiry option at top of category list.

## Environment Variables

Required (server fails without):
- `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`
- `RESEND_API_KEY`
- `ADMIN_SECRET` - for `/admin/*` endpoints
- `ENCRYPTION_KEY` - 64 hex chars for database.json encryption (`openssl rand -hex 32`)
- `METRICS_SALT` - for pseudonymizing logs

Optional:
- `WORKER_SLOTS` (default: 10)
- `SLOT_COST_DECIMATE` (default: 1), `SLOT_COST_REMESH` (default: 5)
- `BLENDER_PATH` (default: "blender")
- `ACME_EMAIL` - for Let's Encrypt via Caddy

## Deployment

Docker-based deployment via `deploy.sh`:
1. Rsyncs code to production server
2. Builds Docker image (Rust binaries + Blender 4.1)
3. Runs with Caddy reverse proxy for HTTPS
4. Mounts: `database.json`, `stats.db`, `pricing.json`, Caddy data volume
