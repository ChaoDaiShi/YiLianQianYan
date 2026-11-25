"""AT-17: two explicitly selected provider profiles run concurrently in sibling conversations."""
import json
import runpy
import sys
import tempfile
import threading
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

run_backend = runpy.run_path(str(Path(__file__).with_name("verify-capability-upgrade.py")))["run_backend"]
rendezvous = threading.Barrier(2, timeout=15)
observed = {}
lock = threading.Lock()

class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type","application/json")
        self.end_headers()
        self.wfile.write(json.dumps({"data":[{"id":name} for name in ("qa-a","qa-b")]}).encode())
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        model = body["model"]
        if not body.get("stream"):
            self.send_response(200)
            self.send_header("Content-Type","application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"id":"verify","choices":[{"index":0,"message":{"role":"assistant","content":"ready"},"finish_reason":"stop"}]}).encode())
            return
        current = next(m["content"] for m in reversed(body["messages"]) if m["role"]=="user")
        with lock:
            observed[model]=current
        rendezvous.wait()
        self.send_response(200)
        self.send_header("Content-Type","text/event-stream")
        self.end_headers()
        choice={"index":0,"delta":{"content":f"{model}-reply"},"finish_reason":None}
        end={"index":0,"delta":{},"finish_reason":"stop"}
        for event in [{"choices":[choice]},{"choices":[end]}]:
            self.wfile.write(("data: "+json.dumps(event)+"\n\n").encode())
        self.wfile.write(b"data: [DONE]\n\n")

def main():
    exe=Path(sys.argv[1]).resolve()
    root=Path(tempfile.mkdtemp(prefix="yilian-parallel-models-"))
    (root/"workspace").mkdir()
    provider=ThreadingHTTPServer(("127.0.0.1",0),Provider)
    threading.Thread(target=provider.serve_forever,daemon=True).start()
    lanes={}
    def exercise(request):
        parent=request("/api/conversations","POST",{"title":"parallel model acceptance"})["id"]
        def prepare(model):
            profile=request("/api/llm/models","POST",{"provider":"llamacpp","label":model,"model":model,
                "base_url":f"http://127.0.0.1:{provider.server_port}/v1","api_format":"openai",
                "temperature":0,"max_tokens":100,"invoke_timeout_ms":20000})
            request(f"/api/llm/models/{profile['id']}/verify","POST",{})
            lane=request(f"/api/conversations/{parent}/lanes","POST",{"kind":"chat","title":model})
            lanes[model]=lane["id"]
            return profile["id"]
        ids={model:prepare(model) for model in ("qa-a","qa-b")}
        def turn(model):
            stream=request("/api/chat","POST",{"conversation_id":lanes[model],"message":model+"-input","workspace_mode":"chat","model_profile_id":ids[model]})
            assert model+"-reply" in stream and '"type":"done"' in stream
        with ThreadPoolExecutor(max_workers=2) as pool:
            list(pool.map(turn,ids))
        assert observed=={m:m+"-input" for m in ids}
        for model, lane in lanes.items():
            conv=request("/api/conversations/"+lane)
            assert conv["run_status"]=="completed"
            assert [m["content"] for m in conv["messages"] if m["role"] in ("user","assistant")]==[model+"-input",model+"-reply"]
        assert len(set(lanes.values()))==2
    try:
        run_backend(exe,root,exercise)
        print(json.dumps({"status":"PASS","controlled_models":2,"distinct_lanes":2,"overlapping_calls":True,"mixed_messages":0}))
    finally:
        provider.shutdown()
        provider.server_close()
if __name__=="__main__": main()

