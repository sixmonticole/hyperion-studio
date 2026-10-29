// Verifies that the FR and EN dictionaries in src/i18n.ts define exactly the
// same keys, and that every key is declared in the `Dict` type. A hand-written
// pair of dictionaries silently drifts otherwise.

const fs = require("fs");

const src = fs.readFileSync("src/i18n.ts", "utf8");

// Finds `const NAME: Dict = { ... };` (or `type Dict = { ... }`) and returns
// the quoted keys inside the top-level object.
function keysOf(name) {
  const start = src.indexOf(`const ${name}: Dict = {`);
  const startType = src.indexOf(`type ${name} = {`);
  const from = start >= 0 ? start : startType;
  if (from < 0) return null;
  const open = src.indexOf("{", from);
  // Walk braces to find the matching close.
  let depth = 0;
  let end = -1;
  for (let i = open; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}") {
      depth--;
      if (depth === 0) {
        end = i;
        break;
      }
    }
  }
  const body = src.slice(open, end);
  const set = new Set();
  for (const m of body.matchAll(/^\s*"([a-zA-Z][a-zA-Z0-9._]*)":/gm)) {
    set.add(m[1]);
  }
  return set;
}

const fr = keysOf("fr");
const en = keysOf("en");
const dict = keysOf("Dict");

if (!fr || !en || !dict) {
  console.error("could not parse dictionaries", {
    fr: !!fr,
    en: !!en,
    dict: !!dict,
  });
  process.exit(1);
}

const missingInEn = [...fr].filter((k) => !en.has(k));
const missingInFr = [...en].filter((k) => !fr.has(k));
const inFrNotInType = [...fr].filter((k) => !dict.has(k));
const inTypeNotInFr = [...dict].filter((k) => !fr.has(k));

let ok = true;
const report = (label, arr) => {
  if (arr.length) {
    ok = false;
    console.error(`${label} (${arr.length}):`, arr.join(", "));
  }
};

console.log(`Dict: ${dict.size} keys, fr: ${fr.size}, en: ${en.size}`);
report("keys in fr but missing in en", missingInEn);
report("keys in en but missing in fr", missingInFr);
report("keys used in code but absent from Dict type", inFrNotInType);
report("keys declared in Dict but unused", inTypeNotInFr);

process.exit(ok ? 0 : 1);
