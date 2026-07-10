# OAuth 应用注册指南

SkillHub 支持通过 GitHub / Google / Microsoft 三家的 OAuth(PKCE)登录以连接账号，也支持手动
粘贴访问令牌(Personal Access Token)作为替代方式。三家的 client_id 目前在代码里是占位常量，
Charles 需要分别注册好各自的 OAuth 应用后回填真实值，应用内 OAuth 弹窗登录才能真正跑通。

本文档由 Task 7 起草骨架；本次(Task 12, M2 集成收口)已对照三家官方最新文档逐条核实并补全下面
的注册步骤、redirect_uri 配置结论与已知疑点。但受限于当前没有任何一家的真实 client_id，本次
核实止步于"文档层面确认"，尚未做过一次真机端到端登录；请 Charles 注册好应用、回填 client_id
后实测一次，重点关注下面每家小节里单独列出的疑虑。

## 应用内登录的本地回调(loopback)工作方式

`commands::auth::auth_login` 每次登录都会：在本机随机找一个空闲端口起一个 TCP 监听、拼出
`http://127.0.0.1:<该端口>/callback` 作为 redirect_uri(`services::auth::build_redirect_uri`，
三家 provider 目前共用同一份实现，都是 127.0.0.1)、带上这个 redirect_uri 与 PKCE 挑战打开一个
WebviewWindow 承载对应 provider 的真实授权页、阻塞等待恰好一次回调、拿到 code 后换取 token。

三家 provider 对"注册时填的 callback 地址"与"实际运行时用的地址(尤其端口)"是否必须精确匹配，
结论并不相同，这直接决定了 Charles 该在各家后台填什么。结论分别写在下面三节里，这里先给统一
结论：GitHub 与 Google 均官方明确支持 loopback 地址端口在运行时与注册时不同，与 SkillHub 现在
"每次登录随机换端口"的实现直接吻合；Microsoft 有一处需要额外注意的差异，见 Microsoft 一节。

## client_id 常量填写位置

三个占位常量都在 `src-tauri/src/services/auth.rs` 文件顶部：

- `GITHUB_CLIENT_ID`
- `GOOGLE_CLIENT_ID`
- `MICROSOFT_CLIENT_ID`

把对应值从 `REPLACE_WITH_..._OAUTH_CLIENT_ID` 改成注册后拿到的真实 client_id 即可，无需改动
其它代码。

## GitHub

1. 打开 GitHub -> 右上角头像 -> Settings -> Developer settings -> OAuth Apps -> New OAuth
   App。注意这里要注册的是经典的 "OAuth App"，不是 "GitHub App"，二者是 GitHub 两套不同的
   应用体系，本仓库 `services::auth` 走的授权码+PKCE 流程对应前者。
2. Application name 随意；Homepage URL 填仓库地址或占位地址均可(GitHub 不校验其可达性)。
3. Authorization callback URL 填 `http://127.0.0.1:8765/callback`(端口号本身只是一个占位值，
   随便选一个未被常用服务占用的端口即可，8765 只是示例)。已核实 GitHub 官方文档对 loopback
   地址的匹配规则：host 是 127.0.0.1(或 IPv6 的 ::1)时，实际回调用的端口不要求和这里注册的
   端口一致；GitHub 官方文档本身也推荐 loopback 场景优先用 127.0.0.1/::1 而不是 localhost。
   path 建议保持 `/callback`，与代码里固定的路径一致。
