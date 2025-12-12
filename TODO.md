# Project TODO Checklist

## Completed
- [x] **Credit System Logic**: Database now tracks `credits` per API Key.
- [x] **Payment Integration**: Stripe payments (or top-ups) now automatically add credits.
- [x] **Admin Tools**: "God Mode" API endpoint created to manually add/refund credits.
- [x] **Configurable Pricing**: `CREDIT_COST` and `CREDIT_INCREMENT` are now loaded from `.env`.
- [x] **Transaction History UI**: Realtime search/filter by activity, date, and amount with CSV export.
- [x] **Process Failure Refunds**: Include filename in transaction description for better debugging.
- [x] **Blender Remeshing Workflow**: Implemented `remesh.py` for high-quality retopology and baking.
- [x] **API Remesh Support**: Updated backend to handle `mode`, `faces`, and `texture_size` parameters.
- [x] **Web UI Enhancements**: Added Remeshing controls (Face count, Texture size) to the frontend.

## Testing Required
- [ ] **Test Payment Success Page**: Verify the enhanced success page displays the API key prominently with the "SAVE YOUR KEY NOW" warning. Confirm users can copy the key easily.
- [ ] **Test Key-Based History**: Verify users can paste their API key in the Web UI and see their transaction history correctly.
- [ ] **Test Transaction Search**: Verify realtime search/filter works correctly for activity descriptions, dates, and amounts.
- [ ] **Test CSV Download**: Verify CSV download works with proper formatting and filename (transaction-history-YYYY-MM-DD.csv).

## Security (Critical for Production)
- [ ] **Remove Hardcoded Admin Secret**: `ADMIN_SECRET` must not default to `"supersecret123"`. Make it required and fail loudly if not set.
- [ ] **Remove Hardcoded Metrics Salt**: `METRICS_SALT` must not default to `"default-insecure-salt"`. Make it required in production.
- [ ] **Remove Email from Dockerfile**: Replace hardcoded `Brian@BrianGinn.com` in Caddyfile with an environment variable.
- [ ] **Encrypt JSON Key Store**: The `server/db.json` contains unencrypted API keys. Either encrypt at rest or migrate entirely to SQLite.
- [ ] **Fix CORS Configuration**: Replace `.permissive()` with specific domain whitelist to prevent CSRF attacks.
- [ ] **Reduce Upload Limit**: Lower from 5GB to a reasonable limit (e.g., 500MB-1GB) to prevent DoS attacks.
- [ ] **Add Rate Limiting**: Implement rate limiting on all endpoints (especially `/optimize`, `/admin/*`) to prevent abuse.
- [ ] **Secure File Downloads**: Add authentication checks to `/download/{batch_id}/{filename}` to prevent unauthorized access.

- [ ] **Add Request Validation**: Validate all user inputs before processing (file types, sizes, query parameters).

## Post-Launch / DevOps
- [ ] **Create Quick-Update Script**: Write a lightweight shell script (e.g., `update_config.sh`) that:
    1. Rsyncs *only* the `.env` file to the server.
    2. Restarts the Docker container (using the existing image) to load new variables.
    *Goal: Allow changing `CREDIT_COST` or `CREDIT_INCREMENT` in seconds without waiting for a full `ship.sh` Docker rebuild.*
    3. Remove robots.txt to allow indexing
    4. remove test key
- [ ] **Enable GPU Support**: Configure Docker to pass through NVIDIA/AMD GPUs (using `--gpus all`) to accelerate Blender cycles baking.
- [ ] **Tiered Pricing Logic**: Update transaction logic to deduct higher credit amounts (e.g., 5) for computational heavy "remesh" operations compared to standard decimation.


## Future Features / UX
- [ ] **Web UI**: Persist the API Key in browser `localStorage` so users don't have to paste it on every refresh.
- [ ] **Web UI**: Change API Key input type to `password` to prevent shoulder-surfing.
- [ ] **Dashboard**: Simple page for users to check their credit balance.
- [ ] **Subscriptions**: Support for recurring billing (monthly unlimited or refill) for "whale" clients.
- [ ] **DAILY Usage for Hardware**: I need to know when my hardware is being used and not living up to the work ask.


## add to ship.sh
# Add this line to your ship.sh if it's not already there:
ssh root@your-server "bash /root/mesh-optimizer/scripts/backup/setup.sh"

## just for me to use anytime
ssh root@your-server "bash /root/mesh-optimizer/scripts/backup/backup.sh"
"✅ Blender health check passed" message:
```bash
./scripts/reports/blender_health_check.sh
