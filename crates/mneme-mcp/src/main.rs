//! Stdio entry point for the Mneme MCP server.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use mneme_mcp::{McpServer, McpServerConfig, ServerMode};
use serde_json::json;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut config = McpServerConfig::from_env().map_err(|source| source.to_string())?;
    let mut self_test = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--mode requires personal|team|all".to_owned())?;
                config.mode = ServerMode::parse(&value)
                    .ok_or_else(|| format!("unknown MCP mode: {value}"))?;
            }
            "--v1-store" => {
                config.v1_store = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--v1-store requires <path>".to_owned())?,
                );
            }
            "--team-store" => {
                config.team_store = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--team-store requires <path>".to_owned())?,
                );
            }
            "--team-workspace" => {
                config.team_workspace_id = args
                    .next()
                    .ok_or_else(|| "--team-workspace requires <id>".to_owned())?;
            }
            "--self-test" => self_test = true,
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => return Err(format!("unknown mneme-mcp option: {other}")),
        }
    }

    let server = McpServer::new(config);
    if self_test {
        let tools = server.tools();
        println!(
            "{}",
            json!({
                "ok": true,
                "mode": server.config().mode.as_str(),
                "tool_count": tools.len(),
                "tools": tools.iter().map(|tool| tool.name).collect::<Vec<_>>(),
                "v1_store": server.config().v1_store.display().to_string(),
                "team_store": server.config().team_store.display().to_string(),
            })
        );
        return Ok(());
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|source| format!("read stdin: {source}"))?;
        if line.trim().is_empty() {
            continue;
        }
        stdout
            .write_all(server.handle_json_line(&line).as_bytes())
            .map_err(|source| format!("write stdout: {source}"))?;
        stdout
            .flush()
            .map_err(|source| format!("flush stdout: {source}"))?;
    }
    Ok(())
}

fn print_help() {
    println!(
        r#"Usage: mneme-mcp [--mode personal|team|all] [--v1-store <path>] [--team-store <path>] [--team-workspace <id>] [--self-test]

Run a local stdio MCP server for Mneme.

Environment:
  MNEME_MCP_MODE           personal, team, or all
  MNEME_V1_STORE           v1 personal-memory JSON store
  MNEME_STORE              fallback v1 store path
  MNEME_TEAM_STORE         v2 team-memory JSON store
  MNEME_TEAM_WORKSPACE_ID  workspace id for missing v2 stores
"#
    );
}
