# Project TODO Checklist

## Completed

### Core Features
- [x] **Credit System Logic**: Database now tracks `credits` per API Key.
- [x] **Payment Integration**: Stripe payments (or top-ups) now automatically add credits.
- [x] **Admin Tools**: "God Mode" API endpoint created to manually add/refund credits.
- [x] **Configurable Pricing**: `CREDIT_COST` and `CREDIT_INCREMENT` are now loaded from `.env`.
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
- [ ] **Test Email Notifications**: Verify you receive success/failure emails from backup system.
- [ ] **Verify Blender on Production**: Confirm Blender is installed and functional on server.
- [ ] **Test Cron Jobs Manually**: Run each script to verify it works.
- [ ] **Test Payment Success Page**: Verify API key displays with "SAVE YOUR KEY NOW" warning.
- [ ] **Test Key-Based History**: Verify transaction history loads correctly.
- [ ] **Test Transaction Search**: Verify realtime search/filter works.
- [ ] **Test CSV Download**: Verify proper formatting and filename.

### Security
- [x] **Remove Hardcoded Metrics Salt**: `METRICS_SALT` must not default to `"default-insecure-salt"`.
- [x] **Remove Email from Dockerfile**: Replace hardcoded `Brian@BrianGinn.com` in Caddyfile with env var.

---

## High Priority (Before/Shortly After Launch)

### Infrastructure
- [ ] **Load Test with Large File**: Test 500MB FBX remesh, measure actual memory/time.
- [ ] **Verify Storage Box Connection**: Test SSH key authentication to Hetzner Storage Box.
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

### Features
- [ ] **Add More 3D Formats**: STEP, STL, etc.
- [ ] **User Documentation**: Public-facing guides.

---

## Quick Commands

### Add to ship.sh
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