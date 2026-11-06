# QuWork 邀请接口说明

本文记录 QuWork（rrzu 网关）邀请体系的接口形态、字段语义与实测结论。前半部分是从客户端
`app.asar` 逆向出的上游契约，后半部分是网关侧为它加的封装。

> **一句话结论**：邀请码在注册时就能拿到，但**邀请是否算数取决于被邀请方有没有真的消耗积分**
> （`valid_invite`）。这一点决定了「互邀」这类批量注册拿积分的做法在上游眼里是无效的。

---

## 一、上游契约（从客户端逆向）

### 1.1 邀请概览

```http
GET {ACCOUNT_BASE}/api/user/invite/summary
Authorization: Bearer <accessToken(JWT)>
```

```json
{ "data": { "invite_code": "580085", "invited_count": 1, "points": 1000, "quota": 5000000 },
  "success": true }
```

| 字段 | 含义 |
| --- | --- |
| `invite_code` | 该账号的邀请码（6 位数字，注册时由上游分配，不可更改） |
| `invited_count` | 已邀请人数（**含无效邀请**） |
| `points` | 已到账积分 |
| `quota` | 已到账积分折算的 token 额度（1000 积分 = 5,000,000） |

### 1.2 邀请记录

```http
GET {ACCOUNT_BASE}/api/user/invite/records?page=1&page_size=20
Authorization: Bearer <accessToken>
```

```json
{ "data": { "list": [ /* 见下 */ ], "page": 1, "page_size": 20, "total": 1 },
  "success": true }
```

单条记录（实测完整字段）：

```json
{
  "id": 835,
  "invite_code": "580085",          // 被填写的邀请码
  "invitee_phone": "13119164021",   // 被邀请人
  "invitee_user_id": 4081,
  "inviter_phone": "18998864021",   // 邀请人
  "inviter_user_id": 3962,
  "inviter_points": 1000,           // 记给邀请人的积分
  "inviter_quota": 5000000,
  "invitee_points": 1000,           // 记给被邀请人的积分
  "invitee_quota": 5000000,
  "invite_level": 1,                // 1=直接邀请，2=间接邀请（A→B→C 中的 C 也算 A 的间接）
  "is_external_user": 1,
  "inviter_is_sales": 0,
  "device_dup": 0,                  // 设备重复标记
  "first_consume_at": 0,            // 被邀请方首次消耗积分的时刻（0 = 从未消耗）
  "valid_invite": 0,                // ★ 见下节
  "created_at": 1793941908
}
```

### 1.3 ★ `valid_invite` 是什么

从客户端渲染层反查（`dist/assets/main-*.js`），这一列在邀请记录表里的**表头是「是否消耗积分」**：

```js
n.valid_invite === 1
  ? `是`   // 绿色高亮
  : `否`   // 灰色
```

| 值 | 界面 | 含义 |
| --- | --- | --- |
| `1` | 绿色「是」 | 被邀请方**产生过真实用量**（消耗过积分）→ 邀请有效 |
| `0` | 灰色「否」 | 被邀请方从未消耗积分 → 邀请**不计入** |

判据实体是 `first_consume_at`：
- `first_consume_at == 0` → 从未消耗 → `valid_invite = 0`
- `first_consume_at > 0` → 有过消耗 → `valid_invite = 1`

**关键结论**：积分在注册时**当场记到账**（`inviter_points` 已给），但邀请是否「算数」要等
被邀请方第一次真正发起对话、消耗积分。**注册完躺着不用的号，邀请永远无效。**

同一张表里还有一列「是否新设备」（`is_new_device`），说明上游按设备做过注册判重；
另有 `device_dup` 字段（接口返回，界面未展示）。

### 1.4 其它记录维度

```http
GET {ACCOUNT_BASE}/api/user/invite/records/activity?page=1&page_size=20
GET {ACCOUNT_BASE}/api/user/invite/records/activity/orders?page=1&page_size=20
GET {ACCOUNT_BASE}/api/user/invite/records/activity/recharges?page=1&page_size=20
```

