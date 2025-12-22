#!/bin/sh
# Start Caddy in background, then run API
# Using exec ensures mesh-api receives signals directly
caddy start --config /etc/caddy/Caddyfile
exec mesh-api
