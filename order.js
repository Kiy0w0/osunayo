const fs = require("fs");

const world = new Map();
for (const f of fs.readdirSync("output").filter(n => n.endsWith(".json") && n !== "info.json")) {
  const o = JSON.parse(fs.readFileSync("output/" + f, "utf8"));
  const mod = Object.keys(o)[0];
  for (const [cls, info] of Object.entries(o[mod].classes)) world.set(cls, info.fields);
}

for (const cls of ["osu.Game.Online.API.Requests.Responses.APIUser", "osu.Game.Online.Chat.Message", "osu.Game.Online.Multiplayer.MultiplayerRoomUser"]) {
  console.log("\n=== " + cls + " (my emitted order, with my offsets) ===");
  const f = world.get(cls);
  for (const [k, v] of Object.entries(f)) console.log("    " + String(v).padStart(4) + " 0x" + v.toString(16) + "  " + k);
}
