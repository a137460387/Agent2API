//! 邀请概览（`GET /api/accounts/invite?id=<accountId>`）。
//!
//! ── 这个接口做什么 ────────────────────────────────────────────
//! 把插件申报的 `invite.summary` 管理动作转成 HTTP：回该账号的邀请码、
//! 已邀人数、已到账积分与最近明细。纯读，不产生任何上游副作用。
//!
//! ── 为什么**不**提供「填邀请码」的写接口 ───────────────────────
//! 上游的 `POST /api/user/activate` 是一次性、不可撤销、且会给邀请方发真实
//! 积分的动作（客户端注释写明按 `device_identifier` 做「一机一名额」防刷）。
//! 做成 HTTP 写接口就意味着任何一次误点、重试或自动化调用都会永久烧掉一个
//! 账号的激活资格，而这类资格不可恢复。要接也应当由人在上游界面里显式完成，
//! 网关不该代劳 —— 所以这里只有读。
//!
//! ── 为什么不做「聚合全部账号」 ────────────────────────────────
//! 每查一个账号就要打一次上游；账号页一屏十几个账号会把上游打出限流。调用方
//! 明确指名一个账号（与 `/api/accounts/usage?id=` 同一取舍），要全量就自己
//! 按需逐个查。

use axum::response::Response;
use serde_json::Value;

use crate::server::errors::management_error;
use crate::server::http::ok_json;
use crate::server::ServerState;

/// `GET /api/accounts/invite?id=<accountId>`
pub async fn account_invite(state: &ServerState, query: &str) -> Response {
    let Some(id) = query_param(query, "id") else {
        return management_error(400, "缺少 id：邀请概览必须指明账号");
    };
    let id = id.trim();
    if id.is_empty() {
        return management_error(400, "缺少 id：邀请概览必须指明账号");
    }
    let Some(provider) = state.store().account_provider_of(id) else {
        return management_error(404, format!("账号不存在: {id}"));
    };
    let Some(adapter) = crate::server::core::providers::adapter::adapter_by_id(&provider) else {
        return management_error(
            503,
            format!("「{provider}」的提供商插件没有安装，无法查询邀请信息"),
        );
    };
    match adapter
        .admin_action(state.store(), id, "invite.summary", &Value::Null)
        .await
    {
        Ok(result) => ok_json(result),
        Err(error) => management_error(error.status_code, error.message),
    }
}

/// 从查询串里取一个参数（原样解码，不做 `+` → 空格，账号 id 里不会出现空格）。
fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=')?;
        if name == key {
            return Some(decode(value));
        }
    }
    None
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = &value[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
