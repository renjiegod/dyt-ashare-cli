<div align="center">

# 🚀 DYT AShare CLI (`dytcn`)

**面向 A 股高频分钟级历史数据研究与量化 AI Agent 的官方现代化 Rust 命令行工具**

[![Rust](https://img.shields.io/badge/rust-1.88%2B%20(2024)-orange.svg?style=flat-square)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg?style=flat-square)](LICENSE)
[![Security: Ed25519 Signed](https://img.shields.io/badge/security-Ed25519%20Signed-blueviolet.svg?style=flat-square)](#-安全机制与自愈更新体系)
[![Platform](https://img.shields.io/badge/platform-macOS%20ARM64%20%7C%20Linux%20x86_64-lightgrey.svg?style=flat-square)](#-安装指南)
[![Zero Dependency](https://img.shields.io/badge/dependencies-zero%20runtime-success.svg?style=flat-square)](#-核心亮点)

<p align="center">
  <a href="#-核心亮点">核心亮点</a> •
  <a href="#-安装指南">安装指南</a> •
  <a href="#-快速上手">快速上手</a> •
  <a href="#-ai-agent-深度赋能">Agent Skills</a> •
  <a href="#-核心命令详解">命令参考</a> •
  <a href="#-a-股交易特性与数据契约规范">数据规范</a> •
  <a href="#-高级配置与环境变量">配置说明</a>
</p>

</div>

> **一句话定位**：A 股数据查询、分钟线分析、质量检查与 AI Agent 接入的一体化 CLI。安装一个 `dyt-ashare` Skill，就能覆盖从行情检索到研究分析的完整工作流。

---

## 🌟 核心亮点

`dytcn` 把数据、分析和 Agent 操作收进一个可验证、可更新的命令行工具：

| 核心亮点 | 用户得到什么 |
|:---|:---|
| **安装即用** | 单个预编译 CLI，无需配置 Python、Pandas 或 C++ 运行库；安装后自动配置 Skill。 |
| **一个 Skill 覆盖 A 股工作流** | `dyt-ashare` 统一路由行情、历史分钟线、覆盖度、数据质量、技术分析和研究请求。 |
| **Agent 原生适配** | 自动写入 canonical Skill，并为 Claude Code、Codex、Cursor、Windsurf、OpenCode 等目录建立链接。 |
| **结果可审计** | 数据集保留版本、字节数和 SHA256；分析结果带输入哈希、时间范围和质量指标。 |
| **本地分析零额度** | 下载后的分钟数据可离线完成统计与质量检查，不额外消耗服务端配额。 |
| **签名更新可回滚** | Ed25519 验签、原子激活、旧版本保留，更新失败自动回到可用版本。 |
| **JSON 优先** | stdout 输出结构化 JSON，错误和诊断写入 stderr，适合脚本、CI 和 Agent 调用。 |

---

## 📦 安装指南

用户安装使用签名预编译包，不需要安装 Rust、Cargo、Python 或 C++ 运行库。当前 stable 通道支持 macOS Apple Silicon 和 Linux x86_64。

```bash
# macOS Apple Silicon 或 Linux x86_64
curl -fsSL https://data.doyoutrade.cloud/releases/dyt-ashare/install.sh | sh

# 安装器会将目录写入当前 Shell 配置；重新加载当前 shell
exec $SHELL

# 验证安装
dytcn --version
```

如需保持安装器不修改 shell 配置，可使用：

```bash
curl -fsSL https://data.doyoutrade.cloud/releases/dyt-ashare/install.sh | sh -s -- --no-modify-path
```

安装脚本会下载目标平台的签名 manifest，校验 Ed25519 签名、字节数和 SHA256 后激活 CLI，并自动安装统一的 `dyt-ashare` Skill。首次安装会把源文件写入 `~/.agents/skills/dyt-ashare`；检测到 Claude Code、Codex、Cursor 等 Agent 目录时，会自动建立相对链接。

安装完成后可检查版本、服务来源、更新状态和 Skill 状态：

```bash
dytcn doctor
```

Windows MSVC 资产尚未进入 stable 通道，PowerShell 安装入口将在对应预编译包发布后补充。

---

## ⚡ 快速上手

以下是使用 `dytcn` 进行 A 股数据检索、质检与 AI 赋能的典型全流程：

```bash
# 1. 登录服务端账户（终端交互静默输入，密钥不入命令行历史）
dytcn auth login

# 2. 查询当前账户权益与服务配额余量
dytcn usage

# 3. 探查指定股票的数据覆盖区间与不可变版本
dytcn coverage --symbol 600000.SH --from 2026-06-01 --to 2026-06-30

# 4. 查询高频分钟级 K 线数据并原子保存至本地
dytcn minutes --symbol 600000.SH --from 2026-06-01 --to 2026-06-05 --out minutes.json

# 5. 本地离线运行数据质量与描述统计分析（不消耗任何线上额度）
dytcn analyze --input minutes.json

# 6. 一键同步并校验可重现的不可变 Parquet 数据集
dytcn sync --symbol 600000.SH --from 2026-06-01 --to 2026-06-30 --dir ./research

# 7. 将配套 Agent Skills 一键分发到本地 AI 编程工具
dytcn skills install

# 8. 综合检查本地环境与更新状态
dytcn doctor
```

---

## 🤖 AI Agent 深度赋能

`dytcn` 不仅是人类量化研究员的利器，更专为大语言模型（LLM）与自动化 AI Agent 深度适配。CLI 二进制内部直接打包了经过精心打磨的 Agent 知识库与操作技能：

```
skills/
└── dyt-ashare/              # A 股信息查询、数据质量与研究分析统一路由
```

### 1. 内置 Skills 详解

* **`dyt-ashare`**：
  - 对所有 A 股、沪深京股票、行情/历史信息、分钟线、覆盖度、数据质量、技术分析、研究和回测请求触发。
  - 在同一个入口内路由到 `usage`、`coverage`、`minutes`、`sync`、`analyze`、`doctor` 和认证流程，避免用户安装多个相互竞争的 Skill。
  - 强制使用 `.SH` / `.SZ` / `.BJ` 交易所后缀、保留不可变数据集哈希，并提醒原始价格变化不等于考虑公司行为和交易成本后的收益。

### 2. 一键安装与适配支持

通过以下命令将内置 Skills 导出到主流 AI 辅助工具的技能库：

```bash
# 默认写入 canonical 目录 (~/.agents/skills)，并为已检测到的 Agent 建立相对链接
dytcn skills install

# 或指定安装到当前项目的自定义技能目录
dytcn skills install --dir ./.agents/skills
```

支持即插即用的 AI 研发环境：
- **Claude Code**、**Codex**、**Cursor**、**Windsurf**、**OpenCode**、**Gemini/Antigravity**、**Trae**、**Qwen**、**OpenClaw** 和 **Roo**（检测到对应用户目录时自动建立链接）。
- canonical 文件只有一份，CLI 更新或回滚后会同步它；各 Agent 目录共享这份文件，用户编辑或外部目录会保留并出现在 `pending`。
- 未检测到的 Agent 可使用 `dytcn skills install --dir <agent-skill-dir>` 显式接入；该命令写入一个目录并保留原有文件。

---

## 🛠️ 核心命令详解

`dytcn` 严格遵循 Unix 哲学：**标准输出（stdout）全部为可解析的 JSON 字符串**，任何交互日志、排障诊断和错误跟踪均写入标准错误输出（stderr），操作失败时返回非零退出码。

```
dytcn [GLOBAL_OPTIONS] <COMMAND>
```

**全局参数 (Global Options)**：
- `--home <PATH>` 或 `DYT_ASHARE_HOME`：指定本地数据与状态主目录（默认 Windows 为 `%LOCALAPPDATA%\DYT\AShare`，Unix 为 `~/.local/share/dyt-ashare`）。
- `--api-url <URL>` 或 `DYT_ASHARE_API_URL`：指定服务端 Origin 地址（由服务方提供，如 `https://<API_HOST>`）。
- `--out <PATH>`：将命令输出结果以原子写入方式落地为本地文件，stdout 仅输出文件路径、字节数与 SHA256 校验和。
- `--no-update` 或 `DYT_ASHARE_NO_UPDATE=1`：在 CI 或受控环境中临时禁用自动更新检测。

---

### 1. 认证管理 (`dytcn auth`)

第一版数据服务由运营方统一签发账户密钥（API Key）。为杜绝敏感凭据泄露，密钥以交互隐藏方式输入，绝不允许放入 CLI 参数或出现在终端历史记录中。

```bash
# 交互式安全登录（输入内容不可见）
dytcn auth login

# 管道输入（适用于 CI / 无人值守自动化流程）
echo "ask_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" | dytcn auth login --stdin

# 查看当前认证状态与环境变量生效情况
dytcn auth status

# 清除本地保存的凭据
dytcn auth logout
```

### 2. 权益与用量监控 (`dytcn usage`)

查询服务端登记的账户等级、服务到期时间、总配额以及当前分钟线用量统计：

```bash
dytcn usage
```

<details>
<summary>📋 点击展开示例输出 (JSON)</summary>

```json
{
  "account_id": "acc-research-01",
  "tier": "professional",
  "expires_at": "2027-12-31T23:59:59Z",
  "minutes_quota_monthly": 10000000,
  "minutes_consumed_monthly": 142560,
  "remaining_quota": 9857440
}
```
</details>

### 3. 数据覆盖度探查 (`dytcn coverage`)

在批量下载或回测前，先探查特定标的在指定区间内可用的历史不可变数据集版本。

```bash
dytcn coverage --symbol 600000.SH --from 2026-01-01 --to 2026-06-30
```

> [!NOTE]
> 数据集以不可变月份为粒度归档。返回列表中某一月份缺失表示该区间暂无归档数据，不代表该区间全部为法定休市日。

### 4. 分钟级 K 线查询 (`dytcn minutes`)

查询指定股票在时间窗口内的高频分钟数据。单次请求最多支持 31 个自然日，日期参数基于上海时区（Asia/Shanghai），日期终点 `--to` 包含当日收盘。

```bash
# 直接在终端输出 JSON 结果
dytcn minutes --symbol 600000.SH --from 2026-06-01 --to 2026-06-05

# 使用 --out 原子保存到本地文件（推荐用于后续分析与存档）
dytcn minutes --symbol 600000.SH --from 2026-06-01 --to 2026-06-05 --out minutes.json
```

输出反馈示例（`--out` 模式）：
```json
{
  "output_path": "/Users/research/minutes.json",
  "bytes": 285420,
  "sha256": "8f4a13c9e6d0a7f1a3298a0cbe445df3320f269a8b1ec45674c30c4e4ef65c71"
}
```

### 5. 本地离线零成本分析 (`dytcn analyze`)

直接在本地对 `minutes --out` 生成的 JSON 数据进行深度统计与质量体检。**该过程 100% 在本地执行，不产生任何网络流量与服务端配额扣减**：

```bash
dytcn analyze --input minutes.json
```

<details open>
<summary>📋 本地离线分析输出示例 (JSON)</summary>

```json
{
  "cli_version": "0.1.0",
  "input_sha256": "8f4a13c9e6d0a7f1a3298a0cbe445df3320f269a8b1ec45674c30c4e4ef65c71",
  "symbol": "600000.SH",
  "rows": 960,
  "first_bar": "2026-06-01 09:31:00",
  "last_bar": "2026-06-05 15:00:00",
  "raw_close_change": 0.024512,
  "raw_close_max_drawdown": 0.011804,
  "volume_shares": 18452300.0,
  "turnover_cny": 196320450.0,
  "observed_day_rows": {
    "2026-06-01": 240,
    "2026-06-02": 240,
    "2026-06-03": 240,
    "2026-06-04": 240
  },
  "observed_days_with_partial_or_invalid_counts": [],
  "duplicate_timestamps": 0,
  "invalid_ohlcv": 0,
  "off_session_timestamps": 0,
  "interpretation": "unadjusted descriptive statistics; not strategy returns; no certified calendar or fee model"
}
```
</details>

**核心质检指标说明**：
- `observed_day_rows`：每日实际观测到的有效分钟 K 线数量（A 股标准全交易日基准为 240 根）。
- `observed_days_with_partial_or_invalid_counts`：缺失、早收或异常的分秒日明细列表。
- `invalid_ohlcv`：极端行情非法数值计数（如 High < Low、Close <= 0、价格非有限数值等）。
- `off_session_timestamps`：非标准 A 股交易时段（不在 09:31-11:30 或 13:01-15:00 范围）的时间戳。
- `duplicate_timestamps`：重复时间戳统计。

### 6. 不可变 Parquet 数据集同步 (`dytcn sync`)

用于研究库的大规模归档。`sync` 会校验服务端目录中的每个不可变数据切片（UUID v4），按大小与 SHA256 校验和原子落地为 `.parquet` 与 `.manifest.json`，并汇总生成 `catalog.json`。

```bash
dytcn sync --symbol 600000.SH --from 2026-01-01 --to 2026-06-30 --dir ./data/parquet
```

- **增量跳过**：重复执行时，本地哈希比对一致的文件将被直接跳过（`verified_skipped`），节省带宽。
- **版本保留**：当数据产生修订时，旧版本 Parquet 仍保留在磁盘上以保证旧回测完全可复现，当前主索引统一由 `catalog.json` 指定。
- **越权保护**：若所请求区间跨越了历史权限边界，未授权部分将标记为 `restricted_downloads` 并在客户端受限保护。

### 7. 系统综合诊断 (`dytcn doctor`)

在排查问题或进行环境交付前，使用 `doctor` 一键获取当前 CLI 版本、二进制路径、状态目录、服务来源、更新状态与已安装 Skills 状态：

```bash
dytcn doctor
```

### 8. 自愈更新与版本管理 (`dytcn update`)

`dytcn` 内置了完整的自愈与更新机制，所有更新包均由 Ed25519 密钥签名。

```bash
# 检查更新状态与当前激活版本
dytcn update status

# 立即触发一次验签更新检测
dytcn update

# 回滚到上一个稳定版本并暂停自动更新
dytcn update rollback

# 重新启用或禁用周期性后台自动更新
dytcn update enable
dytcn update disable
```

---

## 📊 A 股交易特性与数据契约规范

量化研究容不得模糊假设。`dytcn` 在数据模型上严格尊重中国 A 股市场的特有规则：

```
                09:15-09:25       09:30           11:30   13:00           15:00
市场状态 --------| 早盘集合竞价 |------|---- 连续竞价 ----|-------|---- 连续竞价 ----|---- 收盘
K 线输出 ---------------> [09:31 K线包含竞价] --------------> [上午 120 根]  [下午 120 根] = 共 240 根/日
```

### 1. 证券代码与交易所后缀
必须显式指定交易所后缀，目前支持的格式包括：
- 上海证券交易所：`600000.SH`、`688001.SH`
- 深圳证券交易所：`000001.SZ`、`300750.SZ`
- 北京证券交易所：`830946.BJ`

### 2. 交易时区与标准时间戳
- **时区标准**：全链路采用北京时间 **Asia/Shanghai (UTC+8)**。
- **闭区间语义**：纯日期格式的 `--to 2026-06-05` 自动展开为 `2026-06-05 23:59:59`，确保完整包含该交易日全天数据。

### 3. 分钟 K 线结构与集合竞价
- **240 根标准交易日**：全天正常交易日由上午 120 根（09:31 至 11:30）和下午 120 根（13:01 至 15:00）构成。
- **09:31 柱线**：第一根 1 分钟线的时间戳通常标记为 `09:31:00`，成交量与成交额中已包含早盘 09:15-09:25 的集合竞价成交结果。

### 4. 价格、成交量与除权除息契约
- **不复权价格 (Unadjusted)**：所有 `px_open`, `px_high`, `px_low`, `px_close` 均为真实历史不复权行情。
- **成交量与成交额**：`volume_shares` 单位为股（Shares），`turnover_cny` 单位为人民币元（CNY）。
- **策略严谨性**：原始价格变化与最大回撤（`raw_close_change` / `raw_close_max_drawdown`）仅作为数据描述性指标，**不可直接作为策略真实收益**。当标的发生分红、送转股或配股等除权除息行为时，应在量化引擎中结合复权因子单独处理。

---

## 🔐 安全机制与自愈更新体系

`dytcn` 将研究环境的安全性与确定性置于首位：

```mermaid
flowchart TD
    A["触发更新检测 (CLI 启动 / 定时任务)"] --> B["获取 Release Manifest (HTTPS)"]
    B --> C["校验 Ed25519 密码学签名"]
    C -->|验签失败| D["拒绝更新，保留当前版本运行"]
    C -->|验签成功| E["下载目标平台资产包"]
    E --> F["验证文件字节大小与 SHA256"]
    F --> G["解压到独立版本目录"]
    G --> H["拉起新程序执行 self-test 自检"]
    H -->|自检通过| I["原子切换 active.json 软链/指针"]
    H -->|自检失败| J["终止切换，保留原版本并记录错误"]
    I --> K["下次启动生效新版本，旧版本保留可回滚"]
```

1. **密码学完整性保障**：
   - 更新包通过内置的 Ed25519 公钥（`release-public-key.hex`）进行身份验签，防止任何中间人攻击或恶意镜像劫持。
   - 每个资产严格匹配 SHA256 与字节长度双重审计。
2. **进程隔离与原子切换**：
   - 正在运行的研究任务或回测不会被中断（旧进程继续占用原有镜像）。
   - 通过 `active.json` 进行原子激活，升级失败自动回退，不破坏现有运行目录。
3. **敏感凭据本地作用域隔离**：
   - 凭据仅存储在用户专属的本地状态目录中，且严格绑定服务 Origin。
   - 当向对象存储下载 Parquet 数据或发生 302 重定向时，`dytcn` 会自动剥离 Authorization Bearer 凭据头，防止密钥泄漏至第三方存储节点。
4. **零后台遥测**：
   - 不收集任何研究标的、查询习惯或量化代码信息，专注纯粹的数据通道交付。

---

## ⚙️ 高级配置与环境变量

`dytcn` 支持通过命令行参数或环境变量进行全局配置，环境变量非常适用于 CI/CD 自动化流水线：

| 环境变量 | CLI 等价参数 | 默认值 | 作用与说明 |
|:---|:---|:---|:---|
| `DYT_ASHARE_HOME` | `--home <PATH>` | 见下文 | 本地状态根目录，存放凭据、缓存数据集、Skills 及更新版本 |
| `DYT_ASHARE_API_KEY` | *(交互输入)* | *(无)* | 显式提供 API Key（`ask_...` 格式），优先于本地保存的凭据 |
| `DYT_CN_API_KEY` | *(交互输入)* | *(无)* | `DYT_ASHARE_API_KEY` 的向前兼容别名 |
| `DYT_ASHARE_API_URL` | `--api-url <URL>` | *(由服务方配置)* | 数据服务端 API 基础地址（支持私有化或代理，仅允许 HTTPS 或 loopback 本地地址） |
| `DYT_ASHARE_NO_UPDATE` | `--no-update` | `false` | 设为 `1` 或 `true` 时，禁止一切后台自动更新与定时检测 |

> 默认状态根目录：
> - Windows: `%LOCALAPPDATA%\DYT\AShare`
> - Linux / macOS: `~/.local/share/dyt-ashare`

---

## 🛠️ 开发者指南

我们欢迎社区贡献！客户端代码遵循 Apollo GraphQL 与 Rust 官方最佳实践。

### 本地编译与验证

以下命令仅用于仓库贡献者进行本地验证，不是用户安装方式。

```bash
# 格式化代码校验
cargo fmt --check

# Clippy 严格静态检查
cargo clippy --all-targets --locked -- -D warnings

# 运行全套单元测试与行为断言
cargo test --locked

# 或直接运行仓库提供的自动化本地测试门禁
bash scripts/test.sh
```

关于完整的 Ed25519 签名发布打包、多架构交叉编译、Windows MSVC 静态 CRT 链接与端到端更新测试细节，请参阅 [DEVELOPING.md](DEVELOPING.md)。

---

## ⚖️ 免责声明与开源协议

### 免责声明 (Disclaimer)
1. 本项目及其所提供的数据工具仅供**学术研究、量化数据处理与技术开发**交流使用，不构成任何形式的投资建议、买卖要约或金融决策依据。
2. 股市有风险，投资需谨慎。用户应对基于本项目数据运行的策略模型、交易决策及其盈亏结果承担全部责任。
3. 数据源及本软件不保证历史行情数据的绝对无遗漏或完全不间断，在实盘环境中请配合独立的风控系统使用。

### 开源协议 (License)
本项目客户端采用 [MIT License](LICENSE) 开源。
