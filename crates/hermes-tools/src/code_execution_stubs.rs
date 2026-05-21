//! `hermes_tools.py` stub generator for execute_code PTC (Python `generate_hermes_tools_module` parity).

use std::collections::BTreeSet;

use crate::code_execution_env::SANDBOX_ALLOWED_TOOLS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcTransport {
    Uds,
    Tcp,
}

struct ToolStubDef {
    name: &'static str,
    signature: &'static str,
    doc: &'static str,
    args_expr: &'static str,
}

const TOOL_STUBS: &[ToolStubDef] = &[
    ToolStubDef {
        name: "web_search",
        signature: "query: str, limit: int = 5",
        doc: r#"""Search the web. Returns dict with data.web list of {url, title, description}."""#,
        args_expr: r#"{"query": query, "limit": limit}"#,
    },
    ToolStubDef {
        name: "web_extract",
        signature: "urls: list",
        doc: r#"""Extract content from URLs. Returns dict with results list of {url, title, content, error}."""#,
        args_expr: r#"{"urls": urls}"#,
    },
    ToolStubDef {
        name: "read_file",
        signature: "path: str, offset: int = 1, limit: int = 500",
        doc: r#"""Read a file (1-indexed lines). Returns dict with "content" and "total_lines"."""#,
        args_expr: r#"{"path": path, "offset": offset, "limit": limit}"#,
    },
    ToolStubDef {
        name: "write_file",
        signature: "path: str, content: str",
        doc: r#"""Write content to a file (always overwrites). Returns dict with status."""#,
        args_expr: r#"{"path": path, "content": content}"#,
    },
    ToolStubDef {
        name: "search_files",
        signature: r#"pattern: str, target: str = "content", path: str = ".", file_glob: str = None, limit: int = 50, offset: int = 0, output_mode: str = "content", context: int = 0"#,
        doc: r#"""Search file contents (target="content") or find files by name (target="files"). Returns dict with "matches"."""#,
        args_expr: r#"{"pattern": pattern, "target": target, "path": path, "file_glob": file_glob, "limit": limit, "offset": offset, "output_mode": output_mode, "context": context}"#,
    },
    ToolStubDef {
        name: "patch",
        signature: r#"path: str = None, old_string: str = None, new_string: str = None, replace_all: bool = False, mode: str = "replace", patch: str = None"#,
        doc: r#"""Targeted find-and-replace (mode="replace") or V4A multi-file patches (mode="patch"). Returns dict with status."""#,
        args_expr: r#"{"path": path, "old_string": old_string, "new_string": new_string, "replace_all": replace_all, "mode": mode, "patch": patch}"#,
    },
    ToolStubDef {
        name: "terminal",
        signature: "command: str, timeout: int = None, workdir: str = None",
        doc: r#"""Run a shell command (foreground only). Returns dict with "output" and "exit_code"."""#,
        args_expr: r#"{"command": command, "timeout": timeout, "workdir": workdir}"#,
    },
];

const COMMON_HELPERS: &str = r#"

# ---------------------------------------------------------------------------
# Convenience helpers (avoid common scripting pitfalls)
# ---------------------------------------------------------------------------

def json_parse(text: str):
    """Parse JSON tolerant of control characters (strict=False)."""
    return json.loads(text, strict=False)


def shell_quote(s: str) -> str:
    """Shell-escape a string for safe interpolation into commands."""
    return shlex.quote(s)


def retry(fn, max_attempts=3, delay=2):
    """Retry a function with exponential backoff."""
    last_err = None
    for attempt in range(max_attempts):
        try:
            return fn()
        except Exception as e:
            last_err = e
            if attempt < max_attempts - 1:
                time.sleep(delay * (2 ** attempt))
    raise last_err

"#;

const UDS_TRANSPORT_HEADER: &str = r#"""Auto-generated Hermes tools RPC stubs."""
import json, os, socket, shlex, threading, time

_sock = None
_call_lock = threading.Lock()
"#;

const TCP_TRANSPORT_HEADER: &str = r#"""Auto-generated Hermes tools RPC stubs."""
import json, os, socket, shlex, threading, time

_sock = None
_call_lock = threading.Lock()
"#;

const UDS_CALL_IMPL: &str = r#"
def _connect():
    global _sock
    if _sock is None:
        endpoint = os.environ["HERMES_RPC_SOCKET"]
        if endpoint.startswith("tcp://"):
            _host_port = endpoint[len("tcp://"):]
            _host, _, _port = _host_port.rpartition(":")
            _sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            _sock.connect((_host or "127.0.0.1", int(_port)))
        else:
            _sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            _sock.connect(endpoint)
        _sock.settimeout(300)
    return _sock

def _call(tool_name, args):
    request = json.dumps({"tool": tool_name, "args": args}) + "\n"
    with _call_lock:
        conn = _connect()
        conn.sendall(request.encode())
        buf = b""
        while True:
            chunk = conn.recv(65536)
            if not chunk:
                raise RuntimeError("Agent process disconnected")
            buf += chunk
            if buf.endswith(b"\n"):
                break
    raw = buf.decode().strip()
    result = json.loads(raw)
    if isinstance(result, str):
        try:
            return json.loads(result)
        except (json.JSONDecodeError, TypeError):
            return result
    return result
"#;

/// Tools in both [`SANDBOX_ALLOWED_TOOLS`] and `enabled_tools`, sorted.
pub fn resolve_sandbox_tools(enabled_tools: &[String]) -> Vec<String> {
    let allowed: BTreeSet<&str> = SANDBOX_ALLOWED_TOOLS.iter().copied().collect();
    let enabled: BTreeSet<&str> = enabled_tools.iter().map(|s| s.as_str()).collect();
    allowed
        .intersection(&enabled)
        .map(|s| (*s).to_string())
        .collect()
}

/// Build `hermes_tools.py` source (Python `generate_hermes_tools_module`).
pub fn generate_hermes_tools_module(enabled_tools: &[String], transport: RpcTransport) -> String {
    let tools = resolve_sandbox_tools(enabled_tools);
    let header = match transport {
        RpcTransport::Uds => UDS_TRANSPORT_HEADER,
        RpcTransport::Tcp => TCP_TRANSPORT_HEADER,
    };
    let mut out = String::from(header);
    out.push_str(COMMON_HELPERS);
    out.push_str(UDS_CALL_IMPL);
    for name in tools {
        let Some(stub) = TOOL_STUBS.iter().find(|s| s.name == name) else {
            continue;
        };
        out.push_str(&format!(
            "\ndef {}({}):\n    {}\n    return _call({:?}, {})\n",
            stub.name, stub.signature, stub.doc, stub.name, stub.args_expr
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_web_search_stub() {
        let src = generate_hermes_tools_module(&["web_search".into()], RpcTransport::Tcp);
        assert!(src.contains("def web_search("));
        assert!(src.contains("HERMES_RPC_SOCKET"));
        assert!(!src.contains("def terminal("));
    }

    #[test]
    fn intersects_sandbox_allowlist() {
        let src = generate_hermes_tools_module(
            &["web_search".into(), "execute_code".into()],
            RpcTransport::Tcp,
        );
        assert!(src.contains("def web_search("));
        assert!(!src.contains("def execute_code("));
    }
}
