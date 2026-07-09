-- 文件作用: SkillHub 初始数据库结构(10 张表), 遵循阿里巴巴泰山版规约
-- 创建日期: 2026-07-09

-- 资源表: SkillHub 托管的 Skill/MCP 元数据
CREATE TABLE resource (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  res_type      INTEGER  NOT NULL DEFAULT 1,                 -- 类型: 1-Skill, 2-MCP
  name          TEXT     NOT NULL DEFAULT '',                -- 资源名(唯一标识, 业务长度<=64)
  display_name  TEXT     NOT NULL DEFAULT '',                -- 展示名(<=128)
  version       TEXT     NOT NULL DEFAULT '',                -- 版本号(<=32)
  source_type   INTEGER  NOT NULL DEFAULT 0,                 -- 来源: 0-本地导入, 1-官方仓库, 2-第三方仓库
  local_path    TEXT     NOT NULL DEFAULT '',                -- 本地路径(<=512)
  enabled       INTEGER  NOT NULL DEFAULT 1,                 -- 是否启用: 0-禁用, 1-启用
  create_time   TEXT     NOT NULL DEFAULT (datetime('now')), -- 创建时间
  update_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_resource_type_name ON resource (res_type, name);  -- 同类型下名称唯一
CREATE INDEX ix_resource_enabled ON resource (enabled);

-- Agent 表: 检测到的本机 AI 工具实例
CREATE TABLE agent (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  agent_kind     INTEGER  NOT NULL DEFAULT 0,                 -- 工具: 1-ClaudeCode,2-ClaudeDesktop,3-Cursor,4-Windsurf,5-Cline,6-VSCode,7-GeminiCli,8-Codex
  name           TEXT     NOT NULL DEFAULT '',                -- 显示名(<=64)
  config_path    TEXT     NOT NULL DEFAULT '',                -- 配置文件路径(<=512)
  scope          INTEGER  NOT NULL DEFAULT 0,                 -- 作用域: 0-全局, 1-项目
  status         INTEGER  NOT NULL DEFAULT 0,                 -- 状态: 0-离线/不可用, 1-在线/可用
  last_sync_time TEXT     NOT NULL DEFAULT '',                -- 最后同步时间(空串=从未)
  create_time    TEXT     NOT NULL DEFAULT (datetime('now')), -- 创建时间
  update_time    TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_agent_kind_path ON agent (agent_kind, config_path);  -- 同类型下配置路径唯一
CREATE INDEX ix_agent_status ON agent (status);

-- 资源-Agent 关联表: 期望态 + 上次应用指纹
CREATE TABLE resource_agent (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  resource_id   INTEGER  NOT NULL DEFAULT 0,                 -- 资源 id(应用层关联 resource.id)
  agent_id      INTEGER  NOT NULL DEFAULT 0,                 -- Agent id(应用层关联 agent.id)
  desired       INTEGER  NOT NULL DEFAULT 1,                 -- 期望态: 0-不应存在, 1-应存在
  applied_hash  TEXT     NOT NULL DEFAULT '',                -- 上次成功应用的内容指纹(漂移检测)
  sync_status   INTEGER  NOT NULL DEFAULT 0,                 -- 同步状态: 0-待同步,1-已同步,2-本地修改,3-同步失败,4-已禁用
  create_time   TEXT     NOT NULL DEFAULT (datetime('now')), -- 创建时间
  update_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_resource_agent_rid_aid ON resource_agent (resource_id, agent_id);  -- 资源+Agent 唯一
CREATE INDEX ix_resource_agent_agent ON resource_agent (agent_id);

-- 同步运行表: 一次同步操作的汇总
CREATE TABLE sync_run (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  scope_type    INTEGER  NOT NULL DEFAULT 0,                 -- 范围: 0-全部Agent,1-单Agent,2-选择集
  agent_id      INTEGER  NOT NULL DEFAULT 0,                 -- 目标 Agent id(0=多个)
  total_cnt     INTEGER  NOT NULL DEFAULT 0,                 -- 总项数
  success_cnt   INTEGER  NOT NULL DEFAULT 0,                 -- 成功数
  failed_cnt    INTEGER  NOT NULL DEFAULT 0,                 -- 失败数
  skipped_cnt   INTEGER  NOT NULL DEFAULT 0,                 -- 跳过数
  status        INTEGER  NOT NULL DEFAULT 0,                 -- 状态: 0-进行中,1-成功,2-部分成功,3-失败
  run_time      TEXT     NOT NULL DEFAULT (datetime('now')), -- 运行时间
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE INDEX ix_sync_run_agent ON sync_run (agent_id);

-- 同步明细表: 一次同步中每个资源项的结果
CREATE TABLE sync_item (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  run_id        INTEGER  NOT NULL DEFAULT 0,                 -- 所属 sync_run id
  resource_id   INTEGER  NOT NULL DEFAULT 0,                 -- 资源 id
  agent_id      INTEGER  NOT NULL DEFAULT 0,                 -- Agent id
  action        INTEGER  NOT NULL DEFAULT 0,                 -- 动作: 1-新增,2-更新,3-移除
  local_ver     TEXT     NOT NULL DEFAULT '',                -- 本地版本
  agent_ver     TEXT     NOT NULL DEFAULT '',                -- Agent 侧版本
  result        INTEGER  NOT NULL DEFAULT 0,                 -- 结果: 0-待处理,1-成功,2-失败,3-跳过
  err_msg       TEXT     NOT NULL DEFAULT '',                -- 失败信息(<=512)
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE INDEX ix_sync_item_run ON sync_item (run_id);

-- 市场缓存表: 归一化后的市场资源缓存
CREATE TABLE market_cache (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  source_type   INTEGER  NOT NULL DEFAULT 0,                 -- 来源: 1-github_skills,2-mcp_registry,3-github_mcp
  res_type      INTEGER  NOT NULL DEFAULT 1,                 -- 类型: 1-Skill,2-MCP
  ext_id        TEXT     NOT NULL DEFAULT '',                -- 来源内唯一标识(<=128)
  name          TEXT     NOT NULL DEFAULT '',                -- 资源名(<=64)
  author        TEXT     NOT NULL DEFAULT '',                -- 作者(<=64)
  stars         INTEGER  NOT NULL DEFAULT 0,                 -- 星标数
  category      TEXT     NOT NULL DEFAULT '',                -- 分类(<=32)
  auth_required INTEGER  NOT NULL DEFAULT 0,                 -- 是否需认证: 0-否,1-是
  raw_json      TEXT     NOT NULL DEFAULT '',                -- 归一化载荷(缓存用)
  etag          TEXT     NOT NULL DEFAULT '',                -- HTTP ETag(增量刷新)
  fetch_time    TEXT     NOT NULL DEFAULT (datetime('now')), -- 拉取时间
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE UNIQUE INDEX uk_market_cache_src_ext ON market_cache (source_type, ext_id);  -- 来源+外部id唯一
CREATE INDEX ix_market_cache_type ON market_cache (res_type);

-- 认证账号表: 已连接的第三方账号(令牌存系统钥匙串, 此处不存密文)
CREATE TABLE auth_account (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  provider      INTEGER  NOT NULL DEFAULT 0,                 -- 提供方: 1-GitHub,2-Google,3-Microsoft,4-访问令牌
  account       TEXT     NOT NULL DEFAULT '',                -- 账号标识/邮箱(<=128)
  scope         TEXT     NOT NULL DEFAULT '',                -- 授权范围(<=256)
  keyring_ref   TEXT     NOT NULL DEFAULT '',                -- 钥匙串条目引用键(<=128)
  status        INTEGER  NOT NULL DEFAULT 1,                 -- 状态: 0-已断开,1-已连接
  connect_time  TEXT     NOT NULL DEFAULT (datetime('now')), -- 连接时间
  update_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_auth_account_prov_acc ON auth_account (provider, account);  -- 提供方+账号唯一

-- 导入导出历史表
CREATE TABLE import_export_log (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  direction     INTEGER  NOT NULL DEFAULT 0,                 -- 方向: 0-导出,1-导入
  file_name     TEXT     NOT NULL DEFAULT '',                -- 文件名(<=256)
  file_format   INTEGER  NOT NULL DEFAULT 1,                 -- 格式: 1-zip,2-json,3-tar
  summary       TEXT     NOT NULL DEFAULT '',                -- 内容摘要(<=256)
  status        INTEGER  NOT NULL DEFAULT 0,                 -- 状态: 0-失败,1-成功,2-部分成功
  run_time      TEXT     NOT NULL DEFAULT (datetime('now')), -- 运行时间
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE INDEX ix_import_export_log_dir ON import_export_log (direction);

-- 设置表: 键值对(存储目录/同步偏好/网络代理/更新通道)
CREATE TABLE setting (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  cfg_key       TEXT     NOT NULL DEFAULT '',                -- 配置键(<=64)
  cfg_value     TEXT     NOT NULL DEFAULT '',                -- 配置值(<=1024)
  create_time   TEXT     NOT NULL DEFAULT (datetime('now')), -- 创建时间
  update_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 更新时间
);
CREATE UNIQUE INDEX uk_setting_key ON setting (cfg_key);  -- 配置键唯一

-- 活动流表: 首页"最近变更"来源
CREATE TABLE activity_log (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,           -- 主键
  act_type      INTEGER  NOT NULL DEFAULT 0,                 -- 类型: 1-新增,2-更新,3-下载,4-导入,5-导出,6-同步,7-卸载
  res_type      INTEGER  NOT NULL DEFAULT 0,                 -- 关联类型: 0-无,1-Skill,2-MCP,3-配置,4-Agent
  title         TEXT     NOT NULL DEFAULT '',                -- 标题(<=128)
  detail        TEXT     NOT NULL DEFAULT '',                -- 详情(<=256)
  create_time   TEXT     NOT NULL DEFAULT (datetime('now'))  -- 创建时间
);
CREATE INDEX ix_activity_log_time ON activity_log (create_time);
