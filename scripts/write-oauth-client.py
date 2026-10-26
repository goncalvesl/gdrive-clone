#!/usr/bin/env python3
"""Write a public Desktop OAuth client ID without copying the source secret."""

import json
import os
import pathlib
import sys


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: write-oauth-client.py OUTPUT", file=sys.stderr)
        return 2

    source = os.environ.get("GDRIVE_OAUTH_CLIENT_FILE")
    client_id = os.environ.get("GDRIVE_OAUTH_CLIENT_ID")
    if bool(source) == bool(client_id):
        print("Set exactly one of GDRIVE_OAUTH_CLIENT_FILE or GDRIVE_OAUTH_CLIENT_ID", file=sys.stderr)
        return 2

    if source:
        try:
            data = json.loads(pathlib.Path(source).read_text(encoding="utf-8"))
            client_id = data["installed"]["client_id"]
        except (OSError, ValueError, KeyError, TypeError) as error:
            print(f"Unable to read Desktop OAuth client ID: {type(error).__name__}", file=sys.stderr)
            return 1

    if not isinstance(client_id, str) or not client_id or client_id.strip() != client_id:
        print("Desktop OAuth client ID is invalid", file=sys.stderr)
        return 1

    destination = pathlib.Path(sys.argv[1])
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(
        json.dumps({"installed": {"client_id": client_id}}, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    destination.chmod(0o644)
    return 0


if __name__ == "__main__":
    sys.exit(main())
