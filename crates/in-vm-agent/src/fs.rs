use petty_agent_comms::protocol::{UploadParams, DownloadParams, DownloadResult};
use anyhow::{Result, Context};
use tokio::fs;
use base64::{Engine as _, engine::general_purpose};

pub struct FileSystem;

impl FileSystem {
    pub async fn upload(params: serde_json::Value) -> Result<serde_json::Value> {
        let params: UploadParams = serde_json::from_value(params)?;
        
        let content = general_purpose::STANDARD.decode(&params.content_base64)
            .context("Failed to decode base64 content")?;
            
        // Ensure directory exists
        if let Some(parent) = std::path::Path::new(&params.path).parent() {
            fs::create_dir_all(parent).await?;
        }
        
        fs::write(&params.path, content).await
            .context(format!("Failed to write file: {}", params.path))?;
            
        Ok(serde_json::Value::Null)
    }

    pub async fn download(params: serde_json::Value) -> Result<serde_json::Value> {
        let params: DownloadParams = serde_json::from_value(params)?;
        
        let content = fs::read(&params.path).await
            .context(format!("Failed to read file: {}", params.path))?;
            
        let content_base64 = general_purpose::STANDARD.encode(content);
        
        let result = DownloadResult {
            content_base64,
        };
        
        Ok(serde_json::to_value(result)?)
    }
}
