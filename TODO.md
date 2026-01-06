# Project TODO Checklist

## Completed

### Core Features
- [x] **Credit System Logic**: Database now tracks `credits` per API Key.
- [x] **Payment Integration**: Stripe payments (or top-ups) now automatically add credits.
- [x] **Admin Tools**: "God Mode" API endpoint created to manually add/refund credits.
- [x] **Configurable Pricing**: Pricing now managed via `server/pricing.json` with tiered bonuses and dynamic purchase amounts.
- [x] **Transaction History UI**: Realtime search/filter by activity, date, and amount with CSV export.
- [x] **Process Failure Refunds**: Include filename in transaction description for better debugging.
- [x] **Blender Remeshing Workflow**: Implemented `remesh.py` for high-quality retopology and baking.
- [x] **API Remesh Support**: Updated backend to handle `mode`, `faces`, and `texture_size` parameters.
- [x] **Web UI Enhancements**: Added Remeshing controls (Face count, Texture size) to the frontend.

### Critical Fixes
- [x] **Blender 4.x API Compatibility**: Updated `remesh.py` to use `temp_override()` instead of deprecated context dict pattern.
- [x] **OBJ Import API**: Changed from `bpy.ops.import_scene.obj` (removed in Blender 4.0) to `bpy.ops.wm.obj_import`.
- [x] **Output Validation**: Backend now verifies output file exists and has non-zero size before marking job complete.
- [x] **Credit Refunds on Empty Output**: Credits are now refunded if worker exits successfully but produces no output file.
- [x] **Explicit Exit Codes**: `remesh.py` now uses `sys.exit(0/1)` with try/except to ensure proper exit codes.
- [x] **Absolute Paths**: `remesh.py` now converts relative paths to absolute to avoid working directory issues.

### Security Hardening (Dec 2024)
- [x] **ADMIN_SECRET Required**: Server fails to start if not set (no more `"supersecret123"` default).
- [x] **ENCRYPTION_KEY Required**: `database.json` encrypted at rest with AES-256-GCM.
- [x] **CORS Restricted**: Replaced `.permissive()` with allowlist for webdeliveryengine.com domains.
- [x] **Admin Rate Limiting**: 5 requests/minute per IP on `/admin/*` endpoints.
- [x] **UUID Batch IDs**: Changed from timestamps to UUIDs (not guessable).
- [x] **1-Hour File Expiry**: Downloads auto-delete, docs updated from 24h to 1h.
- [x] **Input Validation**: File extensions (glb, gltf, obj, fbx, zip only), form field bounds enforced.
- [x] **Admin Endpoint Hardening**: Secret in `X-Admin-Secret` header, timing-safe comparison, audit logging.
- [x] **Job Persistence**: Jobs persist to SQLite, survive restarts with recovery logic.
- [x] **Capacity Monitoring**: Periodic stats logging, semaphore wait time warnings.

### Infrastructure (from launch_prep.md)
- [x] **Cron Environment Loading**: Fixed cron jobs to properly source `.env`.
- [x] **Upload Cleanup Automation**: Cron job cleans uploads every 15 minutes.
- [x] **Worker Slots Configuration**: Accepted default of 10 with monitoring.
- [x] **Disk Space Monitoring**: Added to health check.
- [x] **Blender Verification**: Added to health check.
- [x] **Error Handling in Cron**: Improved error handling in cleanup scripts.

---

## Pre-Launch Critical (Must Do)

### Testing
- [ ] **Test Backup System**: Run `setup.sh` and wait for first automated backup.
- [x] **Test Email Notifications**: Verify you receive success/failure emails from backup system.
- [x] **Verify Blender on Production**: Confirm Blender is installed and functional on server.
- [ ] **Test Cron Jobs Manually**: Run each script to verify it works.
- [ ] **Test Payment Success Page**: Verify API key displays with "SAVE YOUR KEY NOW" warning.
- [ ] **Test Key-Based History**: Verify transaction history loads correctly.
- [ ] **Test Transaction Search**: Verify realtime search/filter works.
- [ ] **Test CSV Download**: Verify proper formatting and filename.

