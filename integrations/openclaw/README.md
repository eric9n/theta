# OpenClaw Integration

This directory contains OpenClaw-specific assets that consume `theta`.

Files:

- `install.sh`: install or refresh the OpenClaw `theta` skill
- `integrations/openclaw/skills/theta/SKILL.md`: OpenClaw skill source
- `sync_ledger.py`: replay OpenClaw ledger transactions into `theta portfolio`

Examples:

```bash
./integrations/openclaw/install.sh
python3 ./integrations/openclaw/sync_ledger.py
```

Optional environment variables for `sync_ledger.py`:

- `OPENCLAW_HOME`
- `OPENCLAW_WORKSPACE`
- `LEDGER_DB_PATH`
- `THETA_BIN`
