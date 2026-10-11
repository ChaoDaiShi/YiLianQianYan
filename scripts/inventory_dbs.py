import os
import pathlib
import sqlite3
import json

dirs_to_check = []
if 'YILIAN_DATA_DIR' in os.environ:
    dirs_to_check.append(pathlib.Path(os.environ['YILIAN_DATA_DIR']))

appdata = os.environ.get('APPDATA')
if appdata:
    dirs_to_check.append(pathlib.Path(appdata) / 'yilianqianyan-v1')

local_data = pathlib.Path('data')
if local_data.exists():
    dirs_to_check.append(local_data.resolve())

found_dbs = []
inventory = []
for base in dirs_to_check:
    print('Checking base:', base)
    if base.exists():
        for p in base.rglob('*.db'):
            found_dbs.append(p)

print('Found databases:', len(found_dbs))
for p in found_dbs:
    entry = {'path': str(p), 'status': 'OK', 'migrations': []}
    print('-', p)
    try:
        with sqlite3.connect(p.as_uri() + '?mode=ro', uri=True) as conn:
            exists = conn.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations'").fetchone()
            if not exists:
                print('   NO_MIGRATION_TABLE')
                entry['status'] = 'NO_MIGRATION_TABLE'
            else:
                cols = {row[1] for row in conn.execute('PRAGMA table_info(schema_migrations)')}
                digest = 'digest' if 'digest' in cols else 'NULL AS digest'
                rows = conn.execute(f'SELECT version, name, owner, {digest} FROM schema_migrations ORDER BY version').fetchall()
                print(f'   {len(rows)} migrations')
                for r in rows:
                    entry['migrations'].append({
                        'version': r[0],
                        'name': r[1],
                        'owner': r[2],
                        'digest': r[3],
                    })
    except Exception as e:
        print('   Error reading:', e)
        entry['status'] = f'ERROR: {e}'
    inventory.append(entry)

out_path = pathlib.Path('交付/修复验证/2026-10-10/database-inventory.json')
out_path.parent.mkdir(parents=True, exist_ok=True)
out_path.write_text(json.dumps({
    'production_other_devices': 'NOT_INSPECTED',
    'local_inventory': inventory
}, indent=2, ensure_ascii=False), encoding='utf-8')
print('Inventory written to', out_path)
