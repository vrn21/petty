use tokio_vsock::VsockListener;
use tokio_util::codec::{Framed, LinesCodec};
use futures::{SinkExt, StreamExt};
use petty_agent_comms::protocol::{JsonRpcRequest, JsonRpcResponse, JsonRpcError};
use crate::executor::Executor;
use crate::fs::FileSystem;
use anyhow::Result;

pub struct AgentServer {
    port: u32,
}

impl AgentServer {
    pub fn new(port: u32) -> Self {
        Self { port }
    }

    pub async fn run(&self) -> Result<()> {
        // Bind to any CID (VMADDR_CID_ANY is -1U or similar, tokio-vsock handles it via libc or constant)
        // tokio-vsock 0.4+ uses u32 for CID. VMADDR_CID_ANY is usually -1i32 cast to u32, i.e. u32::MAX
        let listener = VsockListener::bind(u32::MAX, self.port)?;
        println!("Agent listening on port {}", self.port);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    println!("Accepted connection from CID: {}, Port: {}", addr.cid(), addr.port());
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream).await {
                            eprintln!("Error handling connection: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {}", e);
                }
            }
        }
    }

    async fn handle_connection(stream: tokio_vsock::VsockStream) -> Result<()> {
        let mut framed = Framed::new(stream, LinesCodec::new());

        while let Some(line) = framed.next().await {
            let line = line?;
            if line.trim().is_empty() { continue; }

            let req: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    eprintln!("Invalid JSON: {}", e);
                    continue;
                }
            };

            let response = Self::process_request(req).await;
            let response_str = serde_json::to_string(&response)?;
            framed.send(response_str).await?;
        }
        Ok(())
    }

    async fn process_request(req: JsonRpcRequest) -> JsonRpcResponse {
        let result = match req.method.as_str() {
            "execute" => Executor::execute(req.params).await,
            "upload" => FileSystem::upload(req.params).await,
            "download" => FileSystem::download(req.params).await,
            _ => Err(anyhow::anyhow!("Method not found")),
        };

        match result {
            Ok(val) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(val),
                error: None,
                id: req.id,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32603,
                    message: e.to_string(),
                    data: None,
                }),
                id: req.id,
            },
        }
    }
}
