# Project TODO Checklist

## Completed
- [x] **Credit System Logic**: Database now tracks `credits` per API Key.
- [x] **Payment Integration**: Stripe payments (or top-ups) now automatically add credits.
- [x] **Admin Tools**: "God Mode" API endpoint created to manually add/refund credits.
- [x] **Configurable Pricing**: `CREDIT_COST` and `CREDIT_INCREMENT` are now loaded from `.env`.

## Security (Critical for Production)
- [ ] **Remove Hardcoded Admin Secret**: `ADMIN_SECRET` must not default to `"supersecret123"`. Make it required and fail loudly if not set.
- [ ] **Remove Hardcoded Metrics Salt**: `METRICS_SALT` must not default to `"default-insecure-salt"`. Make it required in production.
- [ ] **Remove Email from Dockerfile**: Replace hardcoded `Brian@BrianGinn.com` in Caddyfile with an environment variable.
- [ ] **Encrypt JSON Key Store**: The `server/db.json` contains unencrypted API keys. Either encrypt at rest or migrate entirely to SQLite.
- [ ] **Fix CORS Configuration**: Replace `.permissive()` with specific domain whitelist to prevent CSRF attacks.
- [ ] **Reduce Upload Limit**: Lower from 5GB to a reasonable limit (e.g., 500MB-1GB) to prevent DoS attacks.
- [ ] **Add Rate Limiting**: Implement rate limiting on all endpoints (especially `/optimize`, `/admin/*`) to prevent abuse.
- [ ] **Secure File Downloads**: Add authentication checks to `/download/{batch_id}/{filename}` to prevent unauthorized access.
- [ ] **Rotate TEST_KEY Regularly**: Ensure `TEST_KEY` is not exposed and rotated periodically.
- [ ] **Add Request Validation**: Validate all user inputs before processing (file types, sizes, query parameters).

## Post-Launch / DevOps
- [ ] **Create Quick-Update Script**: Write a lightweight shell script (e.g., `update_config.sh`) that:
    1. Rsyncs *only* the `.env` file to the server.
    2. Restarts the Docker container (using the existing image) to load new variables.
    *Goal: Allow changing `CREDIT_COST` or `CREDIT_INCREMENT` in seconds without waiting for a full `ship.sh` Docker rebuild.*

## Future Features / UX
- [ ] **Web UI**: Persist the API Key in browser `localStorage` so users don't have to paste it on every refresh.
- [ ] **Web UI**: Change API Key input type to `password` to prevent shoulder-surfing.
- [ ] **Dashboard**: Simple page for users to check their credit balance.
- [ ] **Subscriptions**: Support for recurring billing (monthly unlimited or refill) for "whale" clients.