//! LIVE test: connect to a real stdio MCP server via npx and exercise the
//! tools/list + tools/call round-trip. `#[ignore]` by default (needs npx +
//! network on first run to fetch the package).
//!
//! Run: cargo test -p deepseek-mcp --test live_stdio_test -- --ignored --nocapture

use deepseek_mcp::config::ServerConfig;
use deepseek_mcp::connection::McpConnection;
use deepseek_mcp::types::ServerStatus;

#[tokio::test]
#[ignore = "live: needs npx + network"]
async fn live_connect_everything_server() {
    let cfg = ServerConfig::Stdio {
        command: "npx".into(),
        args: vec![
            "-y".into(),
            "@modelcontextprotocol/server-everything".into(),
        ],
        env: Default::default(),
        disabled: false,
    };
    let conn = McpConnection::new("everything", cfg);
    conn.connect().await.expect("connect to everything server");
    assert_eq!(conn.status(), ServerStatus::Connected);

    let tools = conn.tools();
    eprintln!("=== everything server exposed {} tools ===", tools.len());
    for t in &tools {
        eprintln!("  - {}: {}", t.name, t.description);
    }
    assert!(!tools.is_empty(), "server should expose tools");
    assert!(
        tools.iter().any(|t| t.name == "echo"),
        "should have echo tool"
    );

    // Call the echo tool.
    let out = conn
        .call_tool("echo", serde_json::json!({ "message": "hello-mcp" }))
        .await
        .expect("echo call");
    eprintln!("=== echo returned ===\n{out}");
    assert!(out.contains("hello-mcp"), "echo should return our message");

    conn.shutdown().await;
}
