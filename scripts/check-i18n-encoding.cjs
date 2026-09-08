/**
 * The webview catalogues must stay UTF-8. Saving them as Latin-1 turns
 * "você" into "vocÃª" and that string then ships inside the installer.
 */
const fs = require("fs");
const path = require("path");
const file = path.join(__dirname, "..", "src", "i18n", "index.ts");
const bytes = fs.readFileSync(file);
const text = bytes.toString("utf8");

if (bytes.includes(Buffer.from([0xc3, 0x83]))) {
  console.error("src/i18n/index.ts is double-encoded (você became vocÃª)");
  process.exit(1);
}
if (!text.includes("você") || !text.includes("máquina")) {
  console.error("src/i18n/index.ts is missing expected Portuguese UTF-8 text");
  process.exit(1);
}
console.log("i18n encoding ok");
