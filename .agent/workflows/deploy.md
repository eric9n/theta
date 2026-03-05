---
description: Deploy MCP server to VPS
---

This workflow syncs the latest code to the VPS and compiles the MCP server in release mode.

1. Deploy code to VPS
// turbo
```bash
ssh srv1313960 "cd /root/theta && git pull origin main && source ~/.cargo/env && cargo build --release"
```
