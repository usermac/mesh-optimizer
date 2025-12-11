# 🎉 Backup System - Ready to Deploy!

## 📦 What's Been Created

I've built you a **complete, production-ready backup system** with:

### Scripts Created:
1. **`scripts/backup/backup.sh`** - Automated backup with Storage Box upload
2. **`scripts/backup/restore.sh`** - Emergency database restoration
3. **`scripts/backup/verify_backup.sh`** - Backup integrity checking
4. **`scripts/backup/setup.sh`** - One-command installation
5. **`scripts/backup/README.md`** - Complete documentation (579 lines!)
6. **`scripts/backup/QUICKSTART.md`** - 5-minute setup guide
7. **`scripts/backup/env-example.txt`** - Environment variable template

### Features:
- ✅ **Automatic backups** every 6 hours (00:00, 06:00, 12:00, 18:00)
- ✅ **Dual storage** - Local (7 days) + Storage Box (30 days)
- ✅ **Email notifications** via Resend (success/failure/weekly reports)
- ✅ **Integrity verification** with SHA256 checksums
- ✅ **Easy restoration** with safety backups before restore
- ✅ **Comprehensive logging** to `/var/log/mesh/`
- ✅ **Automatic cleanup** of old backups

---

## 🚀 Deployment Instructions

### Step 1: Add Environment Variables

SSH into your server and edit `.env`:

```bash
ssh root@webdeliveryengine.com
nano /root/mesh-optimizer/.env
```

Add these lines:

```bash
# Storage Box Configuration
STORAGE_BOX_USER=u518013
STORAGE_BOX_HOST=u518013.your-storagebox.de
STORAGE_BOX_PATH=/backups

# Email Configuration (if not already present)
RESEND_API_KEY=re_your_actual_key_here
BACKUP_EMAIL=your-email@example.com
```

Save and exit.

### Step 2: Deploy from Local Machine

From your Mac:

```bash
cd ~/Documents/ZedDocs/mesh-code/mesh-optimizer
./ship.sh
```

This will sync all backup scripts to your server.

### Step 3: Run Setup on Server

SSH back in and run setup:

```bash
ssh root@webdeliveryengine.com
cd /root/mesh-optimizer
bash scripts/backup/setup.sh
```

The setup script will:
- Verify configuration
- Create directories
- Test Storage Box connection
- Set up cron jobs
- Run a test backup
- Send you a test email

**Check your inbox for the backup notification!**

---

## 📧 What to Expect

### Email Notifications

You'll receive emails at `BACKUP_EMAIL` for:

**Every 6 hours (after each backup):**
- Subject: `✅ Database Backup Successful - YYYYMMDD_HHMMSS`
- Contains: Backup size, files backed up, storage locations

**Immediately on failure:**
- Subject: `❌ Database Backup FAILED - YYYYMMDD_HHMMSS`
- Contains: Error details, instructions to investigate

**Weekly (Sunday 2 AM):**
- Subject: `✅ Backup Verification Passed` or `⚠️ Backup Verification Issues`
- Contains: Verification report for all backups

---

## 📁 Where Backups Are Stored

### Local Server
**Location:** `/root/backups/`  
**Retention:** 7 days  
**Format:** `mesh-backup-YYYYMMDD_HHMMSS.tar.gz`

**Example:**
```
/root/backups/
├── mesh-backup-20250108_000000.tar.gz  (12 KB)
├── mesh-backup-20250108_060000.tar.gz  (12 KB)
├── mesh-backup-20250108_120000.tar.gz  (12 KB)
└── mesh-backup-20250108_180000.tar.gz  (12 KB)
```

### Hetzner Storage Box
**Location:** `u518013.your-storagebox.de:/backups/`  
**Retention:** 30 days  
**Access:** SSH/SFTP on port 23

**To view:**
```bash
ssh -p 23 u518013@u518013.your-storagebox.de "ls -lh /backups/"
```

---

## 🔧 Quick Commands Reference

### Manual Operations

```bash
# Run backup manually
bash /root/mesh-optimizer/scripts/backup/backup.sh

# List available backups
bash /root/mesh-optimizer/scripts/backup/restore.sh

# Restore specific backup
bash /root/mesh-optimizer/scripts/backup/restore.sh 20250108_120000

# Restore latest backup
bash /root/mesh-optimizer/scripts/backup/restore.sh latest

# Verify all backups
bash /root/mesh-optimizer/scripts/backup/verify_backup.sh

# Check backup logs
tail -f /var/log/mesh/backup.log

# View cron schedule
crontab -l
```

### Storage Box Operations

```bash
# Connect to Storage Box
ssh -p 23 u518013@u518013.your-storagebox.de

# List backups on Storage Box
ssh -p 23 u518013@u518013.your-storagebox.de "ls -lh /backups/"

# Download backup to server
scp -P 23 u518013@u518013.your-storagebox.de:/backups/mesh-backup-*.tar.gz /root/backups/

# Check Storage Box disk usage
ssh -p 23 u518013@u518013.your-storagebox.de "du -sh /backups"
```

---

## 🚨 Emergency Recovery Procedures

### If Server Crashes

1. **Set up new server or repair existing**

2. **Download latest backup from Storage Box:**
   ```bash
   scp -P 23 u518013@u518013.your-storagebox.de:/backups/mesh-backup-*.tar.gz /root/
   ```

3. **Extract backup:**
   ```bash
   tar -xzf mesh-backup-*.tar.gz
   ```

4. **Copy databases:**
   ```bash
   mkdir -p /root/mesh-optimizer/server
   cp mesh-backup-*/stats.db /root/mesh-optimizer/server/
   cp mesh-backup-*/database.json /root/mesh-optimizer/server/
   ```

