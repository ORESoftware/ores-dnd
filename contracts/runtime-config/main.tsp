import "@typespec/json-schema";
using TypeSpec.JsonSchema;

// Repository-root .ores-dnd.toml consumer binding.
// This file and authored.schema.json are independent human-authored peer authorities.
// Drop policies and wire/session semantics remain authoritative in ../main.tsp and ../authored.schema.json.
namespace OresDnd.RuntimeConfig;

@id("OresDndServerConfig")
model OresDndServerConfig {
  @minLength(1)
  commit_path: string;
  require_auth: boolean;
  require_rate_limit: boolean;
}

@id("OresDndTelemetryConfig")
model OresDndTelemetryConfig {
  enabled: boolean;
  include_payload: boolean;
}

@id("OresDndRuntimeConfig")
model OresDndRuntimeConfig {
  schema_version: 1;
  protocol: "ores.dnd/v1";
  enabled: boolean;
  @minLength(1)
  policy_source: string;
  server: OresDndServerConfig;
  telemetry: OresDndTelemetryConfig;
}
