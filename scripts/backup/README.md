# Mesh Optimizer - Backup System Documentation

## 📦 Overview

This automated backup system protects your Mesh Optimizer and Listmonk databases by:

- **Backing up** `stats.db` (SQLite) and `database.json` every 6 hours
- **Backing up** Listmonk PostgreSQL database daily
- **Storing backups** locally (7 days) and on Hetzner Storage Box (30 days)
- **Sending email notifications** via Resend API on success/failure
- **Verifying integrity** with checksums and archive validation
- **Providing easy restoration** with dedicated restore scripts
- **Single-user export** for Listmonk subscriber data recovery

---

## 🚀 Quick Start

### 1. Configure Environment Variables

Add these to `/root/mesh-optimizer/.env`:

```bash
# Storage Box Configuration
STORAGE_BOX_USER=u518013
STORAGE_BOX_HOST=u518013.your-storagebox.de
STORAGE_BOX_PATH=/backups

# Email Configuration (you already have these)
RESEND_API_KEY=re_your_key_here
BACKUP_EMAIL=your-email@example.com
```

### 2. Run Setup Script

```bash
cd /root/mesh-optimizer
bash scripts/backup/setup.sh
```

This will:
- ✅ Verify your configuration
- ✅ Create necessary directories
- ✅ Set up cron jobs
- ✅ Test Storage Box connection
- ✅ Run a test backup
- ✅ Send you a test email

### 3. You're Done!

**Mesh Optimizer backups** run every 6 hours:
- 00:00 (midnight)
- 06:00 (6 AM)
- 12:00 (noon)
- 18:00 (6 PM)

**Listmonk backups** run daily:
- 03:00 (3 AM)

---

## 📚 Scripts Reference

### `backup.sh` - Main Backup Script

**Purpose:** Creates compressed backups and uploads to Storage Box

**Usage:**
```bash
bash /root/mesh-optimizer/scripts/backup/backup.sh
```

**What it does:**
1. Backs up `stats.db` and `database.json`
2. Creates SHA256 checksums
3. Compresses to `.tar.gz` archive
4. Stores locally in `/root/backups`
5. Uploads to Hetzner Storage Box
6. Sends email notification
7. Cleans up old backups

**Retention:**
- Local: 7 days
- Storage Box: 30 days

---

### `restore.sh` - Database Restore Script

**Purpose:** Restore databases from backup

**Usage:**

List available backups:
```bash
bash /root/mesh-optimizer/scripts/backup/restore.sh
```

List backups on Storage Box:
```bash
bash /root/mesh-optimizer/scripts/backup/restore.sh storage-box
```

Restore specific backup:
```bash
bash /root/mesh-optimizer/scripts/backup/restore.sh 20250108_120000
```

Restore latest backup:
```bash
bash /root/mesh-optimizer/scripts/backup/restore.sh latest
```

**What it does:**
1. Creates safety backup of current databases (in `/root/backups/pre-restore`)
2. Downloads backup from Storage Box if not local
3. Verifies backup integrity
4. Extracts and restores database files
5. Sets proper permissions

**Safety Features:**
- Always creates pre-restore backup
- Requires confirmation (`yes`) before restoring
- Verifies checksums before restoration
- Logs all actions

---

### `listmonk-backup.sh` - Listmonk PostgreSQL Backup

**Purpose:** Backs up the Listmonk newsletter database (PostgreSQL)

**Usage:**
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-backup.sh
```

**What it does:**
1. Runs `pg_dump` on the `listmonk_db` Docker container
2. Compresses to `.sql.gz` file
3. Stores locally in `/root/backups/listmonk`
4. Uploads to Hetzner Storage Box (`/backups/listmonk/`)
5. Sends email notification with subscriber count
6. Cleans up old backups

**Schedule:** Daily at 3:00 AM

**Retention:**
- Local: 7 days
- Storage Box: 30 days

---

### `listmonk-restore.sh` - Listmonk Restore & User Export

**Purpose:** Restore Listmonk database or export individual subscriber data

**Usage:**

List available backups:
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh
```

List backups on Storage Box:
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh storage-box
```

Restore full database:
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh 20250108_030000
```

Restore latest backup:
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh latest
```

**Export single user's data** (from latest backup):
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh user john@example.com
```

