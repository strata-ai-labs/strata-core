---
summary: Read one sanitized configuration value by key.
mcp_description: Use this when the user wants a single configuration value by key rather than the whole sanitized config.
---

Returns one sanitized configuration value from the allowlist by key, or a null value when the key is not recognized. Only allowlisted, non-sensitive keys are served; an empty key is rejected with `invalid_argument.engine.config_key`.
