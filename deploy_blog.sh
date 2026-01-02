#!/bin/bash
set -e  # Exit immediately if any command fails

SERVER="root@webdeliveryengine.com"

echo "Building blog search index..."
node scripts/build-blog-index.js

echo "Syncing blog files to prod..."
rsync -avz server/public/blog/ $SERVER:/root/mesh-optimizer/server/public/blog/

echo "Blog deployed!"
