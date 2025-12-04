const fs = require("fs");
const path = require("path");

const DB_FILE = path.join(__dirname, "database.json");

// Initialize DB if missing
if (!fs.existsSync(DB_FILE)) {
  fs.writeFileSync(
    DB_FILE,
    JSON.stringify({ keys: {}, customers: {} }, null, 2),
  );
}

// Load Data
function load() {
  return JSON.parse(fs.readFileSync(DB_FILE, "utf8"));
}

// Save Data
function save(data) {
  fs.writeFileSync(DB_FILE, JSON.stringify(data, null, 2));
}

module.exports = {
  // Check if key is valid
  isValidKey: (key) => {
    const db = load();
    return !!db.keys[key];
  },

  // Create a new customer key
  createKey: (email, stripeCustomerId) => {
    const db = load();
    const newKey = "sk_" + require("uuid").v4().replace(/-/g, "");

    db.keys[newKey] = {
      email,
      stripeCustomerId,
      created: Date.now(),
      active: true,
    };

    db.customers[stripeCustomerId] = { email, key: newKey };

    save(db);
    return newKey;
  },

  // EXPORT THIS SO INDEX.JS CAN USE IT
  load: load,
};
