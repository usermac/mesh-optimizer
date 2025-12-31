# 🚀 Backup System Quick Start

**Get your databases protected in 5 minutes!**

Covers both **Mesh Optimizer** (SQLite) and **Listmonk** (PostgreSQL) backups.

---

## ✅ Prerequisites

- [x] Hetzner Storage Box ordered (`u518013`)
- [x] SSH key added to Storage Box
- [x] Storage Box connection tested
- [x] Resend API key configured
- [x] Email address for notifications

---

## 📝 Step 1: Add Environment Variables

SSH into your server:
```bash
ssh root@webdeliveryengine.com
```

Edit your `.env` file:
```bash
nano /root/mesh-optimizer/.env
```

Add these lines at the bottom:
```bash
# Storage Box Configuration
STORAGE_BOX_USER=u518013
STORAGE_BOX_HOST=u518013.your-storagebox.de
STORAGE_BOX_PATH=/backups

# Email Configuration (if not already present)
RESEND_API_KEY=re_your_actual_key_here
BACKUP_EMAIL=your-email@example.com
```

Save and exit (`Ctrl+X`, then `Y`, then `Enter`)

---

## 🚀 Step 2: Deploy Backup Scripts

From your **local Mac**, deploy the backup system:

```bash
cd ~/Documents/ZedDocs/mesh-code/mesh-optimizer
./deploy.sh
```

This will sync the backup scripts to your server.

---

## ⚙️ Step 3: Run Setup

SSH back into your server:
```bash
ssh root@webdeliveryengine.com
```

Run the setup script:
```bash
cd /root/mesh-optimizer
bash scripts/backup/setup.sh
```

The setup will:
- ✅ Verify your configuration
- ✅ Create backup directories
- ✅ Test Storage Box connection
- ✅ Set up automated cron jobs
- ✅ Run a test backup
- ✅ Send you a test email

**Check your inbox for the backup notification!**

---

## 🎉 You're Done!

**Mesh Optimizer** is backed up every 6 hours:
- **00:00** (midnight)
- **06:00** (6 AM)
- **12:00** (noon)
- **18:00** (6 PM)

**Listmonk** is backed up daily:
- **03:00** (3 AM)

Backups are stored:
- **Locally:** `/root/backups` and `/root/backups/listmonk` (kept 7 days)
- **Storage Box:** `u518013.your-storagebox.de:/backups` (kept 30 days)

You'll receive email notifications for:
- ✅ Every successful backup (mesh + listmonk)
- ❌ Any backup failures
- 📊 Weekly verification reports

---

## 🔧 Quick Commands

### Mesh Optimizer

**Run backup manually:**
```bash
bash /root/mesh-optimizer/scripts/backup/backup.sh
```

**List available backups:**
```bash
bash /root/mesh-optimizer/scripts/backup/restore.sh
```

**Restore a backup:**
```bash
bash /root/mesh-optimizer/scripts/backup/restore.sh 20250108_120000
```

### Listmonk

**Run backup manually:**
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-backup.sh
```

**List available backups:**
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh
```

**Restore a backup:**
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh 20250108_030000
```

**Export single user's data:**
```bash
bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh user john@example.com
```

### General

**Check logs:**
```bash
tail -f /var/log/mesh/backup.log          # Mesh
tail -f /var/log/mesh/listmonk-backup.log # Listmonk
```

**View cron schedule:**
```bash
crontab -l
```

---

## 📚 Need More Help?

Read the full documentation:
```bash
cat /root/mesh-optimizer/scripts/backup/README.md
```

Or view it locally:
```bash
open scripts/backup/README.md
```

---

## 🆘 Troubleshooting

### "Storage Box connection failed"
```bash
# Test connection manually
ssh -p 23 u518013@u518013.your-storagebox.de

# If fails, check SSH key in Hetzner Robot panel
```

### "Email not sending"
```bash
# Verify RESEND_API_KEY in .env
cat /root/mesh-optimizer/.env | grep RESEND

# Test email manually
curl -X POST https://api.resend.com/emails \
  -H "Authorization: Bearer YOUR_KEY" \
  -H "Content-Type: application/json" \
  -d '{"from":"test@webdeliveryengine.com","to":["your@email.com"],"subject":"Test","html":"<p>Works!</p>"}'
```

### "Cron not running"
```bash
# Check cron service
systemctl status cron

# View cron logs
grep CRON /var/log/syslog | tail -20
```

---

## ✨ That's It!

Your Mesh Optimizer and Listmonk databases are now protected. Sleep well! 🛡️

**Next steps:**
1. Check your email for the first backup notification
2. Test a restore next week to verify everything works
3. Monitor backup logs occasionally
4. Use single-user export if you need to recover subscriber data

**Questions?** Check the full README.md for detailed documentation.
