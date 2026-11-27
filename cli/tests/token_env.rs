//! Checks that acu removes `ACU_TOKEN` from its environment at start, for
//! every command, so the adapters that it starts never inherit the session
//! token (issue #42). It runs the built acu with an adapter command that
//! reports the variable.

#![cfg(unix)]

use std::process::Command;

/// Runs `acu extract` and `acu mcp` with `ACU_TOKEN` set and an adapter
/// that prints the variable, then checks that the adapter saw none.
#[test]
fn an_adapter_never_sees_the_session_token() {
    let dir = tempfile::tempdir().unwrap();
    let token = "0123456789abcdef0123456789abcdef-env-test";
    std::fs::write(
        dir.path().join("atlas.yaml"),
        "version: 1\nproject:\n  name: Token test\nlanguages:\n  - adapter: go\n    root: .\n    command: [sh, -c, 'echo \"probe: ACU_TOKEN=${ACU_TOKEN:-unset}\" >&2; exit 3']\n",
    )
    .unwrap();
    for args in [
        &["extract", "-c", "atlas.yaml", "-o", "out.json"][..],
        &["mcp", "-c", "atlas.yaml"][..],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_acu"))
            .args(args)
            .current_dir(dir.path())
            .env("ACU_TOKEN", token)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "{args:?}: the probe adapter fails on purpose"
        );
        assert!(err.contains("probe: ACU_TOKEN=unset"), "{args:?}: {err}");
        assert!(!err.contains(token), "{args:?}: {err}");
    }
}
