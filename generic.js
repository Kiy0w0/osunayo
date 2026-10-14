const fs = require("fs");
const BT = String.fromCharCode(96);

const W = new Map();
for (const f of fs.readdirSync("output").filter(n => n.endsWith(".json") && n !== "info.json")) {
  const o = JSON.parse(fs.readFileSync("output/" + f, "utf8"));
  const m = Object.keys(o)[0];
  for (const [c, i] of Object.entries(o[m].classes)) W.set(c, { f: i.fields, p: i.parent });
}

for (const n of ["osu.Framework.Bindables.Bindable" + BT + "1",
                 "osu.Game.Beatmaps.Beatmap" + BT + "1",
                 "osu.Game.Beatmaps.Beatmap",
                 "osu.Game.Rulesets.Objects.HitObjectProperty" + BT + "1",
                 "osu.Game.Rulesets.Objects.IHitObjectProperty",
                 "osu.Game.Rulesets.Objects.SliderPath"]) {
  const e = W.get(n);
  console.log("=== " + n + "   parent=" + (e ? e.p : "MISSING"));
  if (!e) continue;
  for (const [k, v] of Object.entries(e.f)) console.log("   0x" + v.toString(16).padStart(3, "0") + "  " + k);
}

console.log("\n=== any class name containing BeatmapProperty / HitObjectProperty ===");
for (const k of W.keys()) if (/HitObjectProperty|BeatmapProperty/.test(k)) console.log("   " + k + "   parent=" + W.get(k).p);