### Security
- [x] **Remove Hardcoded Metrics Salt**: `METRICS_SALT` must not default to `"default-insecure-salt"`.
- [x] **Remove Email from Dockerfile**: Replace hardcoded `Brian@BrianGinn.com` in Caddyfile with env var.
- [ ] **Create dedicated deploy user**: Stop using root for deployments. Create `deploy` user with Docker group access, move project to `/home/deploy/mesh-optimizer`. (2025-12-22)

---

## Unit Tests (High Priority)

### Billing Logic (db.rs) 2025-12-18 run "cargo test -p mesh-api" on local dev machine to test. - Brian
- [x] Test credit deduction for decimate mode
- [x] Test credit deduction for remesh mode  
- [x] Test credit refund on job failure
- [x] Test free re-optimization (same file + mode within window)
- [x] Test insufficient credits rejection
- [x] Test pricing tier calculations

### Stripe Webhooks (main.rs)
- [ ] Test valid checkout.session.completed → credits added
- [ ] Test invalid webhook signature → rejected
- [ ] Test duplicate event ID → idempotent handling
- [ ] Test malformed payload → graceful failure

### API Key Validation
- [ ] Test valid key → authorized
- [ ] Test invalid key → rejected
- [ ] Test inactive/banned key → rejected

---

## High Priority (Before/Shortly After Launch)

### Infrastructure
- [ ] **Load Test with Large File**: Test 500MB FBX remesh, measure actual memory/time.
- [ ] **Verify Storage Box Connection**: Test SSH key authentication to Hetzner Storage Box. If SSH keys work, can remove `sshpass` from deploy.sh (2025-12-22)
- [ ] **Test Restore Procedure**: Perform test restore on staging/backup.
- [ ] **Uncomment Docker Stop/Start**: In `restore.sh` for proper restoration.
- [ ] **Add Python Dependency Check**: To `setup.sh`.
- [ ] **Configure Firewall Rules**: Ports 80, 443, 22.

### Resource Limits (Requires Testing)
- [ ] **Validate Job Timeout**: Current 600s (10 min) - test with largest expected files.
- [ ] **Upload Timeout**: Consider if slow uploads on large files need special handling.
- [ ] **Memory Limits**: Monitor actual memory usage during heavy remesh jobs vs 64GB available.
- [ ] **Document Real Limits**: After testing, update `hardware.md` with measured throughput.

---

## Medium Priority (First Week)

### DevOps
- [ ] **Set Up Log Rotation**: For `/var/log/mesh/*` files.
- [ ] **Add Backup Encryption**: GPG encryption for backups.
- [ ] **Fix Mac/Linux Date Compatibility**: In backup scripts.
- [ ] **Create Quick-Update Script** (`update_config.sh`):
    1. Rsync only `.env` file to server
    2. Restart Docker container to load new variables
    3. Remove robots.txt to allow indexing
    4. Remove test key

### Documentation
- [x] ~~**Review and Update Pricing**: In `Capabilities.md`.~~ Replaced with dynamic `pricing.json` system.
- [ ] **Document Disaster Recovery**: Procedures for restore.
- [ ] **Create Internal vs External Docs**: Separation.

---

## Future Features / Nice to Have

### Monitoring
- [ ] **Create Status Dashboard**: Simple web page for system health.
- [ ] **Add Prometheus/Grafana**: For detailed metrics.
- [ ] **Implement Webhook Notifications**: Beyond email alerts.
- [ ] **Add Health Check HTTP Endpoint**: `/health` for uptime monitoring.
- [ ] **Daily Hardware Usage Report**: Know when hardware is maxed out.

### User Experience
- [x] **Persist API Key in localStorage**: Implemented - pre-populates in purchase modal.
- [ ] **Password-Type API Key Input**: Prevent shoulder-surfing.
- [ ] **Credit Balance Dashboard**: Simple page for users.
- [ ] **Subscriptions**: Monthly unlimited or refill for "whale" clients.

### Performance
- [ ] **Enable GPU Support**: Docker `--gpus all` for Blender cycles baking.
- [x] **Tiered Pricing Logic**: Implemented via `pricing.json` with configurable bonus tiers and live UI calculator.

