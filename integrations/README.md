# Integrations

This directory contains integration assets that consume `theta`.

They are not part of the core CLI or daemon contract:

- `tellar/`: Tellar skill sources and install script
- `openclaw/`: OpenClaw skill source and install script
  - includes `sync_ledger.py` for replaying OpenClaw ledger transactions into `theta portfolio`

Design boundary:

- `core/` and `daemon/` define the real `theta` behavior
- integration assets document or wrap the existing CLI
- integration limitations must not drive changes to `theta` CLI semantics

Install helpers:

```bash
./integrations/tellar/install.sh
./integrations/openclaw/install.sh
```
