#!/bin/bash
set -e

SERVER="root@webdeliveryengine.com"

echo "Building blog search index..."
node scripts/build-blog-index.js

# If --index-only flag is passed, stop here (used by deploy.sh)
if [[ "$1" == "--index-only" ]]; then
    echo "Blog index built (index-only mode)"
    exit 0
fi

echo "Syncing blog files to prod..."
rsync -avz server/public/blog/ $SERVER:/root/mesh-optimizer/server/public/blog/

echo "Blog deployed!"
