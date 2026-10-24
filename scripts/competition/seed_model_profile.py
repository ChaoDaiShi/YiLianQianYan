"""Copy one configured model's metadata into an isolated test profile.

Credentials remain in the existing OS secret store. This never reads or exports
secret values, and opens the source SQLite database read-only.
"""
import json
import os
import pathlib
import sqlite3
import sys
import urllib.parse


def profile_db(root: pathlib.Path) -> pathlib.Path:
    registry = json.loads((root / "profile-registry.json").read_text(encoding="utf-8-sig"))
    entry = registry["profiles"][registry["active_profile_id"]]
    return root / "profiles" / entry["directory_name"] / "app.db"


source_root, destination_root = (pathlib.Path(value).resolve() for value in sys.argv[1:3])
if source_root == destination_root:
    raise SystemExit("The smoke test requires an isolated destination")
source = sqlite3.connect(profile_db(source_root).as_uri() + "?mode=ro", uri=True)
source.row_factory = sqlite3.Row
model = source.execute("SELECT * FROM llm_models WHERE active=1 LIMIT 1").fetchone()
if model is None:
    raise SystemExit("No configured active model; a real model is required")
model = dict(model)
if not model["api_key_ref"]:
    raise SystemExit("This test requires an existing OS-backed model credential")
upstream = urllib.parse.urlsplit(model["base_url"])
if upstream.username or upstream.password or upstream.query:
    raise SystemExit("The test requires a credential-free provider URL")
upstream_base = model["base_url"]
model["max_tokens"] = int(os.environ.get("YILIAN_SMOKE_MAX_OUTPUT_TOKENS", "512"))
model["model"] = os.environ.get("YILIAN_SMOKE_MODEL", "deepseek-flash")
if os.environ.get("YILIAN_SMOKE_PROXY_BASE"):
    model["base_url"] = os.environ["YILIAN_SMOKE_PROXY_BASE"]
model["temperature"] = 0
model["invoke_timeout_ms"] = 60000
target = sqlite3.connect(profile_db(destination_root))
if target.execute("SELECT count(*) FROM llm_models").fetchone()[0]:
    raise SystemExit("Destination already contains model records; use a new run directory")
columns = list(model)
target.execute(
    "INSERT INTO llm_models (" + ",".join(columns) + ") VALUES (" + ",".join("?" for _ in columns) + ")",
    [model[column] for column in columns],
)
target.commit()
print(json.dumps({"model_id": model["id"], "provider": model["provider"], "model": model["model"],
                  "host": upstream.hostname, "upstream_base_url": upstream_base,
                  "credentials": "existing_os_secret_reference", "secret_values_exported": False}, ensure_ascii=False))
