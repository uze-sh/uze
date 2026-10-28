//! Deterministic stdio MCP server for the manually dogfoodable playground.
//! It performs no network or filesystem access: the point is to validate that
//! a real harness received an MCP capability from one UZE-managed package.

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerConfig},
    tool, tool_handler, tool_router,
    transport::io::stdio,
};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct EchoArgs {
    text: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct AddArgs {
    left: i64,
    right: i64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct HelloArgs {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct NoArgs {}

#[derive(Debug, Clone)]
struct PlaygroundServer {
    tool_router: ToolRouter<Self>,
}

impl PlaygroundServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl PlaygroundServer {
    #[tool(description = "Echoes text exactly as supplied. Useful for proving MCP tool wiring.")]
    fn echo(&self, Parameters(args): Parameters<EchoArgs>) -> String {
        args.text
    }

    #[tool(description = "Adds two integer values and returns their exact sum.")]
    fn add(&self, Parameters(args): Parameters<AddArgs>) -> String {
        (args.left + args.right).to_string()
    }

    #[tool(description = "Answers a ping with a pong, proving the server was delivered.")]
    fn ping(&self, Parameters(_args): Parameters<NoArgs>) -> String {
        "pong (UZE_PLAYGROUND_MCP: ping)".to_owned()
    }

    #[tool(description = "Greets the given name, or the world when none is given.")]
    fn hello(&self, Parameters(args): Parameters<HelloArgs>) -> String {
        let name = args.name.as_deref().unwrap_or("world");
        format!("hello, {name} (UZE_PLAYGROUND_MCP: hello)")
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PlaygroundServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions("The playground's ping-pong tools")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server = PlaygroundServer::new().serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
