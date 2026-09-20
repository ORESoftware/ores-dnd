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
  // DnD payloads can contain customer/application data. Telemetry may record metadata only.
  include_payload: false;
}

@id("OresDndRuntimeConfig")
model OresDndRuntimeConfig {
  schema_version: 1;
  protocol: "ores.dnd/v1";
  enabled: boolean;
  // Policy remains owned by the shared public core; consumers must not redirect policy authority.
  policy_source: "pub-lib-core";
  server: OresDndServerConfig;
  telemetry: OresDndTelemetryConfig;
}
