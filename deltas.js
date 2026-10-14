const fs = require("fs");

const world = new Map();
const parents = new Map();
for (const f of fs.readdirSync("output").filter(n => n.endsWith(".json") && n !== "info.json")) {
  const o = JSON.parse(fs.readFileSync("output/" + f, "utf8"));
  const mod = Object.keys(o)[0];
  for (const [cls, info] of Object.entries(o[mod].classes)) {
    world.set(cls, info.fields);
    parents.set(cls, info.parent);
  }
}

const rows = [];
for (const line of fs.readFileSync("D:/coding/reversal/lazer.md", "utf8").split(/\r?\n/)) {
  const m = line.match(/layouts\[2026\.1005\.0\]\.(.*) = (0x[0-9A-Fa-f]+)/);
  if (!m) continue;
  const cut = m[1].lastIndexOf(".");
  rows.push({ cls: m[1].slice(0, cut), fld: m[1].slice(cut + 1), md: parseInt(m[2], 16) });
}

const byClass = new Map();
let hit = 0;
for (const r of rows) {
  const f = world.get(r.cls);
  if (!f || !(r.fld in f)) continue;
  hit++;
  if (!byClass.has(r.cls)) byClass.set(r.cls, []);
  byClass.get(r.cls).push({ fld: r.fld, md: r.md, mine: f[r.fld], d: f[r.fld] - r.md });
}
console.log("matched " + hit + "/" + rows.length + " md entries; exact " + [...byClass.values()].reduce((a, v) => a + v.filter(x => x.d === 0).length, 0));

const want = process.argv[2] ? new RegExp(process.argv[2]) : null;
for (const [cls, v] of byClass) {
  if (want && !want.test(cls)) continue;
  const ds = [...new Set(v.map(x => x.d))];
  console.log("\n" + cls);
  console.log("   parent=" + (parents.get(cls) || "?") + "   deltas=[" + ds.sort((a, b) => a - b).join(",") + "]");
  for (const x of v.sort((a, b) => a.md - b.md))
    console.log("   md=0x" + x.md.toString(16).padStart(3, "0") + " mine=0x" + x.mine.toString(16).padStart(3, "0") + " d=" + String(x.d).padStart(4) + "  " + x.fld);
}
