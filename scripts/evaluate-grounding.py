"""Opt-in, serialized baseline/candidate evaluation against local llama.cpp.

Uses frozen application binaries and synthetic source, never edits user profiles.
Raw requests/responses include source; keep output under ignored target/.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


BEFORE = """fn removed_total(values: &[i32]) -> i32 {
    let total = values.iter().sum();
    total
}
"""
AFTER = """fn adjacent_assertions() {
    let error = classify_error("context requirement exceeds limit");
    assert_eq!(error.code, "context_exceeded");
    assert!(!error.retryable);
    assert!(error.message.contains("context allocation"));
}

fn multiline_expression() -> i32 {
    let values = [
        {
            let first = 1;
            first + 1
        },
        3,
    ];
    values.iter().copied().filter(|value| *value > 1).sum()
}

fn guarded_index(values: &[i32], index: usize) -> Option<i32> {
    if index >= values.len() {
        return None;
    }
    Some(values[index])
}

fn unseen_helpers(input: &str) -> bool {
    let record = parse_record(input);
    write_record(&record);
    true
}
"""
CASES = {
    "adjacent_assertions": ["changes", "concepts", "deep"],
    "multiline_expression": ["changes", "concepts"],
    "removed_total": ["changes", "concepts"],
    "guarded_index": ["changes", "concepts", "deep"],
    "unseen_helpers": ["changes", "concepts", "deep"],
}


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False), encoding="utf-8")


def request(url, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data, {"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=190) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path,
                        help="omit to reuse an earlier baseline with identical source/settings")
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    if not output.is_relative_to(root / "target"):
        parser.error("output must be inside this repository's ignored target directory")
    output.mkdir(parents=True, exist_ok=False)
    repo = output / "repository"
    repo.mkdir()
    (repo / "sample.rs").write_text(BEFORE, encoding="utf-8")
    for command in [
        ["init", "-q"],
        ["add", "sample.rs"],
        ["-c", "user.name=Grounding evaluation", "-c", "user.email=eval@example.invalid",
         "commit", "-qm", "synthetic baseline"],
    ]:
        subprocess.run(["git", *command], cwd=repo, check=True, capture_output=True)
    (repo / "sample.rs").write_text(AFTER, encoding="utf-8")
    records = []
    current = {}

    class Proxy(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            self.forward()

        def do_POST(self):
            self.forward()

        def forward(self):
            data = self.rfile.read(int(self.headers.get("Content-Length", 0)))
            generation = self.path.endswith("/chat/completions")
            entry = None
            if generation:
                entry = {**current, "request": json.loads(data), "started": time.time()}
                records.append(entry)
            req = urllib.request.Request(
                "http://127.0.0.1:8083" + self.path,
                data if self.command == "POST" else None,
                {"Content-Type": "application/json"}, method=self.command,
            )
            try:
                response = urllib.request.urlopen(req, timeout=180)
            except urllib.error.HTTPError as error:
                status, raw = error.code, error.read()
            except Exception as error:
                raw = json.dumps({"error": str(error)}).encode()
                status = 502
            else:
                status = response.status
                raw = response.read()
            if entry is not None:
                entry.update(status=status, elapsed_seconds=time.time() - entry["started"])
                try:
                    entry["response"] = json.loads(raw)
                except ValueError:
                    entry["response_text"] = raw.decode(errors="replace")
                save(output / f"raw-{len(records):02d}.json", entry)
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            try:
                self.wfile.write(raw)
            except (BrokenPipeError, ConnectionResetError):
                pass

    proxy = ThreadingHTTPServer(("127.0.0.1", 0), Proxy)
    threading.Thread(target=proxy.serve_forever, daemon=True).start()
    manifest = {
        "before_sha256": hashlib.sha256(BEFORE.encode()).hexdigest(),
        "after_sha256": hashlib.sha256(AFTER.encode()).hexdigest(),
        "model": "ocr-qwen36-35b", "temperature": 0,
        "normal_tokens": 1024, "deep_tokens": 1400, "reasoning": False,
        "language_help": "brief", "focus": "balanced", "detail": "balanced",
        "note": "Synthetic helpers intentionally have no supplied definitions. Validation does not execute source.",
        "binaries": {},
    }
    results = []
    try:
        binaries = [("baseline", args.baseline)] if args.baseline else []
        binaries.append(("candidate", args.candidate))
        for label, binary in binaries:
            binary = binary.resolve()
            manifest["binaries"][label] = {"path": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
            save(output / "manifest.json", manifest)
            port = free_port()
            config = output / f"{label}.toml"
            config.write_text(f'''[model]
profile = "evaluation"
[profiles.evaluation]
provider = "openai_compatible"
preset = "llama_cpp"
base_url = "http://127.0.0.1:{proxy.server_port}/v1"
model = "ocr-qwen36-35b"
[profiles.evaluation.normal]
reasoning = false
temperature = 0.0
max_tokens = 1024
[profiles.evaluation.deep]
reasoning = false
temperature = 0.0
max_tokens = 1400
[cache]
enabled = false
[server]
host = "127.0.0.1"
open_browser = false
port = {port}
''', encoding="utf-8")
            env = {**os.environ, "GIT_EXPLAIN_USER_CONFIG": str(config),
                   "APPDATA": str(output / label), "XDG_CONFIG_HOME": str(output / label)}
            log = (output / f"{label}.log").open("w", encoding="utf-8")
            app = subprocess.Popen([str(binary), "--direct", "--port", str(port), "diff", "--", "sample.rs"],
                                   cwd=repo, env=env, stdout=log, stderr=log)
            origin = f"http://127.0.0.1:{port}"
            try:
                deadline = time.monotonic() + 45
                while True:
                    if app.poll() is not None:
                        raise RuntimeError(f"{label} exited; inspect its log")
                    try:
                        _, index = request(origin + "/api/ui/v1/index")
                        break
                    except (urllib.error.URLError, TimeoutError):
                        if time.monotonic() > deadline:
                            raise
                        time.sleep(0.1)
                sections = {s["name"]: s for f in index["files"] for s in f["sections"]}
                for name, kinds in CASES.items():
                    section = sections[name]
                    _, detail = request(origin + f'/api/ui/v1/sections/{section["id"]}?snapshot_id={index["snapshot_id"]}')
                    save(output / f"{label}-{name}-source.json", detail)
                    for kind in kinds:
                        current.clear()
                        current.update(label=label, case=name, kind=kind)
                        start_count, start = len(records), time.monotonic()
                        action = "explain" if kind == "changes" else kind
                        status, result = request(origin + f'/api/units/{detail["generation_id"]}/{action}',
                                                 {"generation": detail["generation"], "learning": {}})
                        row = {**current, "http_status": status, "elapsed_seconds": time.monotonic() - start,
                               "generation_requests": len(records) - start_count, "result": result}
                        results.append(row)
                        save(output / "results.json", results)
                        print(json.dumps({k: v for k, v in row.items() if k != "result"}), flush=True)
            finally:
                app.terminate()
                app.wait(timeout=15)
                log.close()
    finally:
        proxy.shutdown()
        proxy.server_close()


if __name__ == "__main__":
    main()