### Configuration Cleanup
- [ ] **Consolidate COST_DECIMATE/COST_REMESH into pricing.json**: These are customer-facing credit costs and should be hot-reloadable like other pricing settings. Currently in `.env`, should move to `pricing.json`. Note: `SLOT_COST_*` variables should stay in `.env` as they are server infrastructure settings (semaphore capacity), not pricing.

### Features
- [ ] **Add More 3D Formats**: STEP, STL, etc.
- [ ] **User Documentation**: Public-facing guides.

---

---

## Launch Operations Playbook

### What You Have Now

**Admin Endpoints (require `X-Admin-Secret` header + `secret` in body):**
- `POST /admin/add-credits` - Add/remove credits from existing key
- `POST /admin/create-key` - Create new API key with initial credits

**Database Structure:**
- `database.json` (encrypted) - Keys, emails, Stripe customer IDs, credit balances, active status
- `stats.db` (SQLite) - Transaction history, job logs

**Key Facts:**
- Each API key has an `active: bool` field (can be used for banning)
- Keys are tied to email addresses
- One email can technically have multiple keys (via multiple Stripe purchases)
- Credits live on the KEY, not the email

---

### Common Operations

#### 1. Give Free Credits to Discord Users

**Option A: They already have a key** (bought before)
```bash
curl -X POST https://webdeliveryengine.com/admin/add-credits \
  -H "Content-Type: application/json" \
  -d '{"key": "sk_their_key", "amount": 50, "secret": "YOUR_ADMIN_SECRET"}'
```

**Option B: Create a fresh key with free credits** (new user, no payment)
```bash
curl -X POST https://webdeliveryengine.com/admin/create-key \
  -H "Content-Type: application/json" \
  -d '{"email": "user@example.com", "initial_credits": 50, "secret": "YOUR_ADMIN_SECRET"}'
```
Returns: `{"success": true, "api_key": "sk_...", "credits": 50}`

Give them that key. They paste it in the UI and go.

#### 2. Ban a Key

**Not yet implemented.** Need to add endpoint. Workaround:
```bash
# SSH into server, manually edit database after decryption
# Or: set credits to -99999 so they can't do anything
curl -X POST https://webdeliveryengine.com/admin/add-credits \
  -H "Content-Type: application/json" \
  -d '{"key": "sk_bad_actor", "amount": -99999, "secret": "YOUR_ADMIN_SECRET"}'
```

**TODO:** Add `POST /admin/ban-key` endpoint that sets `active: false`

#### 3. User Has Two Keys (Consolidate?)

This happens if they:
- Bought twice without entering existing key
- Used different emails

**Current behavior:** Each key is independent. Credits don't merge.

**Options:**
1. Tell them to use whichever key has more credits
2. Manually add credits to one key, zero out the other
3. **TODO:** Build a key merge feature (complex - need to update transaction history)

#### 4. User Lost Their Key

They need to prove ownership:
1. Ask for their email
2. SSH into server, grep the decrypted database for their email
3. Find their key, send it to them securely

**TODO:** Add `POST /admin/lookup-key-by-email` endpoint

#### 5. Check a User's Balance/Status

**Not yet implemented as endpoint.** Workaround:
```bash
# User can check via API:
curl -H "Authorization: Bearer sk_their_key" https://webdeliveryengine.com/credits
```

**TODO:** Add `POST /admin/get-key-info` endpoint for admin to look up any key

---

### Discord Promo: Self-Service Free Key Page

**Your idea:** Secret standalone HTML page for instant key generation.

**Pros:**
- Zero friction for Discord users
- No email back-and-forth

**Cons:**
- Can be abused (bots, multiple signups)
- No email = can't contact users later
- Hard to track who got promo credits

**Recommendation:** Require email, but make it instant.

**Implementation Plan:**
1. Create `/promo.html` (or `/discord.html`) - hidden, not linked anywhere
2. Simple form: just email input
3. On submit, hits a new endpoint `POST /promo/claim`
4. Backend checks:
   - Email not already used for promo (prevent duplicates)
   - Optional: rate limit by IP
5. Creates key with X free credits, returns it immediately
6. Logs promo claims for tracking

**Simpler alternative:** Just use `/admin/create-key` yourself for each Discord user who DMs you. Manual but controlled.

