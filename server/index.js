const express = require("express");
const multer = require("multer");
const { exec } = require("child_process");
const path = require("path");
const fs = require("fs");

const app = express();

// 1. INCREASE TIMEOUT (Crucial for 1GB)
// 20 Minutes (in milliseconds)
// If upload takes longer than this, Node cuts it off.
const TIMEOUT_MS = 20 * 60 * 1000;
// 1. Ensure uploads directory exists
const uploadDir = path.join(__dirname, "uploads");
if (!fs.existsSync(uploadDir)) fs.mkdirSync(uploadDir);

// 2. CONFIGURE MULTER LIMITS
const upload = multer({
  dest: uploadDir,
  limits: {
    fileSize: 1024 * 1024 * 1024 * 2, // Limit to 2GB (Safety buffer)
    fieldSize: 1024 * 1024 * 1024 * 2,
  },
});
app.use(require("cors")());

// --- THE BOUNCER (Authentication Middleware) ---
const validKeys = new Set([
  "sk_test_123", // Hardcoded key for testing
  "sk_live_abc", // You can add more later
]);

const authenticate = (req, res, next) => {
  // 1. Get the header
  const authHeader = req.headers["authorization"];

  // 2. Check if it exists and looks like "Bearer sk_..."
  if (!authHeader || !authHeader.startsWith("Bearer ")) {
    return res.status(401).json({ error: "Missing or malformed API Key" });
  }

  // 3. Extract the token
  const token = authHeader.split(" ")[1];

  // 4. Validate
  if (validKeys.has(token)) {
    next(); // Come on in!
  } else {
    return res.status(403).json({ error: "Invalid API Key" });
  }
};

app.post("/optimize", authenticate, upload.single("file"), (req, res) => {
  if (!req.file) return res.status(400).json({ error: "No file" });

  // Filenames (e.g. "a1b2c3d4")
  const inputFilename = req.file.filename;
  const outputFilename = `${req.file.filename}_opt.glb`;

  // Absolute Paths (for cleanup later)
  const absoluteInputPath = path.join(uploadDir, inputFilename);
  const absoluteOutputPath = path.join(uploadDir, outputFilename);

  // COMMAND: Run binary using simple filenames, but force CWD to uploads folder
  const command = `mesh-optimizer --input "${inputFilename}" --output "${outputFilename}" --ratio 0.1`;

  console.log(`[EXEC] ${command} (in ${uploadDir})`);

  const execOptions = {
    cwd: uploadDir, // Crucial: Run AS IF we are inside the uploads folder
  };

  exec(command, execOptions, (error, stdout, stderr) => {
    // Log Rust output
    if (stdout) console.log(`[RUST LOG] ${stdout}`);
    if (stderr) console.error(`[RUST ERR] ${stderr}`);

    // Cleanup INPUT file immediately
    fs.unlink(absoluteInputPath, () => {});

    if (error) {
      console.error(`Exec Error: ${error}`);
      return res
        .status(500)
        .json({ error: "Optimization Process Failed", stderr });
    }

    // Check if OUTPUT file exists
    if (fs.existsSync(absoluteOutputPath)) {
      console.log(`Success! Sending ${outputFilename}`);
      res.download(absoluteOutputPath, "optimized.glb", (err) => {
        if (err) console.error("Send Error:", err);
        // Cleanup OUTPUT file after sending
        fs.unlink(absoluteOutputPath, () => {});
      });
    } else {
      console.error("Rust finished but output file is missing.");
      res.status(500).json({ error: "Output file missing", logs: stdout });
    }
  });
});

const PORT = 3000;
app.listen(PORT, () => console.log(`Server running on port ${PORT}`));
// 3. APPLY TIMEOUT TO SERVER
server.setTimeout(TIMEOUT_MS);
