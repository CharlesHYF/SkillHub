# OAuth 应用注册指南

SkillHub 支持通过 GitHub / Google / Microsoft 三家的 OAuth(PKCE)登录以连接账号，也支持手动
粘贴访问令牌(Personal Access Token)作为替代方式。三家的 `client_id` 目前在代码里是占位常量，
Charles 需要分别注册好各自的 OAuth 应用后回填真实值，应用才能真机完成登录。

本文档由 Task 7 起草骨架，Task 12(M2 集成收口)会补全真机验证过的细节；三家的具体步骤/截图/
callback 精确格式届时可能还需微调。

## client_id 常量填写位置

三个占位常量都在 `src-tauri/src/services/auth.rs` 文件顶部：

- `GITHUB_CLIENT_ID`
- `GOOGLE_CLIENT_ID`
- `MICROSOFT_CLIENT_ID`

把对应值从 `REPLACE_WITH_..._OAUTH_CLIENT_ID` 改成注册后拿到的真实 client_id 即可，无需改动
其它代码。

## 回调地址(redirect_uri)说明

应用内 OAuth 弹窗的本地回调捕获(loopback)由 Task 8 实现：Tauri 开一个 WebviewWindow 加载
授权页，本机 `127.0.0.1` 上的一个临时端口监听回调，从中取出 `code`/`state`。三家平台通常都
要求回调地址在注册时精确匹配（或至少匹配到某种通配规则），Task 8 定下具体端口/路径方案后，
需要回到本文档补充每家平台"回调地址应该填什么、是否支持动态端口"的确切结论。

在 Task 8 完成前，先按各家平台惯例，注册时填一个开发期占位回调（如
`http://127.0.0.1:8765/callback`），后续如需调整再回来改注册信息。

## GitHub

1. 打开 GitHub -> Settings -> Developer settings -> OAuth Apps -> New OAuth App。
2. Homepage URL 填仓库地址或占位地址均可（无强约束）。
3. Authorization callback URL 按上面"回调地址说明"填。
4. 创建后得到 Client ID，填入 `GITHUB_CLIENT_ID`。

已知限制（重要，注册前请自行核实 GitHub 当前文档）：GitHub 的经典 OAuth App 授权码流程历史上
要求携带 `client_secret` 才能换取 token，桌面/公共客户端常用的纯 PKCE(无 secret)流程是否已被
支持，需要在注册时对照 GitHub 官方文档确认。若确认不支持纯 PKCE，可选方案：

- 改用 GitHub App（而非 OAuth App）注册，二者的令牌获取流程与权限模型不同；或
- 接受在桌面应用内保留一个 client_secret（业界对"公共客户端"的 secret 不当作机密看待，只
  依赖 PKCE 的 code_verifier 防护，但仍建议先确认这是否符合期望的安全基线）。

本仓库当前的 `services::auth::exchange_code` 实现不发送 `client_secret`；如上述确认后发现必须
携带，需要在该函数里补一个 secret 参数（不要把 secret 硬编码进源码，应从本地配置或系统钥匙串
读取）。

## Google

1. 打开 Google Cloud Console -> APIs & Services -> Credentials。
2. 创建 OAuth 2.0 客户端 ID，应用类型选"桌面应用"或"Web 应用"（取决于 Task 8 最终的回调方案，
   桌面应用类型通常对 `http://127.0.0.1:*` 回调更宽松）。
3. 按上面"回调地址说明"填授权重定向 URI。
4. 创建后得到客户端 ID，填入 `GOOGLE_CLIENT_ID`。
5. 若需要拿到 refresh token，注意 Google 要求首次授权时带 `access_type=offline`（已在
   `services::auth::authorize_url` 的 scope/参数里按需处理，如未生效可对照 Google 文档核对
   query 参数）。

## Microsoft

1. 打开 Microsoft Entra 管理中心(Entra admin center) -> 应用注册 -> 新注册。
2. 支持的账户类型按需选择（个人/组织/两者皆可）。
3. 按上面"回调地址说明"填重定向 URI，平台类型选"公共客户端/本机(移动和桌面)"。
4. 创建后得到应用(客户端) ID，填入 `MICROSOFT_CLIENT_ID`。

## 访问令牌(Personal Access Token)方式

三家都支持在不走 OAuth 弹窗的情况下，手动粘贴一个已有的访问令牌（调用
`auth_enter_token(provider, token)` 命令），后端会调用对应品牌的身份接口校验并取回账号标识：

- GitHub：`GET https://api.github.com/user`（PAT 需要至少能读到用户信息的权限）
- Google：`GET https://openidconnect.googleapis.com/v1/userinfo`
- Microsoft：`GET https://graph.microsoft.com/v1.0/me`

这条路径不需要注册 OAuth 应用、不需要 `client_id`，适合还没走完上面注册流程时先临时接入。
