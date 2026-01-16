//! Language Server Process Proxy

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{oneshot, Mutex, RwLock};
use tower_lsp::jsonrpc::Result as JsonRpcResult;
use tower_lsp::lsp_types::*;
use vraftls_core::{LanguageId, Result, VRaftError};

/// Pool of language server processes
pub struct LanguageServerPool {
    /// Running language servers
    servers: DashMap<LanguageId, Arc<LanguageServerProxy>>,
}

impl LanguageServerPool {
    pub fn new() -> Self {
        Self {
            servers: DashMap::new(),
        }
    }

    /// Get or spawn a language server for the given language
    pub async fn get_or_spawn(&self, lang: LanguageId) -> Result<Arc<LanguageServerProxy>> {
        // Check if already running
        if let Some(server) = self.servers.get(&lang) {
            return Ok(server.clone());
        }

        // Spawn new server
        let server = LanguageServerProxy::spawn(lang.clone()).await?;
        let server = Arc::new(server);
        self.servers.insert(lang, server.clone());
        Ok(server)
    }

    /// Shutdown all language servers
    pub async fn shutdown_all(&self) {
        for entry in self.servers.iter() {
            entry.value().shutdown().await;
        }
        self.servers.clear();
    }
}

impl Default for LanguageServerPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Pending request map type
type PendingRequests = Arc<DashMap<i64, oneshot::Sender<Value>>>;

/// Proxy to a language server process
pub struct LanguageServerProxy {
    /// Language ID
    language: LanguageId,

    /// Process handle
    process: Mutex<Option<Child>>,

    /// Stdin writer
    stdin: Mutex<Option<ChildStdin>>,

    /// Pending requests waiting for response
    pending: PendingRequests,

    /// Next request ID
    next_id: AtomicI64,

    /// Is the server initialized
    initialized: RwLock<bool>,
}

impl LanguageServerProxy {
    /// Spawn a new language server process
    pub async fn spawn(lang: LanguageId) -> Result<Self> {
        let cmd = lang
            .language_server_command()
            .ok_or_else(|| VRaftError::UnsupportedLanguage(format!("{:?}", lang)))?;

        tracing::info!("Spawning language server: {} for {:?}", cmd, lang);

        let mut child = Command::new(cmd)
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| VRaftError::LanguageServer(format!("Failed to spawn {}: {}", cmd, e)))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();

        let proxy = Self {
            language: lang,
            process: Mutex::new(Some(child)),
            stdin: Mutex::new(stdin),
            pending: Arc::new(DashMap::new()),
            next_id: AtomicI64::new(1),
            initialized: RwLock::new(false),
        };

        // Start response reader task
        if let Some(stdout) = stdout {
            let pending = proxy.pending.clone();
            tokio::spawn(async move {
                Self::read_responses(stdout, pending).await;
            });
        }

        Ok(proxy)
    }

    /// Read responses from the language server
    async fn read_responses(stdout: ChildStdout, pending: PendingRequests) {
        let mut reader = BufReader::new(stdout);
        let mut headers = String::new();

        loop {
            headers.clear();

            // Read headers
            let mut content_length: Option<usize> = None;
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line).await {
                    Ok(0) => return, // EOF
                    Ok(_) => {
                        if line == "\r\n" || line == "\n" {
                            break;
                        }
                        if line.to_lowercase().starts_with("content-length:") {
                            if let Some(len_str) = line.split(':').nth(1) {
                                content_length = len_str.trim().parse().ok();
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Error reading from language server: {}", e);
                        return;
                    }
                }
            }

            // Read content
            if let Some(len) = content_length {
                let mut content = vec![0u8; len];
                if reader.read_exact(&mut content).await.is_err() {
                    return;
                }

                if let Ok(json) = serde_json::from_slice::<Value>(&content) {
                    // Check if it's a response (has id)
                    if let Some(id) = json.get("id").and_then(|v| v.as_i64()) {
                        if let Some((_, sender)) = pending.remove(&id) {
                            let _ = sender.send(json);
                        }
                    } else {
                        // It's a notification
                        tracing::debug!("Received notification: {:?}", json.get("method"));
                    }
                }
            }
        }
    }

    /// Send a request and wait for response
    async fn request<P, R>(&self, method: &str, params: P) -> JsonRpcResult<R>
    where
        P: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let content = serde_json::to_string(&request)
            .map_err(|e| tower_lsp::jsonrpc::Error::internal_error())?;

        let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

        // Create response channel
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id, tx);

        // Send request
        {
            let mut stdin = self.stdin.lock().await;
            if let Some(ref mut stdin) = *stdin {
                if stdin.write_all(message.as_bytes()).await.is_err() {
                    self.pending.remove(&id);
                    return Err(tower_lsp::jsonrpc::Error::internal_error());
                }
            }
        }

        // Wait for response
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(response)) => {
                if let Some(result) = response.get("result") {
                    serde_json::from_value(result.clone())
                        .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())
                } else if let Some(error) = response.get("error") {
                    Err(tower_lsp::jsonrpc::Error::internal_error())
                } else {
                    Err(tower_lsp::jsonrpc::Error::internal_error())
                }
            }
            _ => {
                self.pending.remove(&id);
                Err(tower_lsp::jsonrpc::Error::internal_error())
            }
        }
    }

    /// Send a notification (no response expected)
    async fn notify<P: Serialize>(&self, method: &str, params: P) {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        if let Ok(content) = serde_json::to_string(&notification) {
            let message = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);

            let mut stdin = self.stdin.lock().await;
            if let Some(ref mut stdin) = *stdin {
                let _ = stdin.write_all(message.as_bytes()).await;
            }
        }
    }

    /// Shutdown the language server
    pub async fn shutdown(&self) {
        // Send shutdown request
        let _: JsonRpcResult<()> = self.request("shutdown", ()).await;

        // Send exit notification
        self.notify("exit", ()).await;

        // Kill process
        let mut process = self.process.lock().await;
        if let Some(ref mut child) = *process {
            let _ = child.kill().await;
        }
    }

    // LSP method implementations

    pub async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.notify("textDocument/didOpen", params).await;
    }

    pub async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.notify("textDocument/didChange", params).await;
    }

    pub async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.notify("textDocument/didClose", params).await;
    }

    pub async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.notify("textDocument/didSave", params).await;
    }

    pub async fn completion(
        &self,
        params: CompletionParams,
    ) -> JsonRpcResult<Option<CompletionResponse>> {
        self.request("textDocument/completion", params).await
    }

    pub async fn hover(&self, params: HoverParams) -> JsonRpcResult<Option<Hover>> {
        self.request("textDocument/hover", params).await
    }

    pub async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> JsonRpcResult<Option<GotoDefinitionResponse>> {
        self.request("textDocument/definition", params).await
    }

    pub async fn references(
        &self,
        params: ReferenceParams,
    ) -> JsonRpcResult<Option<Vec<Location>>> {
        self.request("textDocument/references", params).await
    }

    pub async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> JsonRpcResult<Option<DocumentSymbolResponse>> {
        self.request("textDocument/documentSymbol", params).await
    }

    pub async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> JsonRpcResult<Option<Vec<TextEdit>>> {
        self.request("textDocument/formatting", params).await
    }

    pub async fn rename(&self, params: RenameParams) -> JsonRpcResult<Option<WorkspaceEdit>> {
        self.request("textDocument/rename", params).await
    }

    pub async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> JsonRpcResult<Option<CodeActionResponse>> {
        self.request("textDocument/codeAction", params).await
    }
}
