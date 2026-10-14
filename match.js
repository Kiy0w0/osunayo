const fs = require("fs");

const world = new Map();
for (const f of fs.readdirSync("output").filter(n => n.endsWith(".json") && n !== "info.json")) {
  const o = JSON.parse(fs.readFileSync("output/" + f, "utf8"));
  const mod = Object.keys(o)[0];
  for (const [cls, info] of Object.entries(o[mod].classes)) world.set(cls, info.fields);
}

const targets = [
  ["api_access_game", "osu.Game.Online.API.APIAccess", "game"],
  ["ext_link_opener_api", "osu.Game.Online.Chat.ExternalLinkOpener", "<api>k__BackingField"],
  ["game_base_beatmap_clock", "osu.Game.OsuGameBase", "beatmapClock"],
  ["game_base_beatmap", "osu.Game.OsuGameBase", "<Beatmap>k__BackingField"],
  ["game_base_api", "osu.Game.OsuGameBase", "<API>k__BackingField"],
  ["game_screen_stack", "osu.Game.OsuGame", "<ScreenStack>k__BackingField"],
  ["screen_stack_stack", "osu.Framework.Screens.ScreenStack", "stack"],
  ["framed_clock_final_source", "osu.Game.Beatmaps.FramedBeatmapClock", "finalClockSource"],
  ["framed_clock_current_time", "osu.Framework.Timing.FramedClock", "<CurrentTime>k__BackingField"],
  ["working_map_info", "osu.Game.Beatmaps.WorkingBeatmapCache+BeatmapManagerWorkingBeatmap", "BeatmapInfo"],
  ["working_map_set_info", "osu.Game.Beatmaps.WorkingBeatmapCache+BeatmapManagerWorkingBeatmap", "BeatmapSetInfo"],
  ["map_info_online_id", "osu.Game.Beatmaps.BeatmapInfo", "<OnlineID>k__BackingField"],
  ["map_info_hash", "osu.Game.Beatmaps.BeatmapInfo", "<Hash>k__BackingField"],
  ["map_info_difficulty(DifficultyName)", "osu.Game.Beatmaps.BeatmapInfo", "<DifficultyName>k__BackingField"],
  ["set_info_online_id", "osu.Game.Beatmaps.BeatmapSetInfo", "<OnlineID>k__BackingField"],
  ["submitting_player_api", "osu.Game.Screens.Play.SubmittingPlayer", "<api>k__BackingField"],
  ["player_api", "osu.Game.Screens.Play.Player", "<api>k__BackingField"],
  ["player_drawable_ruleset", "osu.Game.Screens.Play.Player", "<DrawableRuleset>k__BackingField"],
  ["bindable_value", "osu.Framework.Bindables.Bindable`1", "<value>k__BackingField"],
  ["bindable_value(alt)", "osu.Framework.Bindables.Bindable`1", "value"],
  ["bindable_number_value", "osu.Framework.Bindables.BindableNumber`1", "<value>k__BackingField"],
  ["drawable_osu_beatmap", "osu.Game.Rulesets.UI.DrawableRuleset`1", "<Beatmap>k__BackingField"],
  ["drawable_osu_beatmap(alt)", "osu.Game.Rulesets.UI.DrawableRuleset`1", "Beatmap"],
  ["beatmap_hit_objects", "osu.Game.Beatmaps.Beatmap`1", "<HitObjects>k__BackingField"],
  ["list_items", "System.Collections.Generic.List`1", "_items"],
  ["list_size", "System.Collections.Generic.List`1", "_size"],
  ["hit_object_start_time_bindable", "osu.Game.Rulesets.Objects.HitObject", "<StartTimeBindable>k__BackingField"],
  ["osu_hit_object_position_xy", "osu.Game.Rulesets.Osu.Objects.OsuHitObject", "<Position>k__BackingField"],
  ["hit_object_nested_objects", "osu.Game.Rulesets.Objects.HitObject", "<NestedHitObjects>k__BackingField"],
  ["slider_path_wrapper", "osu.Game.Rulesets.Osu.Objects.Slider", "<Path>k__BackingField"],
  ["path_ctrl_points_list", "osu.Game.Rulesets.Osu.Objects.SliderPath", "<ControlPoints>k__BackingField"],
];

const lame = JSON.parse(fs.readFileSync("lame-offsets.json", "utf8"));
const md = JSON.parse(fs.readFileSync("lazer-md.json", "utf8"));

for (const [key, cls, fld] of targets) {
  const fields = world.get(cls);
  let mine = "-";
  if (fields && fld in fields) mine = "0x" + fields[fld].toString(16).toUpperCase() + " (" + fields[fld] + ")";
  else if (!fields) mine = "TYPE NOT FOUND";
  else mine = "field not found; fields: " + Object.keys(fields).join(",");

  const l = lame[key] !== undefined ? lame[key] + " (0x" + lame[key].toString(16).toUpperCase() + ")" : "-";
  const m = md[key] !== undefined ? md[key] + " (0x" + md[key].toString(16).toUpperCase() + ")" : "-";
  console.log(key.padEnd(38) + " | lame " + l.padEnd(16) + " | md " + m.padEnd(16) + " | osunayo " + mine);
}