**Export single user from specific backup:**
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh user john@example.com 20250108_030000
```

**What the user export includes:**
- Subscriber record (email, name, attributes, status, created_at)
- List memberships (which lists they subscribed to)
- Campaign views (which emails they opened)
- Link clicks (which links they clicked)

**Output:** JSON file at `/root/backups/listmonk/user-exports/`

**Safety Features:**
- Creates pre-restore backup before full restore
- Requires confirmation (`yes`) before full restore
- User export uses temporary database (non-destructive)
- Stops/starts Listmonk container during full restore

---

### `verify_backup.sh` - Backup Verification Script

**Purpose:** Test backup integrity without restoring

**Usage:**

Verify all backups:
```bash
bash /root/mesh-optimizer/scripts/backup/verify_backup.sh
```

Verify specific backup:
```bash
bash /root/mesh-optimizer/scripts/backup/verify_backup.sh 20250108_120000
```

Verify latest backup:
```bash
bash /root/mesh-optimizer/scripts/backup/verify_backup.sh latest
```

**What it checks:**
- File exists and is not corrupted
- Archive can be extracted
- Required files (`stats.db`) are present
- Checksums match original
- File sizes are reasonable

**Email report:** Sends summary to `BACKUP_EMAIL`

---

## 📧 Email Notifications

You'll receive emails for:

### Success Emails (every 6 hours)
Subject: `✅ Database Backup Successful - YYYYMMDD_HHMMSS`

Contains:
- Timestamp
- Backup size
- Files included
- Storage locations
- Backup name

### Failure Emails (immediate)
Subject: `❌ Database Backup FAILED - YYYYMMDD_HHMMSS`

Contains:
- Error message
- Timestamp
- Instructions to SSH and investigate
- Log file location

### Verification Reports (weekly)
Subject: `✅ Backup Verification Passed` or `⚠️ Backup Verification Issues Detected`

Contains:
- Total backups checked
- Passed/Failed counts
- Detailed report

---

## 📁 Directory Structure

```
/root/
├── backups/                           # Local backup storage
│   ├── mesh-backup-20250108_000000.tar.gz
│   ├── mesh-backup-20250108_060000.tar.gz
│   ├── pre-restore/                   # Safety backups before restore
│   │   └── before-restore-20250108_123000.tar.gz
│   │
│   └── listmonk/                      # Listmonk backups
│       ├── listmonk-backup-20250108_030000.sql.gz
│       ├── pre-restore/               # Safety backups before restore
│       │   └── before-restore-20250108_123000.sql.gz
│       └── user-exports/              # Single-user data exports
│           └── john_example_com_20250108_030000.json
│
├── mesh-optimizer/
│   ├── server/
│   │   ├── stats.db                   # Main SQLite database
│   │   └── database.json              # Legacy JSON database
│   │
│   └── scripts/backup/
│       ├── backup.sh                  # Main backup script
│       ├── restore.sh                 # Restore script
│       ├── listmonk-backup.sh         # Listmonk backup script
│       ├── listmonk-restore.sh        # Listmonk restore script
│       ├── verify_backup.sh           # Verification script
│       ├── setup.sh                   # Setup script
│       └── README.md                  # This file
│
└── /var/log/mesh/
    ├── backup.log                     # Mesh backup logs
    ├── listmonk-backup.log            # Listmonk backup logs
    ├── restore.log                    # Restore logs
    └── verify.log                     # Verification logs
```

**Storage Box Structure:**
```
u518013.your-storagebox.de:/backups/
├── mesh-backup-20250101_000000.tar.gz
├── mesh-backup-20250101_060000.tar.gz
├── mesh-backup-20250101_120000.tar.gz
├── ... (30 days of mesh backups)
│
└── listmonk/
    ├── listmonk-backup-20250101_030000.sql.gz
    ├── listmonk-backup-20250102_030000.sql.gz
    └── ... (30 days of listmonk backups)
```

---

## 🔧 Manual Operations

### Run Backup Manually

```bash
cd /root/mesh-optimizer
export $(cat .env | grep -v '^#' | xargs)
bash scripts/backup/backup.sh
```

### Check Backup Logs

```bash
# View full log
cat /var/log/mesh/backup.log

# Follow in real-time
tail -f /var/log/mesh/backup.log

# View only errors
grep ERROR /var/log/mesh/backup.log
```

### View Cron Schedule

```bash
crontab -l
```

### Edit Cron Schedule

```bash
crontab -e
```

### Download Backup from Storage Box

```bash
# List backups
ssh -p 23 u518013@u518013.your-storagebox.de "ls -lh /backups/"

