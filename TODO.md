# Project TODO Checklist

## Completed
- [x] **Credit System Logic**: Database now tracks `credits` per API Key.
- [x] **Payment Integration**: Stripe payments (or top-ups) now automatically add credits.
- [x] **Admin Tools**: "God Mode" API endpoint created to manually add/refund credits.
- [x] **Configurable Pricing**: `CREDIT_COST` and `CREDIT_INCREMENT` are now loaded from `.env`.

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