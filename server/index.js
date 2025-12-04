const express = require("express");
const multer = require("multer");
const { exec } = require("child_process");
const path = require("path");
const fs = require("fs");
const cors = require("cors");
// Make sure you created db.js in the previous step!
const db = require("./db");

// --- CONFIGURATION ---
// REPLACE with your Stripe Secret Key (sk_test_...)
const stripe = require("stripe")(
  "sk_test_51OoumnD2a0WQ2ytfq0fpUoxpoe4VUhGt6JECIxGCmqtwQPHVTbOCNaPmSifRDeNYLMpLqRQ5l8HyVXTJAtidkLzg0093vaPiAQ",
);
// REPLACE with your Stripe Webhook Secret (whsec_...)
const WEBHOOK_SECRET = "whsec_UJLaOJGaFq1cqIUrQkx2xe8itJL0lzw5";

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

// 3. Middleware
// Webhook needs raw body
app.use("/webhook", express.raw({ type: "application/json" }));
// Everything else needs JSON
app.use(express.json());
app.use(express.static("public"));
app.use(cors());

// --- THE BOUNCER (Authentication) ---
const authenticate = (req, res, next) => {
  let token = null;
  const authHeader = req.headers["authorization"];

  // 1. Check Header
  if (authHeader && authHeader.startsWith("Bearer ")) {
    token = authHeader.split(" ")[1];
  }
  // 2. Check Query Param (for browser testing)
  else if (req.query.key) {
    token = req.query.key;
  }

  // Special Backdoor for testing (You can remove this later)
  if (token === "sk_test_123") return next();

  // Check DB
  if (token && db.isValidKey(token)) {
    return next();
  }

  return res
    .status(401)
    .json({ error: "Invalid API Key. Please purchase a license." });
};

// --- PAYMENT ROUTES ---

// 1. Create Checkout Session
app.post("/create-checkout-session", async (req, res) => {
  console.log("Starting Checkout Session..."); // NEW LOG
  try {
    const session = await stripe.checkout.sessions.create({
      payment_method_types: ["card"],
      line_items: [
        {
          price_data: {
            currency: "usd",
            product_data: { name: "MeshOpt Pro License" },
            unit_amount: 4900,
          },
          quantity: 1,
        },
      ],
      mode: "payment",
      success_url:
        "https://webdeliveryengine.com/success?session_id={CHECKOUT_SESSION_ID}",
      cancel_url: "https://webdeliveryengine.com/",
    });

    console.log("Session Created:", session.url); // NEW LOG
    res.json({ url: session.url });
  } catch (e) {
    console.error("STRIPE FATAL ERROR:", e); // NEW LOG
    res.status(500).json({ error: e.message, type: e.type });
  }
});

// 2. Stripe Webhook
app.post("/webhook", async (req, res) => {
  const sig = req.headers["stripe-signature"];
  let event;

  try {
    event = stripe.webhooks.constructEvent(req.body, sig, WEBHOOK_SECRET);
  } catch (err) {
    console.error(`Webhook Error: ${err.message}`);
    return res.status(400).send(`Webhook Error: ${err.message}`);
  }

  if (event.type === "checkout.session.completed") {
    const session = event.data.object;
    const customerEmail = session.customer_details.email;
    const customerId = session.customer;

    console.log(`💰 Payment received from ${customerEmail}`);

    // Generate and Save Key
    const newKey = db.createKey(customerEmail, customerId);
    console.log(`🔑 Generated Key: ${newKey}`);
  }

  res.json({ received: true });
});

// 3. Success Page
app.get("/success", async (req, res) => {
  if (!req.query.session_id) return res.redirect("/");

  try {
    const session = await stripe.checkout.sessions.retrieve(
      req.query.session_id,
    );
    const email = session.customer_details.email;

    // Find key in DB
    const localDb = db.load();
    const entry = Object.entries(localDb.keys).find(
      ([k, v]) => v.email === email,
    );
    const myKey = entry ? entry[0] : "Key generated (Check email)";

    res.send(`
            <html><body style="font-family:sans-serif; background:#111; color:white; text-align:center; padding:50px;">
                <h1 style="color:#10b981">Payment Successful!</h1>
                <p>Thank you ${email}</p>
                <p>Here is your API Key:</p>
                <div style="background:#333; padding:20px; font-size:24px; font-family:monospace; border-radius:10px; display:inline-block; border: 1px solid #555;">
                    ${myKey}
                </div>
                <p style="color:#aaa">Save this key.</p>
                <a href="/" style="color:#3b82f6; text-decoration:none; margin-top:20px; display:inline-block;">&larr; Back to Dashboard</a>
            </body></html>
        `);
  } catch (e) {
    console.error("Retrieval Error:", e);
    res.status(500).send(`Error retrieving session: ${e.message}`);
  }
});

// --- OPTIMIZE ROUTE ---
app.post("/optimize", authenticate, upload.single("file"), (req, res) => {
  if (!req.file) return res.status(400).json({ error: "No file" });

  // 1. INPUTS
  const inputFilename = req.file.filename;
  const outputFilename = `${req.file.filename}_opt.glb`;
  const absoluteInputPath = path.join(uploadDir, inputFilename);
  const absoluteOutputPath = path.join(uploadDir, outputFilename);

  // 2. PARSE RATIO
  let userRatio = parseFloat(req.body.ratio);
  if (isNaN(userRatio) || userRatio <= 0 || userRatio > 1) userRatio = 0.5;

  // 3. RUN COMMAND
  // Note: We use the global binary name 'mesh-optimizer'
  const command = `mesh-optimizer --input "${inputFilename}" --output "${outputFilename}" --ratio ${userRatio}`;

  console.log(`[EXEC] ${command} (in ${uploadDir})`);

  const execOptions = { cwd: uploadDir };

  exec(command, execOptions, (error, stdout, stderr) => {
    // Cleanup Input
    fs.unlink(absoluteInputPath, () => {});

    if (stdout) console.log(`[RUST LOG] ${stdout}`);

    if (error) {
      console.error(`[EXEC ERR] ${stderr}`);
      return res
        .status(500)
        .json({ error: "Optimization Failed", details: stderr });
    }

    if (fs.existsSync(absoluteOutputPath)) {
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