分别对应「活动参与 / 订单 / 充值」三类记录，均带 `is_staff_view`（内部视角开关）。

### 1.5 填写邀请码（**写操作，不可撤销**）

```http
POST {ACCOUNT_BASE}/api/user/activate
Authorization: Bearer <accessToken>
Content-Type: application/json

{ "invite_code": "580085", "device_identifier": "<64位小写hex>", "device_uuid": "" }
```

成功：`{"data":{"sales_bound":false},"success":true}`

- **一次性**：同一账号只能成功一次，成功后 `inviter_id` 永久固定，无撤销接口
- 未登录直接 401；填错码回 `{"message":"…","success":false}`（HTTP 仍是 200）
- `device_identifier` 由客户端算法派生（`sha256({ns:"quwork.device-identifier",version:1,platform,source,value})`），
  用于「一机一名额」防刷

---

## 二、网关侧封装（本项目）

### 2.1 查询邀请概览（只读）

```http
GET /api/accounts/invite?id=<accountId>
Cookie: <面板会话>
```

```json
{ "data": { "inviteCode": "580085", "invitedCount": 1, "points": 1000, "quota": 5000000,
            "records": [ /* 上游 list 原样透出 */ ] },
  "success": true }
```

- 必须用 `?id=` 指名账号（与 `/api/accounts/usage?id=` 同一取舍）：一屏十几个账号逐个查会把上游打出限流
- 实现：插件 `invite.summary` 动作 → 上游 `summary` + `records` 两个请求
- 明细拿不到时降级为空数组，概览仍返回

### 2.2 填写邀请码（**写操作，不可撤销**）

```http
POST /api/accounts/invite/activate
Cookie: <面板会话>
Content-Type: application/json

{ "id": "<accountId>", "inviteCode": "580085" }
```

```json
{ "data": { "activated": true, "inviteCode": "580085", "result": { "sales_bound": false } },
  "success": true }
```

**实现上刻意保留的三条约束**：

1. **不重试**：只有一次 POST。网络失败直接返回错误，由调用方决定是否人工重试
   —— 自动重试可能重复消耗资格
2. **不批量**：一次一个账号、一个邀请码，必须显式指名
3. **不绕过风控**：`device_identifier` 按客户端算法如实生成，**没有**「传空串跳过设备校验」
   的开关

为什么读写分成两条路由：`GET .../invite` 是「看一眼我的码」，随时可调；`POST .../activate`
会永久消耗目标账号的激活资格并给邀请方发真实积分。放进同一个 GET 里，等于让「刷新页面」
也可能触发写操作。

---

## 三、实测记录

| 时间 | 操作 | 结果 |
| --- | --- | --- |
| 2026-11-06 | `13119164021` 使用 `580085`（`18998864021` 的码） | 200，邀请方 +1000 积分 / +5,000,000 quota |
| 同上 | 查询记录 | `valid_invite: 0`、`first_consume_at: 0` —— **账面到账但邀请无效** |

**解读**：积分在注册/激活时当场记账，但 `valid_invite` 要等被邀请方真实消耗积分才转正。
同设备多号互邀的结果是「无效邀请堆积」。

---

## 四、未确认项

- `device_dup` 的具体触发条件（本次实测为 0，没有触发）
- `is_new_device` 的判据（上游按什么口径认定「新设备」）
- `invite_level: 2`（间接邀请）的触发条件与奖励是否相同
- `valid_invite` 转正后，账面积分是否有额外的结算动作（目前看注册时已记账）

---

## 五、相关代码落点

| 层 | 文件 |
| --- | --- |
| 插件动作 | `crates/quwork/src/quwork.rs`（`invite.summary` / `invite.activate`） |
| 网关路由 | `api/accounts_invite.rs`（只读）、`api/accounts_invite_activate.rs`（写） |
| 路由挂载 | `api/accounts.rs` 的 `GET invite` / `POST invite/activate` 两个分支 |
