# Fixtures

Sanitized, real JSON responses from each provider's usage endpoint (SPEC §7). Parsers are
pinned against these files in unit tests; a new field in a real response that breaks the
parse is a parser bug, not a vendor bug.

Naming: `<provider>_<case>.json`, e.g. `claude_usage_ok.json`, `claude_usage_429.json`.

Sanitize before committing: replace emails, account ids and any token-looking string.
Keep the shape and the numeric values.
