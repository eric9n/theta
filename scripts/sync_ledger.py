#!/usr/bin/env python3
import sqlite3
import subprocess
import os
import sys

# Configure paths
LEDGER_DB_PATH = os.path.expanduser("~/.openclaw/workspace-trader/memory/ledger.db")
PORTFOLIO_BIN = os.path.expanduser("~/theta/target/release/portfolio")

def main():
    if not os.path.exists(LEDGER_DB_PATH):
        print(f"Error: Ledger DB not found at {LEDGER_DB_PATH}")
        sys.exit(1)

    if not os.path.exists(PORTFOLIO_BIN):
        print(f"Error: Portfolio CLI not found at {PORTFOLIO_BIN}")
        sys.exit(1)

    # We assume 'portfolio' command is executed against the default ~/.theta/portfolio.db.
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
            PORTFOLIO_BIN,
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
