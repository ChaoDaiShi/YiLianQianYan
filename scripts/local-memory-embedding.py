"""Loopback-only OpenAI-compatible embeddings from an installed local model."""

import argparse
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Lock


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--model", default="BAAI/bge-small-zh-v1.5",
                        choices=("BAAI/bge-small-zh-v1.5", "Qwen/Qwen3-Embedding-0.6B"))
    parser.add_argument("--port", type=int, default=8765)
    args = parser.parse_args()
    os.environ["HF_HUB_OFFLINE"] = "1"
    os.environ["TRANSFORMERS_OFFLINE"] = "1"

    import torch
    from transformers import AutoModel, AutoTokenizer

    tokenizer = AutoTokenizer.from_pretrained(args.model_dir, local_files_only=True)
    model = AutoModel.from_pretrained(args.model_dir, local_files_only=True).eval()
    model.to("cpu")
    torch.set_num_threads(min(4, os.cpu_count() or 1))
    inference = Lock()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def send_json(self, status, data):
            body = json.dumps(data, allow_nan=False).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            if self.path != "/v1/embeddings":
                self.send_json(404, {"error": "endpoint_not_found"})
                return
            size = int(self.headers.get("Content-Length", "0"))
            if not 0 < size <= 1024 * 1024:
                self.send_json(400, {"error": "invalid_content_length"})
                return
            try:
                request = json.loads(self.rfile.read(size))
            except (ValueError, UnicodeError):
                self.send_json(400, {"error": "invalid_request"})
                return
            if not isinstance(request, dict) or request.get("model") != args.model:
                self.send_json(400, {"error": "model_not_available"})
                return
            text = request.get("input")
            if not isinstance(text, str):
                self.send_json(400, {"error": "input_must_be_text"})
                return
            with inference, torch.inference_mode():
                tokens = tokenizer(text, max_length=512, truncation=True, return_tensors="pt")
                hidden = model(**tokens).last_hidden_state[0]
                position = 0 if args.model.startswith("BAAI/") else tokens.attention_mask.sum().item() - 1
                embedding = torch.nn.functional.normalize(hidden[position].float(), dim=0).tolist()
            self.send_json(200, {"model": args.model, "data": [{"embedding": embedding}]})

    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"{args.model} listening on http://127.0.0.1:{args.port}/v1", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
