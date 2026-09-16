//! The lowest-level Anthropic Messages API client. `AnthropicReasoningProvider` and
//! `AnthropicConversationProvider` share this single `complete()`.



use std::process::Stdio;

use anyhow::{bail, Context};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const ANTHROPIC_API_URL: &str = "https://api.teamorouter.cn/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const MODEL_ENV_VAR: &str = "HARBORIA_ANTHROPIC_MODEL";

const DEFAULT_MODEL: &str = "gpt-5.6-luna";

pub struct AnthropicClient {
    api_key: String,
    model: String,
}

impl AnthropicClient {
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY is not set; cannot construct AnthropicClient")?;
        let model = std::env::var(MODEL_ENV_VAR).unwrap_or_else(|_| DEFAULT_MODEL.to_string());
        Ok(Self { api_key, model })
    }


    pub async fn complete(&self, system: &str, user: &str, max_tokens: u32) -> anyhow::Result<String> {
        let body = json!({
            "model": self.model,
            "max_tokens": max_tokens,
            "system": system,
            "messages": [
                { "role": "user", "content": user }
            ],
        })
        .to_string();

        let mut child = Command::new("curl")
            .arg("-sS")
            .arg("--fail-with-body")
            .arg("-X")
            .arg("POST")
            .arg(ANTHROPIC_API_URL)
            .arg("-H")
            .arg(format!("x-api-key: {}", self.api_key))
            .arg("-H")
            .arg(format!("anthropic-version: {ANTHROPIC_VERSION}"))
            .arg("-H")
            .arg("content-type: application/json")
            .arg("--data-binary")
            .arg("@-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn `curl` — is it installed and on PATH?")?;

        {
            let mut stdin = child
                .stdin
                .take()
                .context("failed to open curl's stdin pipe")?;
            stdin
                .write_all(body.as_bytes())
                .await
                .context("failed to write request body to curl's stdin")?;
        }

        let output = child
            .wait_with_output()
            .await
            .context("failed to wait for curl to finish")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!(
                "curl exited with {:?} calling Anthropic API; stderr: {stderr}; body: {stdout}",
                output.status.code()
            );
        }

        let payload: Value = serde_json::from_slice(&output.stdout)
            .context("failed to parse Anthropic API response as JSON")?;

        let content = payload
            .get("content")
            .and_then(|c| c.as_array())
            .context("Anthropic API response is missing a 'content' array")?;

        let text: String = content
            .iter()
            .filter(|block| block.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n");

        if text.is_empty() {
            bail!("Anthropic API response contained no text content blocks: {payload}");
        }
        Ok(text)
    }
}
