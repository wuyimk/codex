//! Server collaboration-mode discovery and TUI-visible selection. Missing discovery adds no presets.

use codex_app_server_client::AppServerRequestHandle;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::CollaborationModeListParams;
use codex_app_server_protocol::CollaborationModeListResponse;
use codex_app_server_protocol::RequestId;
use codex_protocol::config_types::CollaborationModeMask;
use codex_protocol::config_types::ModeKind;
use std::time::Duration;

use crate::model_catalog::ModelCatalog;

pub(crate) const WORK_MODE_INSTRUCTIONS: &str = r#"# Work mode

You are in persistent Work mode for this conversation until the user switches modes with `/mode`.

Execute the user's plan or explicit request promptly. Use tools, edit files, run commands, and
verify results as needed within the granted permissions. Follow the normal approval and sandbox
rules.

Keep responses concise. Do not narrate routine reasoning or restate the plan. Report only essential
progress, decisions, blockers, and the final result. Ask only when safety, missing authority, or a
material choice requires the user's input.
"#;

pub(crate) const CHAT_MODE_INSTRUCTIONS: &str = r#"# Chat mode

You are in persistent Chat mode for this conversation until the user switches modes with `/mode`.

Focus on discussion: answering questions, explaining ideas, brainstorming, generating options, and
recommending solutions. Respond in text and use only necessary read-only file inspection and
permitted web search. Prefer not to use tools. Do not edit files, execute work, change external
state, or run commands that perform an action.

Web search is off by default. Use it in the current turn only when the user explicitly permits it.
If search is genuinely necessary without that permission, explain what must be checked and ask
first. Permission applies only to the turn in which it was granted.

Be concise and measured. Keep simple questions short. For complex questions, explain the key
reasoning, tradeoffs, alternatives, and a recommendation with enough detail to be useful without
covering every detail. Prioritize a quick response and avoid unnecessary deep reasoning.
"#;

pub(crate) const fn display_name(kind: ModeKind) -> &'static str {
    match kind {
        ModeKind::Default => "Work",
        ModeKind::Plan => "Chat",
    }
}

pub(crate) const fn instructions(kind: ModeKind) -> &'static str {
    match kind {
        ModeKind::Default => WORK_MODE_INSTRUCTIONS,
        ModeKind::Plan => CHAT_MODE_INSTRUCTIONS,
    }
}

fn filtered_presets(model_catalog: &ModelCatalog) -> Vec<CollaborationModeMask> {
    model_catalog
        .collaboration_modes
        .iter()
        .filter(|mask| mask.mode.is_some_and(ModeKind::is_tui_visible))
        .cloned()
        .collect()
}

pub(crate) fn default_mask(model_catalog: &ModelCatalog) -> Option<CollaborationModeMask> {
    let presets = filtered_presets(model_catalog);
    presets
        .iter()
        .find(|mask| mask.mode == Some(ModeKind::Default))
        .cloned()
        .or_else(|| presets.into_iter().next())
}

pub(crate) fn mask_for_kind(
    model_catalog: &ModelCatalog,
    kind: ModeKind,
) -> Option<CollaborationModeMask> {
    if !kind.is_tui_visible() {
        return None;
    }
    filtered_presets(model_catalog)
        .into_iter()
        .find(|mask| mask.mode == Some(kind))
}

/// Cycle to the next collaboration mode preset in list order.
pub(crate) fn next_mask(
    model_catalog: &ModelCatalog,
    current: Option<&CollaborationModeMask>,
) -> Option<CollaborationModeMask> {
    let presets = filtered_presets(model_catalog);
    if presets.is_empty() {
        return None;
    }
    let current_kind = current.and_then(|mask| mask.mode);
    let next_index = presets
        .iter()
        .position(|mask| mask.mode == current_kind)
        .map_or(0, |idx| (idx + 1) % presets.len());
    presets.get(next_index).cloned()
}

pub(crate) fn default_mode_mask(model_catalog: &ModelCatalog) -> Option<CollaborationModeMask> {
    mask_for_kind(model_catalog, ModeKind::Default)
}

pub(crate) fn plan_mask(model_catalog: &ModelCatalog) -> Option<CollaborationModeMask> {
    mask_for_kind(model_catalog, ModeKind::Plan)
}

/// Discovery is optional even on servers that accept the rest of TUI bootstrap.
pub(crate) async fn list(request_handle: AppServerRequestHandle) -> Vec<CollaborationModeMask> {
    let response = tokio::time::timeout(
        Duration::from_secs(/*secs*/ 2),
        request_handle.request_typed::<CollaborationModeListResponse>(
            ClientRequest::CollaborationModeList {
                request_id: RequestId::String(format!(
                    "collaboration-mode-list-{}",
                    uuid::Uuid::new_v4()
                )),
                params: CollaborationModeListParams::default(),
            },
        ),
    )
    .await;
    match response {
        Ok(Ok(response)) => response
            .data
            .into_iter()
            .map(|mask| CollaborationModeMask {
                name: mask.name,
                mode: mask.mode,
                model: mask.model,
                reasoning_effort: mask.reasoning_effort,
                // The RPC intentionally omits prompts. Clear restored overrides so the server
                // supplies its current instructions when this mode is selected.
                developer_instructions: Some(None),
            })
            .collect(),
        _ => {
            tracing::warn!("optional collaborationMode/list discovery unavailable");
            Vec::new()
        }
    }
}
