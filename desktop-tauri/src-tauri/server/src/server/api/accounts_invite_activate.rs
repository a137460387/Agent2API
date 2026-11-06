//! 填写邀请码（`POST /api/accounts/invite/activate`）。**写操作，不可撤销。**
//!
//! ── 为什么单独一条路由、而不是塞进 `GET /api/accounts/invite` ─────
//! 读与写在语义上不该共用一个入口：`GET .../invite` 是「看一眼我的邀请码和
//! 已邀人数」，随时可调；这一条会**永久消耗**目标账号的激活资格，并给邀请方
//! 发真实积分。放进同一个 GET 里等于让「刷新一下页面」也可能触发写操作。
//!
//! ── 这个接口不做的事 ────────────────────────────────────────
//! 1. **不重试**：上游 `activate` 是一次性的，自动重试可能重复消耗资格。
//! 2. **不批量**：一次只允许一个账号 + 一个邀请码，必须显式指名。
//! 3. **不绕过风控**：`device_identifier` 由插件按客户端算法如实生成，
//!    不提供「传空串跳过设备校验」的开关。
//!
//! ── 调用方必须自己做的确认 ──────────────────────────────────
//! 这是一个不可逆动作。界面在调用前应当明确告知「填了就改不回来」，
//! 并展示目标账号当前是否已被邀请过（`GET /api/accounts/invite` 的
//! `inviterId` 字段）。网关不做二次确认 —— 它没有「弹窗」这个能力，
//! 也不该替调用方做这个决定。

use axum::body::Bytes;
use axum::response::Response;
use serde_json::Value;

use crate::server::errors::management_error;
use crate::server::http::{ok_json, parse_body};
use crate::server::ServerState;

/// `POST /api/accounts/invite/activate` `{id, inviteCode}`
pub async fn account_invite_activate(state: &ServerState, body: &Bytes) -> Response {
    let payload = match parse_body(body) {
        Ok(payload) => payload,
        // `parse_body` 的 Err 已经是 `GatewayError`（带 400 与可读文案），
        // 直接按它的状态码回，别在这里再包一层。
        Err(error) => return management_error(error.status_code, error.message),
    };
    let id = payload
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let invite_code = payload
        .get("inviteCode")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if id.is_empty() {
        return management_error(400, "缺少 id：必须指明哪个账号来填写邀请码");
    }
    if invite_code.is_empty() {
        return management_error(400, "缺少 inviteCode：请填写要使用的邀请码");
    }
    let Some(provider) = state.store().account_provider_of(id) else {
        return management_error(404, format!("账号不存在: {id}"));
    };
    let Some(adapter) = crate::server::core::providers::adapter::adapter_by_id(&provider) else {
        return management_error(
            503,
            format!("「{provider}」的提供商插件没有安装，无法填写邀请码"),
        );
    };
    let params = serde_json::json!({ "inviteCode": invite_code });
    match adapter
        .admin_action(state.store(), id, "invite.activate", &params)
        .await
    {
        Ok(result) => {
            crate::server::logging::log(
                "[Invite]",
                &format!("✅ 账号 {id} 已填写邀请码 {invite_code}（不可撤销）"),
            );
            ok_json(result)
        }
        Err(error) => management_error(error.status_code, error.message),
    }
}
