const express = require("express");
const multer = require("multer");
const { exec } = require("child_process");
const path = require("path");
const fs = require("fs");
const cors = require("cors");

const app = express();

// 1. Setup Uploads
const uploadDir = path.join(__dirname, "uploads");
if (!fs.existsSync(uploadDir)) fs.mkdirSync(uploadDir);

// 2. High Limits (2GB / 20 Mins)
const TIMEOUT_MS = 20 * 60 * 1000;
const upload = multer({
  dest: uploadDir,
  limits: { fileSize: 1024 * 1024 * 1024 * 2 },
});

app.use(cors());

// NEW: Serve static files (The UI) from the 'public' folder
app.use(express.static("public"));

// --- THE BOUNCER ---
const validKeys = new Set(["sk_test_123", "sk_live_abc"]);

const authenticate = (req, res, next) => {
  // Check Header
  const authHeader = req.headers["authorization"];
  if (authHeader && authHeader.startsWith("Bearer ")) {
    const token = authHeader.split(" ")[1];
    if (validKeys.has(token)) return next();
  }

  // Check URL Param (for easy browser testing: ?key=sk_test_123)
  if (req.query.key && validKeys.has(req.query.key)) return next();

  return res.status(401).json({ error: "Invalid or Missing API Key" });
};

// --- THE ENDPOINT ---
app.post("/optimize", authenticate, upload.single("file"), (req, res) => {
  if (!req.file) return res.status(400).json({ error: "No file" });

  // 1. INPUTS
  const inputFilename = req.file.filename;
  const outputFilename = `${req.file.filename}_opt.glb`;

  const absoluteInputPath = path.join(uploadDir, inputFilename);
  const absoluteOutputPath = path.join(uploadDir, outputFilename);

  // 2. PARSE RATIO (The New Logic)
  // Front-end sends 0-100. We convert to 0.0-1.0. Default to 0.5.
  let userRatio = parseFloat(req.body.ratio);
  if (isNaN(userRatio) || userRatio <= 0 || userRatio > 1) {
    userRatio = 0.5; // Default safe value
  }
  console.log(
    `[JOB] File: ${req.file.originalname} | Size: ${req.file.size} | Ratio: ${userRatio}`,
  );

  // 3. RUN COMMAND
  const command = `mesh-optimizer --input "${inputFilename}" --output "${outputFilename}" --ratio ${userRatio}`;

  const execOptions = { cwd: uploadDir };

  exec(command, execOptions, (error, stdout, stderr) => {
    // Log Rust output
    if (stdout) console.log(`[RUST LOG] ${stdout}`);

    // Cleanup Input
    fs.unlink(absoluteInputPath, () => {});

    if (error) {
      console.error(`[EXEC ERR] ${stderr}`);
      return res
        .status(500)
        .json({ error: "Optimization Failed", details: stderr });
    }

    if (fs.existsSync(absoluteOutputPath)) {
      // Send File
      res.download(absoluteOutputPath, "optimized.glb", (err) => {
        if (err) console.error("Send Error:", err);
        fs.unlink(absoluteOutputPath, () => {}); // Cleanup Output
      });
    } else {
      res.status(500).json({ error: "Output file missing", logs: stdout });
    }
  });
});

const PORT = 3000;
const server = app.listen(PORT, () =>
  console.log(`Server running on port ${PORT}`),
);
server.setTimeout(TIMEOUT_MS);
