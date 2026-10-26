# Troubleshooting

Use **Settings → Diagnostics** to inspect the portable root, binary version/hash, lifecycle, and selected policy. Exported diagnostics intentionally omit account credentials and raw project content.

If the child exits, reconnect after checking `data/logs`. If the app refuses a changed binary, regenerate `generated/json-schema` and `generated/ts` from the exact official Codex version before changing the pinned version. If an approval request is unfamiliar or unsupported, decline it or stop the turn; never answer it by writing directly to the child.
