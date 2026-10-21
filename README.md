<div align="center">

# DYT AShare · A 股分钟数据 API

**为量化研究、数据分析与 AI Agent 提供 A 股历史分钟数据服务**

通过配套命令行工具 `dytcn` 查询数据、下载研究数据集，也可以让 AI Agent 帮你完成查询与分析。

[![Platform](https://img.shields.io/badge/platform-Windows%20x64%20%7C%20macOS%20ARM64%20%7C%20Linux%20x86_64-lightgrey.svg?style=flat-square)](#-安装与试用)
[![Contact](https://img.shields.io/badge/咨询与购买-297910962%40qq.com-blue.svg?style=flat-square)](mailto:297910962@qq.com)

<p align="center">
  <a href="#-数据服务">数据服务</a> •
  <a href="#-开通与购买">开通与购买</a> •
  <a href="#-安装与试用">安装与试用</a> •
  <a href="#-使用-api-key">使用 API Key</a> •
  <a href="#-让-ai-agent-帮你查数据">AI Agent</a> •
  <a href="#-联系与合作">联系与合作</a>
</p>

</div>

> **想先体验？** 安装后无需 API Key 即可在游客额度内查询。需要更长历史范围、批量查询或数据集下载，请联系 [297910962@qq.com](mailto:297910962@qq.com) 咨询套餐与开通 API Key。

---

## 📊 数据服务

| 你需要什么 | DYT AShare 提供什么 |
|:---|:---|
| **历史分钟行情** | 查询 A 股分钟 K 线，包括开盘、最高、最低、收盘、成交量与成交额。 |
| **下载前确认覆盖** | 按证券代码与日期查询数据覆盖情况，先确认所需区间是否可用。 |
| **批量研究数据** | 登录后按账户权益下载 Parquet 数据集，适合建立本地研究库。 |
| **数据质量检查** | 对已下载数据检查重复记录、异常价格、交易时段与每日记录数量。 |
| **AI Agent 接入** | 配套 `dyt-ashare` Skill，让 Claude Code、Codex、Cursor 等工具调用数据服务。 |

适用于历史行情研究、量化数据准备、研究工具集成与 AI Agent 数据接入。具体证券和日期范围请先查询覆盖情况，或通过邮箱确认。

## 💳 开通与购买

**咨询邮箱：[297910962@qq.com](mailto:297910962@qq.com)**

1. **说明需求**：所需证券、历史区间、预计查询量，以及是否需要批量下载或 AI Agent 接入。
2. **确认套餐**：通过邮件确认数据覆盖、历史权限、配额、有效期与价格。
3. **开通使用**：取得 API Key 后，在终端登录，即可按已开通权益使用服务。

| 使用方式 | 游客试用 | 购买并登录后 |
|:---|:---|:---|
| 分钟线与覆盖查询 | 可用，受游客额度限制 | 按账户权益使用 |
| 历史区间 | 限最近允许窗口 | 以购买套餐为准 |
| 请求频率与数据量 | 游客限流及每日配额 | 以账户配额为准 |
| Parquet 数据集下载 | 不支持 | 按账户权益使用 |
| 查询账户权益与余量 | 不支持 | 使用 `dytcn usage` 查看 |

套餐价格与具体权益请通过邮件确认。

---

## 📦 安装与试用

支持 **Windows x64**、**macOS Apple Silicon** 和 **Linux x86_64**。下载安装即可使用，无需自行编译或配置 Python 环境。

### macOS / Linux

```bash
curl -fsSL https://data.doyoutrade.cloud/releases/dyt-ashare/install.sh | sh

# 重新加载当前 shell 后确认安装
exec $SHELL
dytcn --version
```

### Windows x64

打开 **PowerShell**，运行以下命令：

```powershell
# 下载并运行安装程序
$installer = Join-Path $env:TEMP "dyt-ashare-install.ps1"
Invoke-WebRequest -UseBasicParsing -Uri "https://data.doyoutrade.cloud/releases/dyt-ashare/install.ps1" -OutFile $installer
powershell -NoProfile -ExecutionPolicy Bypass -File $installer
```

安装完成后，关闭并重新打开终端，检查是否安装成功：

```powershell
dytcn --version
dytcn doctor
```

如果提示找不到 `dytcn`，请确认已重新打开终端。Windows ARM64 暂未开放安装。

### 开始试用

安装后可以先用游客模式查询数据覆盖与分钟线。以下日期仅作示例，请替换为游客当前允许范围内的日期；游客历史窗口和额度由服务端控制。

```bash
# 确认所需区间的数据覆盖
dytcn coverage --symbol 600000.SH --from 2026-10-19 --to 2026-10-20

# 查询分钟线并保存到本地
dytcn minutes --symbol 600000.SH --from 2026-10-19 --to 2026-10-20 --out minutes.json

# 检查已下载数据的质量与描述统计，不额外消耗 API 查询额度
dytcn analyze --input minutes.json
```

游客查询达到频率、每日额度或历史范围限制时，可按命令提示稍后重试，或联系邮箱开通适合需求的账户权益。

### 更新与信任模型

`dytcn` 的自动更新读取一份按目标平台分发的**通道清单**（`channel-<target>.json`），清单
里的每个文件都带大小和 SHA-256。清单本身不带签名，取而代之的是**仓库锚点**：客户端会把
清单字节的 SHA-256 与本仓库随版本标签 `v<version>` 发布的锚点比对，不一致或取不到锚点
就拒绝更新——发布源被攻破也无法单方面推送未批准的客户端。通道没有有效期，因此发布新
版本不需要重新签名，也不需要维护私钥。

> **0.1.6 起的升级说明**：0.1.6 之前使用带签名、7 天有效期的旧通道。已在 0.1.6 重新发布的
> 平台（macOS Apple Silicon、Linux x86_64）不再提供旧通道；已经安装 0.1.5 或更早版本的
> 客户端无法再自动升级，请**重新执行一次上面的安装命令**。其余平台在以新通道重新发布之前
> 仍由旧通道提供，安装器同时兼容两种格式。

## 🔑 使用 API Key

取得 API Key 后，在自己的终端运行以下命令并按提示输入密钥：

```bash
dytcn auth login

# 查看账户权益、有效期与配额余量
dytcn usage
```

登录后继续使用 `coverage` 和 `minutes` 查询；数据范围与用量以账户权益为准。需要批量研究数据时，可下载 Parquet 数据集：

```bash
# 请按已开通的数据范围替换证券与日期
dytcn sync --symbol 600000.SH --from 2026-06-01 --to 2026-06-30 --dir ./research
```

请妥善保管 API Key，不要发送到公开 Issue、聊天记录或代码仓库。

## 🤖 让 AI Agent 帮你查数据

安装器会自动安装配套的 `dyt-ashare` Skill。也可以手动安装：

```bash
dytcn skills install
```

支持 Claude Code、Codex、Cursor、Windsurf、OpenCode、WorkBuddy 等工具。安装后，可以向 Agent 描述需求，例如：

> 查询浦发银行（600000.SH）在指定日期范围内的分钟数据覆盖，确认有数据后下载，并检查是否存在重复记录或异常价格。

Agent 使用同一套数据服务，游客限制与账户配额同样适用。需要登录时，请在自己的终端输入 API Key。

---

## ❓ 常见问题

**可以不购买先试用吗？**

可以。游客模式支持受限的分钟线与覆盖查询。需要更长历史范围、更高用量或 Parquet 下载时，请联系邮箱确认套餐。

**如何确认有我需要的数据？**

先运行 `dytcn coverage`，或将证券代码与日期范围发送到咨询邮箱。具体覆盖情况以查询结果及开通前确认为准。

**查询时要注意什么？**

证券代码需包含交易所后缀，例如 `600000.SH`、`000001.SZ`、`830946.BJ`。单次分钟线查询最多 31 个自然日；日期按北京时间解释，结束日期包含当日。

**价格是否复权？**

当前分钟线为不复权价格；成交量单位为股，成交额单位为人民币元。正常完整交易日以 240 根分钟线为基准。数据中的原始价格变化和回撤属于描述统计，不能直接视为策略收益。

**使用遇到问题怎么办？**

可以运行 `dytcn doctor` 检查本地状态。账户开通、套餐与额度问题请发邮件；客户端使用问题也可以提交 Issue，附上脱敏后的错误信息。

---

## 📬 联系与合作

| 联系方式 | 说明 |
|:---|:---|
| **API 咨询与购买** | [297910962@qq.com](mailto:297910962@qq.com)，咨询套餐、数据覆盖、开通 API Key 与用量需求。 |
| **商务与接入合作** | 同一邮箱，欢迎交流研究工具、数据应用与 AI Agent 接入合作。 |
| **小红书** | **renjiegod**（小红书号：`18425751543`），分享项目更新、数据工具使用与 AI Agent 实践。 |
| **使用反馈** | [提交 GitHub Issue](https://github.com/renjiegod/dyt-ashare-cli/issues)，请勿附带 API Key 或其他敏感信息。 |

<p align="center">
  <img src="assets/xiaohongshu-renjiegod.jpg" alt="小红书 renjiegod 主页二维码，小红书号 18425751543" width="280" />
  <br />
  <sub>扫码关注小红书 · renjiegod</sub>
</p>

如果服务对你有帮助，欢迎点亮 **Star**，或分享给有数据需求的朋友。

---

## ⚖️ 使用说明

本项目及数据工具用于研究、数据处理与技术应用，不构成投资建议。数据可能存在缺失或修订，使用前请确认覆盖与质量；涉及策略研究时，应另行考虑复权、交易成本等因素。

配套 CLI 客户端采用 [MIT License](LICENSE) 开源；数据 API 为独立服务，使用范围、配额与有效期以开通时约定为准。