---

### Admin Endpoints TODO

- [ ] `POST /admin/ban-key` - Set key `active: false`
- [ ] `POST /admin/unban-key` - Set key `active: true`
- [ ] `POST /admin/lookup-key-by-email` - Find key(s) for an email
- [ ] `POST /admin/get-key-info` - Get full info for a key (email, credits, created, active)
- [ ] `POST /admin/list-keys` - List all keys (paginated) for admin dashboard
- [ ] `POST /promo/claim` - Self-service promo key generation (with email)
- [ ] `POST /admin/delete-user` - GDPR-compliant user deletion: sets `active=false`, clears `email` field. Transaction records in SQLite remain with opaque `user_key` (already pseudonymized via `user_hash`). Keeps audit trail intact while removing PII from `database.json`.

---

## MCP Connector GitHub Release Setup

**Status:** Workflow created, needs GitHub authentication to push and trigger.

### What's Done
- [x] `optimize_batch` tool implemented in Rust (`crates/mcp_server/src/tools/batch.rs`)
- [x] GitHub Actions workflow created (`.github/workflows/release-mcp.yml`)
- [x] Workflow committed to local git server

### What's Needed

#### 1. Configure GitHub Authentication (choose one)

**Option A: GitHub CLI (recommended)**
```bash
brew install gh
gh auth login
```

**Option B: Personal Access Token**
1. Go to https://github.com/settings/tokens
2. Generate new token (classic) with `repo` scope
3. Use as password when pushing

**Option C: SSH Key**
```bash
ssh-keygen -t ed25519 -C "your-email@example.com"
cat ~/.ssh/id_ed25519.pub
# Add output to: https://github.com/settings/ssh/new
```

#### 2. Push to GitHub
```bash
git push github main
```

#### 3. Create a Release (triggers cross-platform builds)
```bash
git tag mcp-v0.1.0
git push github mcp-v0.1.0
```

This triggers GitHub Actions to build binaries for:
- macOS ARM64 (Apple Silicon M1/M2/M3)
- macOS x64 (Intel)
- Linux x64
- Windows x64

All binaries are automatically attached to the GitHub release.

#### 4. Update MCP Modal Download Link
After first release, verify the link in `server/public/index.html` points to correct repo:
```
https://github.com/usermac/mesh-optimizer/releases/latest
```
(Currently points to `brianginn/meshopt-mcp-server` - needs updating)

### Files Involved
- `.github/workflows/release-mcp.yml` - GitHub Actions workflow
- `crates/mcp_server/` - MCP connector source code
- `server/public/index.html` - Download link in MCP modal

---

## Quick Commands

### Add to deploy.sh
```bash
ssh root@your-server "bash /root/mesh-optimizer/scripts/backup/setup.sh"
```

### Manual Backup
```bash
ssh root@your-server "bash /root/mesh-optimizer/scripts/backup/backup.sh"
```

### Blender Health Check
```bash
./scripts/reports/blender_health_check.sh
```

---

## Environment Variables Checklist

Required for production (server will fail without these):
- `STRIPE_SECRET_KEY`
- `STRIPE_WEBHOOK_SECRET`
- `RESEND_API_KEY`
- `ADMIN_SECRET` - Strong unique secret for admin endpoints
- `ENCRYPTION_KEY` - 32 bytes as 64 hex chars (generate: `openssl rand -hex 32`)
- `METRICS_SALT` - For pseudonymizing user data in logs (generate: `openssl rand -hex 16`)

Optional:
- `ACME_EMAIL` - Email for Let's Encrypt SSL certificate registration (used by Caddy)
- `WORKER_SLOTS` - Default 10
- `SLOT_COST_DECIMATE` - Default 1
- `SLOT_COST_REMESH` - Default 5
- `BLENDER_PATH` - Default "blender"

Deprecated (no longer used - pricing now in `server/pricing.json`):
- ~~`CREDIT_COST`~~ - Replaced by `pricing.json`
- ~~`CREDIT_INCREMENT`~~ - Replaced by `pricing.json`
- ~~`CREDIT_FREE_SPIN`~~ - Replaced by `pricing.json` (`free_reoptimization_hours`)
