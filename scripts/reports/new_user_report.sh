#!/bin/bash

# This script generates a report of new user signups and their purchased credits
# (excluding free credits) over the last 24 hours.
# It queries the production database, which is expected to be at server/stats.db
# relative to the project root.

# Ensure the script is run from the project root
cd "$(dirname "$0")/../.."

python3 -c "
import sqlite3
import time
import os

db_path = 'server/stats.db'

# Check if the database file exists
if not os.path.exists(db_path):
    print(f\"Error: Database file not found at {db_path}\")
    print(\"Please run this script from the project's root directory.\")
    exit(1)

now_ms = int(time.time() * 1000)
past_24h_ms = now_ms - 86400000

try:
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()
    cursor.execute(\"\"\"
        SELECT COUNT(DISTINCT user_key), SUM(amount)
        FROM credit_transactions
        WHERE created_at >= ?
          AND description != 'free_initial_credits'
          AND amount > 0
    \"\"\", (past_24h_ms,))
    result = cursor.fetchone()
    conn.close()
    users = result[0] if result and result[0] is not None else 0
    credits = result[1] if result and result[1] is not None else 0
    print(f\"{users} new signups with {credits} credits purchased\")
except sqlite3.OperationalError as e:
    print(f\"An error occurred: {e}\")
    print(\"0 new signups with 0 credits purchased\")
"