# Download specific backup
scp -P 23 u518013@u518013.your-storagebox.de:/backups/mesh-backup-20250108_120000.tar.gz /root/backups/
```

### Manually Extract Backup (for inspection)

```bash
cd /root/backups
tar -xzf mesh-backup-20250108_120000.tar.gz
cd mesh-backup-20250108_120000
ls -lh
# You'll see: stats.db, database.json, checksums.txt
```

---

## 🚨 Emergency Procedures

### Scenario 1: Server Crash / Data Loss

**If your server dies and you need to recover everything:**

1. **Set up new server** (or repair existing)

2. **Deploy your application:**
   ```bash
   git clone your-repo
   # or restore from your local git backups
   ```

3. **Restore latest backup from Storage Box:**
   ```bash
   # Set up SSH key for Storage Box
   ssh-keygen -t ed25519
   # Add public key to Storage Box

   # Download latest backup
   scp -P 23 u518013@u518013.your-storagebox.de:/backups/mesh-backup-*.tar.gz /root/

   # Extract
   tar -xzf mesh-backup-*.tar.gz

   # Copy databases
   mkdir -p /root/mesh-optimizer/server
   cp mesh-backup-*/stats.db /root/mesh-optimizer/server/
   cp mesh-backup-*/database.json /root/mesh-optimizer/server/
   ```

4. **Verify data:**
   ```bash
   ls -lh /root/mesh-optimizer/server/
   # Should show stats.db and database.json
   ```

5. **Start your application**

---

### Scenario 2: Accidental Data Deletion

**If you accidentally delete or corrupt the database:**

1. **Stop the Docker container** (optional, to prevent further changes):
   ```bash
   docker stop api
   ```

2. **List available backups:**
   ```bash
   bash /root/mesh-optimizer/scripts/backup/restore.sh
   ```

3. **Restore desired backup:**
   ```bash
   bash /root/mesh-optimizer/scripts/backup/restore.sh 20250108_120000
   # Type 'yes' to confirm
   ```

4. **Restart application:**
   ```bash
   docker start api
   ```

5. **Verify restoration:**
   ```bash
   ls -lh /root/mesh-optimizer/server/
   # Check file sizes and timestamps
   ```

---

### Scenario 3: Listmonk Database Recovery

**If Listmonk database is corrupted or needs restoration:**

1. **List available Listmonk backups:**
   ```bash
   bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh
   ```

2. **Restore the database:**
   ```bash
   bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh 20250108_030000
   # Type 'yes' to confirm
   ```

3. **Verify restoration:**
   ```bash
   docker exec listmonk_db psql -U listmonk -d listmonk -c "SELECT COUNT(*) FROM subscribers;"
   ```

---

### Scenario 4: Export Single Subscriber's Data (GDPR Request)

**If a user requests their data or you need to recover a specific subscriber:**

1. **Export user data from latest backup:**
   ```bash
   bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh user john@example.com
   ```

2. **Or from a specific backup:**
   ```bash
   bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh user john@example.com 20250108_030000
   ```

3. **Find the exported JSON:**
   ```bash
   ls -la /root/backups/listmonk/user-exports/
   cat /root/backups/listmonk/user-exports/john_example_com_*.json
   ```

The export includes: subscriber info, list memberships, campaign opens, and link clicks.

---

### Scenario 5: Test Restore (Monthly Recommended)

**You should test restores regularly to ensure backups work!**

1. **Verify backups are valid:**
   ```bash
   bash /root/mesh-optimizer/scripts/backup/verify_backup.sh
   ```

2. **Download a backup to your local Mac:**
   ```bash
   scp -P 23 u518013@u518013.your-storagebox.de:/backups/mesh-backup-20250108_120000.tar.gz ~/Downloads/
   ```

3. **Extract and inspect:**
   ```bash
   cd ~/Downloads
   tar -xzf mesh-backup-20250108_120000.tar.gz
   cd mesh-backup-20250108_120000
   ls -lh
   # Verify stats.db and database.json are present
   ```

4. **Check checksums:**
   ```bash
   shasum -a 256 -c checksums.txt
   # Should show "OK" for all files
   ```

---

## ⚙️ Configuration Options

### Change Backup Frequency

Edit crontab:
```bash
crontab -e
```

Examples:
```bash
# Every 3 hours
0 */3 * * * bash /root/mesh-optimizer/scripts/backup/backup.sh

