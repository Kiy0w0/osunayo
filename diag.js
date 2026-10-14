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

const pat = /layouts\[2026\.1005\.0\]\.([^.]*(?:\.[^.]*)*)\.([A-Za-z_<][^=]*?) = (0x[0-9A-Fa-f]+)/;
const rows = [];
for (const line of fs.readFileSync("D:/coding/reversal/lazer.md", "utf8").split(/\r?\n/)) {
  const m = line.match(/layouts\[2026\.1005\.0\]\.(.*) = (0x[0-9A-Fa-f]+)/);
  if (!m) continue;
  const full = m[1];
  const md = parseInt(m[2], 16);
  const cut = full.lastIndexOf(".");
  const cls = full.slice(0, cut);
  const fld = full.slice(cut + 1);
  rows.push({ cls, fld, md });
}

const byClass = new Map();
let unresolved = 0;
for (const r of rows) {
  const f = world.get(r.cls);
  if (!f || !(r.fld in f)) { unresolved++; continue; }
  const mine = f[r.fld];
  if (!byClass.has(r.cls)) byClass.set(r.cls, []);
  byClass.get(r.cls).push({ fld: r.fld, md: r.md, mine, d: mine - r.md });
}

console.log("md entries: " + rows.length + "  matched classes: " + byClass.size + "  unmatched: " + unresolved);
console.log("");
console.log("=== classes with a NONZERO delta, grouped ===");
const bad = [...byClass.entries()].filter(([, v]) => v.some(x => x.d !== 0));
for (const [cls, v] of bad) {
  const ds = [...new Set(v.map(x => x.d))].sort((a, b) => a - b);
  console.log("\n" + cls + "   parent=" + (parents.get(cls) || "?") + "   deltas=[" + ds.join(",") + "]");
  for (const x of v.sort((a, b) => a.md - b.md))
    console.log("    d=" + String(x.d).padStart(4) + "  md=0x" + x.md.toString(16) + "  mine=0x" + x.mine.toString(16) + "  " + x.fld);
}
console.log("\n=== classes with a uniform ZERO delta ===");
for (const [cls, v] of [...byClass.entries()].filter(([, v]) => v.every(x => x.d === 0)))
  console.log("  " + cls + "  (" + v.length + " fields)");

console.log("\n=== extra lookups ===");
const extra = [
  ["osu.Framework.Bindables.BindableDouble", ["value"]],
  ["osu.Framework.Bindables.BindableNumber`1", ["value", "minValue", "maxValue"]],
  ["osu.Game.Rulesets.Objects.HitObject", ["StartTimeBindable", "nestedHitObjects", "SamplesBindable"]],
  ["osu.Game.Rulesets.Osu.Objects.OsuHitObject", ["position", "stackHeight"]],
  ["osu.Game.Rulesets.Osu.Objects.Slider", ["path", "repeatCount", "endPositionCache"]],
  ["osu.Game.Rulesets.UI.DrawableRuleset`1", ["Beatmap"]],
  ["osu.Game.Beatmaps.Beatmap`1", ["<HitObjects>k__BackingField"]],
  ["osu.Game.Beatmaps.Beatmap", ["<HitObjects>k__BackingField"]],
];
for (const [cls, flds] of extra) {
  const f = world.get(cls);
  if (!f) { console.log(cls + "  -> TYPE NOT FOUND"); continue; }
  console.log(cls + "  parent=" + (parents.get(cls) || "?"));
  for (const fld of flds)
    console.log("    " + fld + " = " + (fld in f ? "0x" + f[fld].toString(16).toUpperCase() + " (" + f[fld] + ")" : "MISSING"));
}
const sliderPathLike = [...world.keys()].filter(k => /SliderPath/.test(k));
console.log("SliderPath-like classes: " + JSON.stringify(sliderPathLike));
for (const k of sliderPathLike) console.log("    " + k + " -> " + JSON.stringify(world.get(k)));

for (const cls of ["osu.Framework.Graphics.Drawable", "osu.Framework.Graphics.Containers.CompositeDrawable", "osu.Framework.Graphics.Containers.Container", "osu.Game.Online.API.APIAccess", "osu.Game.Online.Chat.ExternalLinkOpener"]) {
  const f = world.get(cls);
  console.log("\n--- " + cls + " parent=" + (parents.get(cls) || "?"));
  if (f) console.log("    " + JSON.stringify(f));
}
