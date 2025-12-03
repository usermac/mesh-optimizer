const express = require('express');
const multer = require('multer');
const { exec } = require('child_process');
const path = require('path');
const fs = require('fs');

const app = express();
// 1. Ensure uploads directory exists
const uploadDir = path.join(__dirname, 'uploads');
if (!fs.existsSync(uploadDir)) fs.mkdirSync(uploadDir);

const upload = multer({ dest: uploadDir });
app.use(require('cors')());

app.post('/optimize', upload.single('file'), (req, res) => {
    if (!req.file) return res.status(400).json({ error: 'No file' });

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
        cwd: uploadDir // Crucial: Run AS IF we are inside the uploads folder
    };

    exec(command, execOptions, (error, stdout, stderr) => {
        // Log Rust output
        if (stdout) console.log(`[RUST LOG] ${stdout}`);
        if (stderr) console.error(`[RUST ERR] ${stderr}`);

        // Cleanup INPUT file immediately
        fs.unlink(absoluteInputPath, () => {});

        if (error) {
            console.error(`Exec Error: ${error}`);
            return res.status(500).json({ error: 'Optimization Process Failed', stderr });
        }

        // Check if OUTPUT file exists
        if (fs.existsSync(absoluteOutputPath)) {
            console.log(`Success! Sending ${outputFilename}`);
            res.download(absoluteOutputPath, 'optimized.glb', (err) => {
                if (err) console.error("Send Error:", err);
                // Cleanup OUTPUT file after sending
                fs.unlink(absoluteOutputPath, () => {});
            });
        } else {
            console.error("Rust finished but output file is missing.");
            res.status(500).json({ error: 'Output file missing', logs: stdout });
        }
    });
});

const PORT = 3000;
app.listen(PORT, () => console.log(`Server running on port ${PORT}`));
