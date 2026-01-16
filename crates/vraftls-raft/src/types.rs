//! Raft type definitions for OpenRaft integration

use openraft::BasicNode;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use vraftls_core::{NodeId, RaftGroupId};
use vraftls_vfs::{VfsCommand, VfsResponse};

/// OpenRaft の NodeId 型（u64 を使用）
pub type RaftNodeId = u64;

/// ノード情報
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VRaftNode {
    /// ノードのHTTPアドレス
    pub addr: String,
}

impl std::fmt::Display for VRaftNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.addr)
    }
}

/// VRaftLS の型設定
///
/// OpenRaft は generic なので、ここでアプリケーション固有の型を指定する
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VRaftTypeConfig;

impl openraft::RaftTypeConfig for VRaftTypeConfig {
    /// ノードID型
    type NodeId = RaftNodeId;

    /// ノード情報型
    type Node = VRaftNode;

    /// ログに記録されるコマンド（クライアントからの要求）
    type Entry = openraft::Entry<Self>;

    /// スナップショットデータ
    type SnapshotData = Cursor<Vec<u8>>;

    /// 非同期ランタイム
    type AsyncRuntime = openraft::TokioRuntime;

    /// レスポンスの型
    type Responder = openraft::impls::OneshotResponder<Self>;
}

/// ログエントリに記録されるリクエスト
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VfsRequest {
    /// Raft グループ ID
    pub group_id: RaftGroupId,
    /// VFS コマンド
    pub command: VfsCommand,
}

/// 状態マシンからのレスポンス
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VfsStateMachineResponse {
    /// VFS レスポンス
    pub response: VfsResponse,
}

/// Raft グループのメンバーシップ情報
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RaftMembership {
    pub group_id: RaftGroupId,
    pub nodes: Vec<(RaftNodeId, VRaftNode)>,
    pub leader: Option<RaftNodeId>,
}

/// スナップショットのメタデータ
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnapshotMeta {
    /// 最後に適用されたログインデックス
    pub last_applied_log_id: Option<openraft::LogId<RaftNodeId>>,
    /// メンバーシップ設定
    pub membership: openraft::StoredMembership<VRaftTypeConfig>,
}

// OpenRaft の Entry で使うためのトレイト実装
impl openraft::AppData for VfsRequest {}
impl openraft::AppDataResponse for VfsStateMachineResponse {}
