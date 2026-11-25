"""Copy one configured model reference into a fresh QA database; never read key values."""
import json
from pathlib import Path
import sqlite3
import sys
from urllib.parse import urlsplit

source = Path(sys.argv[1]).resolve(strict=True)
target = Path(sys.argv[2]).resolve()
if target.exists():
    raise SystemExit("QA database already exists")
preferred = sys.argv[3] if len(sys.argv) > 3 else None
with sqlite3.connect(source.as_uri() + "?mode=ro", uri=True) as original:
    original.row_factory = sqlite3.Row
    row = original.execute(
        "SELECT * FROM llm_models WHERE api_format='openai' AND (?1 IS NULL OR model=?1) "
        "ORDER BY active DESC,updated_at DESC LIMIT 1", (preferred,)
    ).fetchone()
    if row is None:
        raise SystemExit("No configured compatible model")
    host = urlsplit(row["base_url"])
    if host.scheme != "https" or not host.hostname or host.username or host.password:
        raise SystemExit("QA requires a configured HTTPS provider endpoint")
    schema = original.execute("SELECT sql FROM sqlite_master WHERE name='llm_models'").fetchone()[0]
    target.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(target) as qa:
        qa.execute("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL)")
        qa.execute(schema)
        model = dict(row)
        model["active"] = 1
        qa.execute("INSERT INTO llm_models VALUES(" + ",".join("?" for _ in model) + ")", tuple(model.values()))
        config = {"provider": row["provider"], "name": row["model"], "base_url": row["base_url"],
                  "api_key_env": row["api_key_env"], "temperature": row["temperature"],
                  "max_tokens": row["max_tokens"], "invoke_timeout_ms": row["invoke_timeout_ms"]}
        if row["api_key_ref"]:
            config["api_key_ref"] = {"version": 1, "key": row["api_key_ref"]}
        qa.execute("INSERT INTO settings VALUES('app_config',?)", (json.dumps({"model": config}),))
print(json.dumps({"provider": row["provider"], "model": row["model"], "host": host.hostname}))
