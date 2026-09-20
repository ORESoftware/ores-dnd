import "@typespec/json-schema";
using TypeSpec.JsonSchema;

// Repository-root .ores-dnd.toml activation binding.
// This file and authored.schema.json are independent human-authored peer authorities.
// Drop policies and wire/session semantics remain authoritative in ../main.tsp and ../authored.schema.json.
namespace OresDnd.RuntimeConfig;

@id("OresDndRuntimeConfig")
model OresDndRuntimeConfig {
  schema: "ores.dnd/config/v1";
  enabled: boolean;
}
