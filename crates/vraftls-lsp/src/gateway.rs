//! LSP Gateway - Main entry point for LSP protocol handling

use dashmap::DashMap;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result as JsonRpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use vraftls_core::{ClientId, LanguageId};
use vraftls_vfs::{Vfs, VfsHandle, VfsPath};

use crate::proxy::{LanguageServerPool, LanguageServerProxy};
use crate::router::LspRouter;

/// LSP Gateway server
pub struct LspGateway {
    /// LSP client for sending notifications
    client: Client,

    /// Virtual file system
    vfs: VfsHandle,

    /// Language server proxy pool
    ls_pool: Arc<LanguageServerPool>,

    /// Request router
    router: Arc<LspRouter>,

    /// Client ID counter
    next_client_id: AtomicU64,

    /// Workspace folders
    workspace_folders: RwLock<Vec<WorkspaceFolder>>,

    /// Open documents
    open_documents: DashMap<Url, DocumentState>,
}

/// State of an open document
struct DocumentState {
    version: i32,
    language_id: LanguageId,
    vfs_path: VfsPath,
}

impl LspGateway {
    /// Create a new LSP gateway
    pub fn new(client: Client) -> Self {
        let vfs = Arc::new(Vfs::new(vraftls_core::RaftGroupId::new(1)));
        Self {
            client,
            vfs,
            ls_pool: Arc::new(LanguageServerPool::new()),
            router: Arc::new(LspRouter::new()),
            next_client_id: AtomicU64::new(1),
            workspace_folders: RwLock::new(Vec::new()),
            open_documents: DashMap::new(),
        }
    }

    /// Convert a URI to a VfsPath
    fn uri_to_vfs_path(&self, uri: &Url) -> Option<VfsPath> {
        uri.to_file_path().ok().map(VfsPath::from)
    }

    /// Get the language server for a file
    async fn get_language_server(&self, path: &VfsPath) -> Option<Arc<LanguageServerProxy>> {
        let lang_id = path.language_id()?;
        self.ls_pool.get_or_spawn(lang_id).await.ok()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for LspGateway {
    async fn initialize(&self, params: InitializeParams) -> JsonRpcResult<InitializeResult> {
        tracing::info!("LSP initialize: {:?}", params.root_uri);

        // Store workspace folders
        if let Some(folders) = params.workspace_folders {
            let mut ws = self.workspace_folders.write().await;
            *ws = folders;
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Text document sync
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..Default::default()
                    },
                )),

                // Completion
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                    resolve_provider: Some(true),
                    ..Default::default()
                }),

                // Hover
                hover_provider: Some(HoverProviderCapability::Simple(true)),

                // Go to definition
                definition_provider: Some(OneOf::Left(true)),

                // References
                references_provider: Some(OneOf::Left(true)),

                // Document symbols
                document_symbol_provider: Some(OneOf::Left(true)),

                // Workspace symbols
                workspace_symbol_provider: Some(OneOf::Left(true)),

                // Code actions
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),

                // Formatting
                document_formatting_provider: Some(OneOf::Left(true)),

                // Rename
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),

                // Diagnostics
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        inter_file_dependencies: true,
                        workspace_diagnostics: true,
                        ..Default::default()
                    },
                )),

                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "vraftls".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("LSP initialized");
        self.client
            .log_message(MessageType::INFO, "VRaftLS initialized")
            .await;
    }

    async fn shutdown(&self) -> JsonRpcResult<()> {
        tracing::info!("LSP shutdown");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let version = params.text_document.version;
        let text = params.text_document.text.clone();
        let language_id_str = params.text_document.language_id.clone();

        tracing::debug!("did_open: {}", uri);

        if let Some(vfs_path) = self.uri_to_vfs_path(&uri) {
            let language_id = match language_id_str.as_str() {
                "rust" => LanguageId::Rust,
                "typescript" | "typescriptreact" => LanguageId::TypeScript,
                "javascript" | "javascriptreact" => LanguageId::JavaScript,
                "go" => LanguageId::Go,
                "python" => LanguageId::Python,
                other => LanguageId::Other(other.to_string()),
            };

            // Store in VFS
            self.vfs.apply(vraftls_vfs::VfsCommand::CreateFile {
                path: vfs_path.clone(),
                content: text.clone(),
            });

            // Track open document
            self.open_documents.insert(
                uri.clone(),
                DocumentState {
                    version,
                    language_id: language_id.clone(),
                    vfs_path: vfs_path.clone(),
                },
            );

            // Forward to language server
            if let Some(ls) = self.get_language_server(&vfs_path).await {
                ls.did_open(params).await;
            }
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        tracing::debug!("did_change: {}", uri);

        if let Some(mut doc) = self.open_documents.get_mut(&uri) {
            doc.version = params.text_document.version;

            // Forward to language server
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                ls.did_change(params).await;
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        tracing::debug!("did_close: {}", uri);

        if let Some((_, doc)) = self.open_documents.remove(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                ls.did_close(params).await;
            }
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        tracing::debug!("did_save: {}", uri);

        if let Some(doc) = self.open_documents.get(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                ls.did_save(params).await;
            }
        }
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> JsonRpcResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.clone();

        if let Some(doc) = self.open_documents.get(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                return ls.completion(params).await;
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> JsonRpcResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.clone();

        if let Some(doc) = self.open_documents.get(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                return ls.hover(params).await;
            }
        }

        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> JsonRpcResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri.clone();

        if let Some(doc) = self.open_documents.get(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                return ls.goto_definition(params).await;
            }
        }

        Ok(None)
    }

    async fn references(
        &self,
        params: ReferenceParams,
    ) -> JsonRpcResult<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.clone();

        if let Some(doc) = self.open_documents.get(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                return ls.references(params).await;
            }
        }

        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> JsonRpcResult<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.clone();

        if let Some(doc) = self.open_documents.get(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                return ls.document_symbol(params).await;
            }
        }

        Ok(None)
    }

    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> JsonRpcResult<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri.clone();

        if let Some(doc) = self.open_documents.get(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                return ls.formatting(params).await;
            }
        }

        Ok(None)
    }

    async fn rename(&self, params: RenameParams) -> JsonRpcResult<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.clone();

        if let Some(doc) = self.open_documents.get(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                return ls.rename(params).await;
            }
        }

        Ok(None)
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> JsonRpcResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();

        if let Some(doc) = self.open_documents.get(&uri) {
            if let Some(ls) = self.get_language_server(&doc.vfs_path).await {
                return ls.code_action(params).await;
            }
        }

        Ok(None)
    }
}