# Every hour
0 * * * * bash /root/mesh-optimizer/scripts/backup/backup.sh

# Daily at 2 AM only
0 2 * * * bash /root/mesh-optimizer/scripts/backup/backup.sh
```

### Change Retention Period

Edit the scripts:

**Local retention** (in `backup.sh`):
```bash
LOCAL_RETENTION_DAYS=7  # Change to desired days
```

**Storage Box retention** (in `backup.sh`):
```bash
REMOTE_RETENTION_DAYS=30  # Change to desired days
```

### Change Storage Box Path

In `/root/mesh-optimizer/.env`:
```bash
STORAGE_BOX_PATH=/my-custom-path
```

---

## 🔍 Troubleshooting

### Backup fails with "Permission denied"

**Fix:**
```bash
chmod +x /root/mesh-optimizer/scripts/backup/*.sh
chmod 755 /root/backups
chmod 755 /var/log/mesh
```

### Can't connect to Storage Box

**Check SSH key:**
```bash
ssh -p 23 u518013@u518013.your-storagebox.de
# Should connect without password
```

**If fails, verify:**
- SSH key is added to Storage Box (in Hetzner Robot panel)
- Hostname is correct in `.env`
- Storage Box is active (check Hetzner account)

### Email not sending

**Verify Resend API key:**
```bash
curl -X POST https://api.resend.com/emails \
  -H "Authorization: Bearer $RESEND_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "from": "test@webdeliveryengine.com",
    "to": ["your-email@example.com"],
    "subject": "Test",
    "html": "<p>Test email</p>"
  }'
```

### Cron job not running

**Check cron logs:**
```bash
# On Debian/Ubuntu
grep CRON /var/log/syslog

# View your cron jobs
crontab -l

# Check if cron service is running
systemctl status cron
```

### Backup file too large

If your database grows beyond 10-100MB:
- Storage Box handles this fine (you have 1TB!)
- Email won't include attachments (it already doesn't with Storage Box)
- Backups will just take longer to upload

---

## 📊 Monitoring

### Check Backup Status

```bash
# Last backup
ls -lth /root/backups/*.tar.gz | head -5

# Backup log summary
tail -50 /var/log/mesh/backup.log | grep -E "Starting|completed|ERROR"

# Count backups
echo "Local: $(ls /root/backups/*.tar.gz 2>/dev/null | wc -l) backups"
```

### Storage Usage

```bash
# Local backup size
du -sh /root/backups

# Storage Box usage
ssh -p 23 u518013@u518013.your-storagebox.de "du -sh /backups"
```

### Test Email Notifications

```bash
# Set environment
export $(cat /root/mesh-optimizer/.env | grep -v '^#' | xargs)

# Run backup manually and watch for email
bash /root/mesh-optimizer/scripts/backup/backup.sh
```

---

## 🎯 Best Practices

1. **Test restores monthly** - Backups are worthless if you can't restore!

2. **Keep backups in multiple locations:**
   - ✅ Server local (7 days)
   - ✅ Storage Box (30 days)
   - ✅ Your Mac (manual downloads occasionally)

3. **Monitor email notifications** - Set up email filters so you notice failures

4. **Verify backups weekly** - The cron job does this automatically

5. **Document credentials** - Store Storage Box and Resend credentials securely

6. **Keep this README updated** - If you change configuration, update docs

---

## 📞 Support

If you encounter issues:

1. **Check logs:** `/var/log/mesh/backup.log`
2. **Run verification:** `bash scripts/backup/verify_backup.sh`
3. **Test manually:** `bash scripts/backup/backup.sh`
4. **Check Storage Box:** SSH in and verify files exist

---

## 🎉 You're Protected!

Your databases are now:
- ✅ **Mesh Optimizer** backed up every 6 hours automatically
- ✅ **Listmonk** backed up daily at 3 AM
- ✅ Stored safely off-site (Hetzner Storage Box)
- ✅ Monitored via email notifications
- ✅ Verified weekly for integrity
- ✅ Easy to restore in emergencies
- ✅ Single-user export available for Listmonk subscribers

Sleep well knowing your data is safe! 🛡️

---

**Last Updated:** January 2025  
**Version:** 1.0  
**Maintainer:** Mesh Optimizer Team