5. **Redeploy application and start**

### If Database Gets Corrupted

1. **Stop application (optional):**
   ```bash
   docker stop api
   ```

2. **List backups:**
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

---

## 📊 Monitoring Your Backups

### Check Backup Status

```bash
# Last 5 backups
ls -lth /root/backups/*.tar.gz | head -5

# Backup log summary (last 50 lines)
tail -50 /var/log/mesh/backup.log | grep -E "Starting|completed|ERROR"

# Count local backups
ls /root/backups/*.tar.gz 2>/dev/null | wc -l
```

### Storage Usage

```bash
# Local backup directory size
du -sh /root/backups

# Storage Box usage
ssh -p 23 u518013@u518013.your-storagebox.de "du -sh /backups"

# Server disk space
df -h
```

---

## 🧪 Testing Your Backups

**You MUST test restores regularly!** Backups are worthless if they don't work.

### Monthly Test Procedure:

1. **Verify backup integrity:**
   ```bash
   bash /root/mesh-optimizer/scripts/backup/verify_backup.sh
   ```

2. **Download a backup to your Mac:**
   ```bash
   scp -P 23 u518013@u518013.your-storagebox.de:/backups/mesh-backup-*.tar.gz ~/Downloads/
   ```

3. **Extract and inspect locally:**
   ```bash
   cd ~/Downloads
   tar -xzf mesh-backup-*.tar.gz
   cd mesh-backup-*
   ls -lh
   # Should see: stats.db, database.json, checksums.txt
   ```

4. **Verify checksums:**
   ```bash
   shasum -a 256 -c checksums.txt
   # Should show "OK" for all files
   ```

---

## ⚙️ Customization Options

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

# Current (every 6 hours)
0 */6 * * * bash /root/mesh-optimizer/scripts/backup/backup.sh
```

### Change Retention Periods

Edit `scripts/backup/backup.sh`:
```bash
LOCAL_RETENTION_DAYS=7    # Change to keep more/fewer days locally
REMOTE_RETENTION_DAYS=30  # Change to keep more/fewer days on Storage Box
```

---

## 📝 What Each Backup Contains

Each backup archive (`mesh-backup-YYYYMMDD_HHMMSS.tar.gz`) contains:

```
mesh-backup-20250108_120000/
├── stats.db           # SQLite database (36 KB)
├── database.json      # Legacy JSON database (423 bytes)
└── checksums.txt      # SHA256 checksums for verification
```

**Total compressed size:** ~10-15 KB (very small!)

---

## 💰 Costs

**Hetzner Storage Box BX11:**
- **Size:** 1 TB
- **Cost:** €6.36/month (~$7 USD)
- **Usage:** ~30-60 KB per day for backups (you'll use < 1% of space)

**Resend Email:**
- Free tier: 100 emails/day
- You'll send: ~5-10 emails/day
- **Cost:** Free

**Total monthly cost:** ~$7

---

## ✅ Post-Deployment Checklist

After running `setup.sh`, verify:

- [ ] Received test backup email
- [ ] Backup exists in `/root/backups/`
- [ ] Backup uploaded to Storage Box (check via SSH)
- [ ] Cron jobs are installed (`crontab -l`)
- [ ] Log files created (`ls /var/log/mesh/`)
- [ ] Can connect to Storage Box (`ssh -p 23 u518013@u518013.your-storagebox.de`)

---

## 🎯 Best Practices

1. **Check your email regularly** for backup notifications
2. **Test restore monthly** - Don't wait for an emergency!
3. **Keep Storage Box credentials secure** - Store in password manager
4. **Monitor disk space** on server and Storage Box
5. **Download backups to your Mac occasionally** (extra safety)
6. **Update documentation** if you change configuration

---

## 🆘 Troubleshooting

### Setup fails with "Storage Box not configured"
- Check `.env` file has correct values
- Verify `STORAGE_BOX_USER` and `STORAGE_BOX_HOST` are set

### Can't connect to Storage Box
- Test: `ssh -p 23 u518013@u518013.your-storagebox.de`
- Verify SSH key is added in Hetzner Robot panel
- Check Storage Box is active in your Hetzner account

### Email not sending
- Verify `RESEND_API_KEY` in `.env` is correct
- Check API key is active in Resend dashboard
- Test with curl (see README.md for command)

### Cron not running
- Check cron service: `systemctl status cron`
- View logs: `grep CRON /var/log/syslog`
- Verify crontab: `crontab -l`

---

## 📚 Documentation

Full documentation available at:
- **Complete Guide:** `scripts/backup/README.md` (579 lines)
- **Quick Start:** `scripts/backup/QUICKSTART.md`
- **Environment Template:** `scripts/backup/env-example.txt`

Read online:
```bash
cat /root/mesh-optimizer/scripts/backup/README.md | less
```

---

## 🎉 You're Protected!

Once deployed, your Mesh Optimizer database is:

✅ **Automatically backed up** every 6 hours  
✅ **Stored off-site** on Hetzner Storage Box (safe from server failure)  
✅ **Monitored** with email alerts on success/failure  
✅ **Verified** weekly for integrity  
✅ **Easy to restore** in emergencies  
✅ **Logged** for audit trail  

**Sleep well knowing your data is safe!** 🛡️

---

## 🚀 Next Steps

1. **Deploy the backup system** (follow instructions above)
2. **Set up monitoring** (health checks, metrics, status page) - We can discuss this next!
3. **Configure logging** (persistent Docker logs)
4. **Add alerts** (RAM/disk/job failure notifications)

**Ready to proceed with monitoring/logging setup?** Let me know!

---

**Created:** January 2025  
**Version:** 1.0  
**Status:** Ready to Deploy ✅