# `.ores-dnd.toml` contract

This is the project-level configuration authority for ORES drag-and-drop consumers. The org-wide dependency enters through `<org>-pub-lib-core`; UI clients consume the policies re-exported there. Servers re-run the same policy at the commit boundary and never trust the browser verdict.

`policy_source` is therefore fixed to `pub-lib-core`. `telemetry.include_payload` is fixed to `false`: dragged payload bytes must never enter logs or telemetry. A server that enables DnD must keep authentication and rate limiting on the commit endpoint.

Example:

```toml
schema_version = 1
protocol = "ores.dnd/v1"
enabled = true
policy_source = "pub-lib-core"

[server]
commit_path = "/_ores/dnd/drop"
require_auth = true
require_rate_limit = true

[telemetry]
enabled = true
include_payload = false
```
