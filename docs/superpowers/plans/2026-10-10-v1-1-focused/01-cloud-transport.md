# P1：云账号连接使用 HTTPS

> 状态：已完成。对应 R02；已落地 transport 规范化与构造 helper，7 处云连接点全部接入并经针对性集成测试验证。使用 executing-plans；未经用户明确告知，不使用子智能体。

## 最小改动和文件范围

CLIENT 根目录为 `F:\项目开发\忆涟千言\YiLianQianYan`。

新增 `backend/src/modules/cloud/transport.rs`，只放云服务地址解析和 HTTP client 构造。修改同目录 `mod.rs`、`client.rs::new`、`sync.rs::new`、`session.rs::remote_client`、`lifecycle.rs::new`、`conflict_service.rs::new` 和 `revocation.rs::send_remote_revoke` 的构造调用。它们均会向同一个用户配置的云服务发送认证信息，应采用同一规则。

不修改 AI provider、AnySearch、MCP、普通网页访问的连接规则。它们不是这次报告中的云账号连接入口。

## 函数契约与关键代码

```rust
// 新增在 modules/cloud/transport.rs。
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum CloudTransportError {
    #[error("invalid cloud service URL")]
    InvalidUrl,
    #[error("remote cloud service requires HTTPS")]
    HttpsRequired,
    #[error("cloud HTTP client unavailable")]
    ClientUnavailable,
}

pub fn normalize_cloud_base(raw: &str) -> Result<String, CloudTransportError> {
    let url = url::Url::parse(raw.trim()).map_err(|_| CloudTransportError::InvalidUrl)?;
    if url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(CloudTransportError::InvalidUrl);
    }
    let loopback = match url.host() {
        Some(url::Host::Domain("localhost")) => true,
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    match url.scheme() {
        "https" => {},
        "http" if loopback => {},
        "http" => return Err(CloudTransportError::HttpsRequired),
        _ => return Err(CloudTransportError::InvalidUrl),
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

pub fn build_cloud_http(
    raw: &str,
    timeout: Duration,
) -> Result<(reqwest::Client, String), CloudTransportError> {
    let base = normalize_cloud_base(raw)?;
    let http = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| CloudTransportError::ClientUnavailable)?;
    Ok((http, base))
}
```

这是局部复用函数，不是通用网络策略框架。输入保留已有 `/api/v1` 等 base path，输出是无结尾 `/` 的规范化 base。`localhost.evil.example` 和局域网 IP 都不属于回环例外；不为域名做 DNS 查询来“推断本机”。开发者可使用 `127.0.0.1` 或 `::1`。

原构造函数保留其公开签名。调用 `build_cloud_http(base_url, 原timeout)` 后直接赋值现有 `http`、`base_url` 字段。InvalidUrl 映射原 InvalidBaseUrl；HttpsRequired 保留为可辨识错误（对外 HTTP 400 `cloud_https_required`，提示“远程云服务地址必须使用 HTTPS”）；ClientUnavailable 映射原 Unavailable/Transport。P2 修改 logout 时复用 P1 构造，不再自行 `reqwest::Client::builder()`。

请求关系：

```text
login/register/refresh 或保存过的 cloud_base_url
  → build_cloud_http（在取出/发送凭据之前）
  → 现有 identity、sync、sessions、export、revoke 请求
  → 原响应处理（3xx 不是 success，返回明确错误）
```

禁止自动把 `http://` 改成 `https://` 后静默重试；不能把 TLS 失败当成回环例外。云服务地址正确时，原有认证 JSON 和协议头保持一致。

## 独立验证

新增 `backend/tests/v11_cloud_transport.rs`，覆盖下面的实际请求行为。测试使用现有 Tokio/Axum 临时回环服务，无新增测试框架。

```rust
#[test]
fn remote_plaintext_is_rejected_at_construction() {
    use yilian_backend::modules::cloud::client::CloudApiClient;
    assert!(CloudApiClient::new("http://cloud.example.com/api/v1").is_err());
    assert!(CloudApiClient::new("http://192.168.1.10:8080/api/v1").is_err());
    assert!(CloudApiClient::new("http://localhost.evil.example/api/v1").is_err());
    assert!(CloudApiClient::new("http://127.0.0.1:38081/api/v1").is_ok());
    assert!(CloudApiClient::new("http://[::1]:38081/api/v1").is_ok());
}
```

| 用例 | 输入/故障 | 必须断言 |
|---|---|---|
| P1-01 | 远程 HTTP 登录/刷新配置 | 构造即失败，无密码/refresh token 请求 |
| P1-02 | 回环 HTTP 登录和刷新 | 认证成功，已有 JSON 契约不变 |
| P1-03 | A 回环服务返回 307/308 到 B 回环服务 | B 请求计数为 0，认证失败，未落新 session |
| P1-04 | 同路径 302/303，或 HTTPS 配置收到重定向 | 不发第二次请求，不因重定向换 host/scheme |
| P1-05 | URL 内嵌用户名、密码、query、fragment | 输入校验失败，不把 URL 原文写日志 |
| P1-06 | 旧数据库保存了远程 HTTP 地址 | sessions/sync/export/revoke 入口同样拒绝，提示用户修正配置 |

P1-03 的关键测试流程：A/B 使用独立监听端口；B handler 内 `AtomicUsize::fetch_add`；向 A 登录后断言计数为零。不能只断言配置使用了 `Policy::none()`。

执行：

```powershell
cargo test -p yilian-backend --test v11_cloud_transport
cargo fmt
cargo fmt --check
cargo check --workspace
```

- [x] 先复现远程 HTTP 被接受与 307 转发行为。
- [x] 落地一个 helper，接入列出的七处云连接构造点。
- [x] 运行针对性测试并保留退出码。
- [x] 向 P2/P3 交付构造函数、错误映射和函数级补丁；不改它们的业务算法。

完成即停止；不加入证书固定、TLS 库替换、企业代理管理或全局 URL 拦截。