4. 创建后立即显示 Client ID，填入 `GITHUB_CLIENT_ID`。
5. PKCE 现状(已核实，此前草稿里的疑虑现已解除)：GitHub 从 2025 年 7 月起为 OAuth App 与
   GitHub App 正式支持 PKCE(见 GitHub Changelog"PKCE support for OAuth and GitHub App
   authentication")，公开客户端只要在授权请求里带 code_challenge/code_challenge_method、在
   换取 token 时带对应 code_verifier，即可完成整个流程而不需要 client_secret；GitHub 官方原文
   明确写道这类客户端"do not require a client_secret"。本仓库 `services::auth::exchange_code`
   对 GitHub 走的正是这条路径，没有发送 client_secret。这项能力上线不到一年，建议注册好应用后
   仍实测一次登录做最终确认。
6. scope：当前固定申请 `read:user`(仅用于登录后识别用户名)。市场刷新(`market_refresh`)的
   GitHub 限流提额本身不需要任何额外 scope，同一个已连接账号的令牌可以直接复用于"登录识别"和
   "刷新提额"两个用途，不需要为提额单独再连一次、再要更高权限的 scope。

## Google

1. 打开 Google Cloud Console -> 选择或新建一个项目 -> APIs & Services -> Credentials ->
   Create Credentials -> OAuth client ID -> Application type 选 "Desktop app"。
2. redirect_uri：与 GitHub/Microsoft 不同，创建 Desktop app 类型的客户端这一步不需要手动填写
   任何重定向 URI(这是 Web application 类型才有的必填项)。已核实 Google 官方文档明确写道
   Desktop app 客户端天然支持 loopback 回环地址、运行时端口任意，允许的形式包括
   `http://127.0.0.1:<port>`、`http://[::1]:<port>`、`http://localhost:<port>`，与本仓库
   "每次登录随机取一个空闲端口"的实现直接吻合，这一步反而是三家里最省心的。
3. 创建后得到 Client ID(以及一个 Client Secret，见下面第 4 点)，把 Client ID 填入
   `GOOGLE_CLIENT_ID`。
4. 疑虑(client_secret，未解决，需要 Charles 实测确认)：创建 Desktop app 类型客户端时 Google
   会同时生成一个 client_secret。Google 官方"换取 token"参数表明确写道 client_secret 对
   Android/iOS/Chrome 应用类型"不适用"，但没有把 Desktop app 一并列进这份豁免名单——这提示
   Desktop app 类型很可能仍然要求换取 token 的请求里带上 client_secret(即便 Google 自己的
   文档也承认这类客户端不适合把它当真正的机密看待)。本仓库 `services::auth::exchange_code`
   目前对三家 provider 统一不发送 client_secret；如果上面的推断成立，Google 登录会卡在换取
   token 这一步(GitHub 因为 2025 年新支持的纯 PKCE 流程不受此影响，不代表 Google 也一样)。
   建议 Charles 注册好应用后先实测一次；如果确认必须带 client_secret，需要后续给
   `exchange_code` 增加一个按 provider 可选传入的 client_secret 参数(不要把它硬编码进源码，
   应从本地配置文件或系统钥匙串读取，与仓库现有"令牌只进钥匙串、不落库不硬编码"的原则一致)。
5. scope：当前固定申请 `openid email profile`。
6. 疑虑(refresh token，本次核对代码时发现的一处文档表述纠正)：本文档此前的 Task 7 草稿曾写
   "已在 authorize_url 的 scope/参数里按需处理 access_type=offline"；本次核对
   `services::auth::authorize_url` 源码，实际并未追加 `access_type=offline` 这个查询参数，
   此前的表述不准确，特此更正。Google 只有在首次授权且带 `access_type=offline` 时才会返回
   refresh_token，当前实现不带这个参数，意味着 Google 登录仍能正常拿到 access_token，但大概率
   拿不到 refresh_token(`TokenSet.refresh` 会是 None，不会报错，只是长期免登录做不到)。是否
   需要专门为 Google 补上 `access_type=offline`(以及按需补 `prompt=consent`)，取决于产品是否
   需要 Google 长期免登录；需要的话请回到 `authorize_url` 里按 provider 分支加这个参数。

## Microsoft

1. 打开 Microsoft Entra 管理中心(entra.microsoft.com) -> 应用注册 -> 新注册；支持的账户类型
   按需选择(个人/组织/两者皆可)。
2. 重定向 URI 的平台类型选 "公共客户端/本机(移动和桌面)"("Mobile and desktop applications")。
3. 疑虑(redirect_uri host，未解决，三家里最需要注意的一处差异)：已核实 Microsoft 官方文档
   明确写道，应用注册门户里的"重定向 URI"文本框不允许直接填一个使用 http 协议的 127.0.0.1
   地址(会被界面直接拒绝，文档配了报错截图为证)，要用 127.0.0.1 必须改去编辑应用清单
   (application manifest)里的 replyUrlsWithType 属性才能加进去；门户文本框能直接填、且官方
   文档明确写"端口在匹配时会被忽略"的，是 `http://localhost`(可以不带端口，也可以带一个随便
   的端口，效果等价)这一形式，而不是 127.0.0.1。本仓库 `services::auth::build_redirect_uri`
   对三家 provider 是同一份实现，固定用 127.0.0.1，没有为 Microsoft 单独处理这个差异——也就是
   说，即便照上面步骤注册好 client_id，应用内 OAuth 弹窗登录 Microsoft 这条路径大概率会在
   redirect_uri 不匹配这一步失败(报错通常是 AADSTS50011)。本任务未做代码改动，只记录清楚两个
   可行方向，供 Charles 决策：
   - 方案 A(更简单，建议先尝试)：门户里直接填 `http://localhost/callback`；同时把
     `build_redirect_uri` 改成按 provider 分流 —— Microsoft 用 localhost，GitHub/Google 仍用
     127.0.0.1(这两家官方文档都明确支持、且是各自推荐的写法，不用跟着改)。
   - 方案 B：去应用清单里手动加一条 `http://127.0.0.1:<固定端口>/callback`；但官方文档没有
     说明这种通过清单加进去的 127.0.0.1 地址是否也享有"端口无关"的匹配待遇，需要 Charles 实测
     确认。如果端口必须精确匹配，SkillHub 就不能再对 Microsoft 使用随机空闲端口，得改成固定
     监听某个端口(该端口被其它程序占用时登录会失败，是随机端口方案的一个折衷)。
4. 创建后得到"应用程序(客户端) ID"，填入 `MICROSOFT_CLIENT_ID`。
5. scope：当前固定申请 `openid email profile User.Read offline_access`。`offline_access` 就是
   Microsoft 拿到 refresh_token 的方式(与 Google 靠查询参数不同，Microsoft 靠 scope 声明即可
   拿到)，这部分现有实现已经是对的，不需要额外处理。

## 访问令牌(Personal Access Token)方式

三家都支持在不走 OAuth 弹窗的情况下，手动粘贴一个已有的访问令牌(调用
`auth_enter_token(provider, token)` 命令)，后端会调用对应品牌的身份接口校验并取回账号标识：

- GitHub：`GET https://api.github.com/user`(PAT 需要至少能读到用户信息的权限)
- Google：`GET https://openidconnect.googleapis.com/v1/userinfo`
- Microsoft：`GET https://graph.microsoft.com/v1.0/me`

这条路径不需要注册 OAuth 应用、不需要 client_id/client_secret、不涉及上面任何一条 redirect_uri
规则，适合在 Charles 还没走完注册流程、或还没解决上面列出的疑虑之前，先临时连上账号做验证。
本次(Task 12)接入的 `market_refresh` GitHub 限流提额同样能通过这条路径连接的账号生效——它只是
调用 `auth::token_for` 取当前已连接账号的令牌，不关心这个令牌当初是通过 OAuth 弹窗还是 PAT
录入拿到的。
