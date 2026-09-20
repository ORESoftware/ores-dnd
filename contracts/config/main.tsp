import "@typespec/json-schema";
using TypeSpec.JsonSchema;

namespace OresDnd.Config;

model DndServerConfig {
  commit_path: string;
  require_auth: boolean;
  require_rate_limit: boolean;
}

model DndTelemetryConfig {
  enabled: boolean;
  include_payload: false;
}

@id("OresDndConfigFile")
model OresDndConfigFile {
  schema_version: 1;
  protocol: "ores.dnd/v1";
  enabled: boolean;
  policy_source: "pub-lib-core";
  server: DndServerConfig;
  telemetry: DndTelemetryConfig;
}
