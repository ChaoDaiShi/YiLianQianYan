"""Exercise an older Windows backend and the current backend against one isolated profile."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import math
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import sqlite3
import subprocess
import time
import urllib.request

def run_backend(exe, root, verify):
    token = secrets.token_hex(32)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    env = {k:v for k,v in os.environ.items()
           if not k.upper().startswith(("YILIAN_", "VITE_"))
           and not any(word in k.upper() for word in ("KEY","TOKEN","SECRET","PASSWORD"))}
    env.update(YILIAN_HOST=f"127.0.0.1:{port}", YILIAN_CONTROL_SESSION_TOKEN=token,
               YILIAN_DATA_DIR=str(root/"data"), YILIAN_WORKSPACE=str(root/"workspace"))
    log = (root/"backend-validation.log").open("ab")
    proc = subprocess.Popen([str(exe.resolve())], env=env, stdout=log,
                            stderr=log, creationflags=getattr(subprocess,"CREATE_NO_WINDOW",0))
    def request(path, method="GET", data=None):
        req=urllib.request.Request(f"http://127.0.0.1:{port}"+path,method=method,
                                   data=None if data is None else json.dumps(data).encode("utf-8"),
                                   headers={"X-Yilian-Control-Session":token,"Content-Type":"application/json"})
        try:
            opened=urllib.request.urlopen(req,timeout=30 if path=="/api/chat" else 4)
        except urllib.error.HTTPError as error:
            raise RuntimeError(f"{method} {path} -> {error.code}: {error.read().decode('utf-8',errors='replace')}") from error
        with opened as res:
            if res.status==204: return None
            if res.headers.get_content_type()=="text/event-stream":
                return res.read().decode("utf-8")
            return json.load(res)
    try:
        deadline=time.monotonic()+45
        while True:
            if proc.poll() is not None: raise RuntimeError("backend exited during startup")
            try: health=request("/api/health")
            except OSError:
                if time.monotonic()>deadline: raise TimeoutError("backend health timeout")
                time.sleep(.2)
            else: break
        assert health["database"]=="healthy"
        result = verify(request)
        with ThreadPoolExecutor(max_workers=4) as pool:
            def measure(_):
                start=time.perf_counter()
                request("/api/system/preferences")
                return (time.perf_counter()-start)*1000
            times=sorted(pool.map(measure,range(80)))
        stats={"requests":len(times),"workers":4,"errors":0,"p50_ms":round(times[math.ceil(len(times)*.5)-1],2),
               "p95_ms":round(times[math.ceil(len(times)*.95)-1],2)}
        if os.name=="nt":
            output=subprocess.check_output(["powershell.exe","-NoProfile","-Command",
                    f"(Get-Process -Id {proc.pid}).PeakWorkingSet64"],creationflags=subprocess.CREATE_NO_WINDOW)
            stats["process_peak_working_set_mib"]=round(int(output.strip())/1048576,2)
        print(json.dumps({"endpoint":"/api/system/preferences","sample":stats}))
        return result
    finally:
        proc.terminate()
        proc.wait(timeout=15)
        log.close()

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("phase",choices=["seed","verify","soak"])
    parser.add_argument("--seconds",type=int,default=900)
    parser.add_argument("executable",type=Path)
    parser.add_argument("fixture",type=Path)
    args=parser.parse_args()
    root=args.fixture.resolve()
    if args.phase=="soak":
        (root/"workspace").mkdir(parents=True,exist_ok=False)
        if args.seconds < 4: raise ValueError("soak must span both server lifetimes")
        scope=None
        revision=0
        sequence=0
        requests=0
        start=time.monotonic()
        def exercise(request):
            nonlocal scope,revision,sequence,requests
            initial=request("/api/v1/profile")
            assert initial["revision"]==revision
            if scope is None: scope=initial["scope"]
            assert initial["scope"]==scope
            deadline=time.monotonic()+args.seconds/2
            while time.monotonic()<deadline:
                revision+=1
                request("/api/v1/profile","PATCH",{"expected_revision":revision-1,"fields":{"scenario":f"soak-{revision}"}})
                result=request("/api/v1/events?stream_id=profile:"+scope+"&after="+str(sequence))
                assert len(result["events"])==1 and result["next_cursor"]>sequence
                assert result["events"][0]["payload"]["revision"]==revision
                sequence=result["next_cursor"]
                assert request("/api/health")["database"]=="healthy"
                requests+=3
                time.sleep(min(5,max(0,deadline-time.monotonic())))
        for _ in range(2): run_backend(args.executable,root,exercise)
        print(json.dumps({"result":"passed","duration_seconds":round(time.monotonic()-start,1),"requests":requests,
                          "verified_transactions":revision,"forced_process_restart":1,"duplicate_or_missing_events":0}))
        return
    if args.phase=="seed":
        (root/"workspace").mkdir(parents=True,exist_ok=False)
        run_backend(args.executable,root,lambda get:get("/api/system/preferences"))
        stores=list((root/"data").rglob("app.db"))
        if len(stores)!=1: raise RuntimeError(f"expected single profile database, found {len(stores)}")
        with sqlite3.connect(stores[0]) as db:
            assert db.execute("SELECT COUNT(*) FROM sqlite_master WHERE name='local_onboarding'").fetchone()[0]==0
            db.execute("INSERT INTO conversations(id,title,created_at,updated_at) VALUES('upgrade-fixture','升级回归',1,1)")
            db.execute("INSERT INTO messages(id,conversation_id,role,content,created_at) VALUES('upgrade-payload','upgrade-fixture','user','升级前写入的验收消息',1)")
            db.execute("INSERT INTO settings(key,value) VALUES('upgrade.fixture','retained')")
        (root/"fixture.json").write_text(json.dumps({"db":str(stores[0].relative_to(root))}),encoding="utf-8")
        print("pre-upgrade binary created SQLite profile and seeded 3 fixture records")
        return
    path=root/json.loads((root/"fixture.json").read_text(encoding="utf-8"))["db"]
    def verify(get):
        assert get("/api/v1/onboarding/status")["flow_version"]==1
        assert get("/api/v1/conversations/upgrade-fixture/events")["events"]==[]
    run_backend(args.executable,root,verify)
    backups=list(path.parent.glob(path.name+".before-capability-upgrade-*.backup"))
    assert len(backups)==1
    # Open a copy of the backup at a new path and validate original records independently.
    restore=root/"restored.db"
    shutil.copyfile(backups[0],restore)
    for db_path,expected_new in [(path,True),(restore,False)]:
        with sqlite3.connect(db_path) as db:
            assert db.execute("PRAGMA integrity_check").fetchone()[0]=="ok"
            assert db.execute("SELECT count(*) FROM pragma_foreign_key_check").fetchone()[0]==0
            assert db.execute("SELECT title FROM conversations WHERE id='upgrade-fixture'").fetchone()==("升级回归",)
            assert db.execute("SELECT content FROM messages WHERE id='upgrade-payload'").fetchone()==("升级前写入的验收消息",)
            assert db.execute("SELECT value FROM settings WHERE key='upgrade.fixture'").fetchone()==("retained",)
            assert bool(db.execute("SELECT count(*) FROM sqlite_schema WHERE name='local_onboarding'").fetchone()[0])==expected_new
    print("PASS: legacy profile boot, migration, integrity, foreign keys and restored pre-upgrade content")

if __name__=="__main__":
    main()

