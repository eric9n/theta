#!/usr/bin/env python3
import os
import sqlite3
import subprocess
import sys
from pathlib import Path

OPENCLAW_HOME = Path(os.path.expanduser(os.environ.get("OPENCLAW_HOME", "~/.openclaw")))
OPENCLAW_WORKSPACE = os.environ.get("OPENCLAW_WORKSPACE", "workspace-trader")
LEDGER_DB_PATH = Path(
    os.environ.get(
        "LEDGER_DB_PATH",
        OPENCLAW_HOME / OPENCLAW_WORKSPACE / "memory" / "ledger.db",
    )
).expanduser()
THETA_BIN = Path(
    os.environ.get("THETA_BIN", os.path.expanduser("~/theta/target/release/theta"))
).expanduser()

def main():
    if not LEDGER_DB_PATH.exists():
        print(f"Error: Ledger DB not found at {LEDGER_DB_PATH}")
        sys.exit(1)

    if not THETA_BIN.exists():
        print(f"Error: theta CLI not found at {THETA_BIN}")
        sys.exit(1)

    # We assume 'theta portfolio' is executed against the default ~/.theta/portfolio.db.
    # Connect to the ledger database
    conn = sqlite3.connect(LEDGER_DB_PATH)
    conn.row_factory = sqlite3.Row
    cursor = conn.cursor()

    # Read all transactions sorted by timestamp
    cursor.execute("SELECT * FROM transactions ORDER BY timestamp ASC, id ASC")
    rows = cursor.fetchall()

    for row in rows:
        timestamp_str = row['timestamp'] # Example: 2026-02-12T13:50:00Z
        trade_date = timestamp_str.split('T')[0] if 'T' in timestamp_str else timestamp_str.split(' ')[0]

        raw_symbol = row['symbol']
        raw_underlying = row['underlying']

        # Strip .US suffix
        symbol = raw_symbol.replace(".US", "")
        underlying = raw_underlying.replace(".US", "")

        qty = float(row['qty'])
        price = float(row['price']) if row['price'] is not None else 0.0
        commission = float(row['commission']) if row['commission'] is not None else 0.0
        right = row['right']  # 'C', 'P', or empty/None
        strike = row['strike']
        expiry = row['expiry']

        if qty == 0:
            continue

        action = "buy" if qty > 0 else "sell"
        abs_qty = abs(int(qty))

        if right == 'C':
            side = 'call'
        elif right == 'P':
            side = 'put'
        else:
            side = 'stock'

        # Build the command via parameter list
        cmd = [
            str(THETA_BIN),
            "portfolio",
            "trade",
            action,
            "--symbol", symbol,
            "--underlying", underlying,
            "--quantity", str(abs_qty),
            "--price", str(price),
            "--commission", str(commission),
            "--date", trade_date,
            "--side", side,
        ]

        if side in ("call", "put") and strike is not None and expiry is not None:
            cmd.extend([
                "--strike", str(strike),
                "--expiry", expiry,
            ])

        # Execute the command
        print(f"Executing: {' '.join(cmd)}")
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            print(f"Failed to add trade: {symbol} on {trade_date}")
            print(f"Command output: {result.stderr}")
            # Do not exit immediately, try others. But maybe we should stop?
            
    print("Done synchronizing transactions!")

if __name__ == "__main__":
    main()
