"""Whether Antigravity connects a remote MCP server UZE delivers in a
generated plugin, and its tool reaches the model and runs.

The plugin's canonical `mcp.json` declares a Streamable HTTP server under
`url`, the key the bundled docs (1.3.3+) make primary, with `serverUrl`
kept as the legacy one. UZE writes the plugin's `mcp_config.json`; this
measures whichever key the build under test writes there, and prints it.
The server is a minimal Streamable HTTP MCP endpoint in its own container
on the run's network, answering in JSON (no SSE stream), so the proof is
both its own log of a `tools/call` and its result reaching the model.

Run: python3 conformance/lab.py --harness antigravity --experiment antigravity/mcp-url --discovery
"""

import json
import os
import subprocess

from experiments.relaunch_probe import bindings_for
from shared import common

PROOF = "UZE_REMOTE_MCP_PROOF_7Q4"
PLUGIN = "uze-remote-mcp"
SERVER = "remote"
TOOL = "uze_remote_probe"
MARKET = "/work/remote-market"
CWD = "/work/remote-project"

SERVER_SOURCE = r"""
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

PROOF, TOOL = sys.argv[1], sys.argv[2]

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def reply(self, status, body=None):
        data = json.dumps(body).encode() if body is not None else b""
        self.send_response(status)
        if body is not None:
            self.send_header("Content-Type", "application/json")
        self.send_header("Mcp-Session-Id", "uze-lab")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        print("GET", self.path, flush=True)
        self.reply(405)

    def do_DELETE(self):
        print("DELETE", self.path, flush=True)
        self.reply(200)

    def do_POST(self):
        message = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
        method = message.get("method")
        print("POST", self.path, method, flush=True)
        if "id" not in message:
            return self.reply(202)
        if method == "initialize":
            result = {
                "protocolVersion": message["params"].get("protocolVersion", "2025-03-26"),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "uze-lab-remote", "version": "1.0.0"},
            }
        elif method == "tools/list":
            result = {"tools": [{
                "name": TOOL,
                "description": "Returns the remote MCP proof.",
                "inputSchema": {"type": "object", "properties": {}},
            }]}
        elif method == "tools/call":
            result = {"content": [{"type": "text", "text": PROOF}]}
        else:
            result = {}
        self.reply(200, {"jsonrpc": "2.0", "id": message["id"], "result": result})

HTTPServer(("0.0.0.0", 8765), Handler).serve_forever()
"""


def start_server(cfg):
    """The remote server, on the run's network; returns its address."""
    name = f"{cfg.prov_name}-remote-mcp"
    source = os.path.join(cfg.outdir, "remote_mcp_server.py")
    with open(source, "w") as f:
        f.write(SERVER_SOURCE)
    subprocess.run(["docker", "rm", "-f", name], capture_output=True)
    common.sh(
        "docker",
        "run",
        "-d",
        "--name",
        name,
        "--network",
        cfg.net,
        "-v",
        f"{source}:/app/server.py:ro",
        common.PROVIDER_IMG,
        "python",
        "/app/server.py",
        PROOF,
        TOOL,
    )
    ip = subprocess.run(
        [
            "docker",
            "inspect",
            "-f",
            "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}",
            name,
        ],
        capture_output=True,
        text=True,
    ).stdout.strip()
    return name, ip


def prelude(url):
    mcp = json.dumps({"mcpServers": {SERVER: {"type": "streamable-http", "url": url}}})
    market = json.dumps(
        {
            "name": "remote-lab",
            "plugins": [{"name": PLUGIN, "source": f"./plugins/{PLUGIN}"}],
        }
    )
    plugin = json.dumps({"name": PLUGIN, "description": "A remote MCP server."})
    return f"""
mkdir -p {MARKET}/plugins/{PLUGIN} {CWD}
printf '%s' '{market}' > {MARKET}/marketplace.json
printf '%s' '{plugin}' > {MARKET}/plugins/{PLUGIN}/plugin.json
printf '%s' '{mcp}' > {MARKET}/plugins/{PLUGIN}/mcp.json
git -C {MARKET} init -q && git -C {MARKET} add -A
git -C {MARKET} -c user.name=lab -c user.email=lab@uze.invalid commit -q -m remote
uze market add {MARKET} >/dev/null 2>&1
uze install {PLUGIN}@remote-lab -m 2>&1 | tail -4
echo '===== delivered mcp_config.json'
find /work/home/.gemini -name mcp_config.json -path '*{PLUGIN}*' -exec cat {{}} \\;
echo
git -C {CWD} init -q
"""


def run(cfg, prov_ip):
    bindings = bindings_for(cfg.harness)
    name, ip = start_server(cfg)
    try:
        prompt = "call the uze remote probe mcp tool"
        args = {
            "ServerName": f"{PLUGIN}_{SERVER}",
            "ToolName": TOOL,
            "Arguments": {},
            "toolSummary": "Remote proof",
            "toolAction": "Calling MCP tool",
        }
        prov_ip = common.start_provider(
            cfg, "toolcall", {"FC_ARGS": json.dumps(args), "MCP_PROOF": PROOF}
        )
        cmd = bindings.headless(
            cfg, prov_ip, prelude(f"http://{ip}:8765/mcp"), prompt, CWD
        )
        proc = subprocess.run(
            cmd, capture_output=True, text=True, errors="replace", timeout=480
        )
        out = proc.stdout + proc.stderr
        with open(os.path.join(cfg.outdir, "mcp-url.out"), "w") as f:
            f.write(out)
        print(out[-4000:], flush=True)
        log = subprocess.run(
            ["docker", "logs", name], capture_output=True, text=True
        ).stdout
        print("===== server log\n" + log, flush=True)
    finally:
        subprocess.run(["docker", "rm", "-f", name], capture_output=True)
    common.pull_captures(cfg)
    path = os.path.join(cfg.outdir, "raw-requests.log")
    requests = open(path, errors="replace").read() if os.path.exists(path) else ""
    common.check(
        "remote-mcp-connected",
        "tools/list" in log,
        "agy listed the remote server's tools",
    )
    common.check(
        "remote-mcp-offered",
        TOOL in requests,
        "the remote tool reached a model request",
    )
    common.check(
        "remote-mcp-called", "tools/call" in log, "the remote server received the call"
    )
    common.check("remote-mcp-result", PROOF in requests, "its result reached the model")
