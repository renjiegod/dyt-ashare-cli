# DYT AShare

面向 A 股历史分钟数据研究的开源 Rust CLI 和配套 Agent Skills。服务地址默认
`https://data.doyoutrade.cloud`。客户端 MIT 开源，数据服务的账户、额度与套餐由服务端管理。
不同市场独立交付；加密货币能力将属于独立的 `dyt-crypto` 产品。

当前为开发预览：尚未部署安装源和新版账户策略；仓库代码不代表公网服务已经升级。

Windows 用户使用预编译的 `dyt.exe`，不需要安装 Rust、Python、Node.js 或 WSL。
首次安装采用 PowerShell；便携版本也可直接运行。源码开发需要 Rust 1.88+。

```powershell
# 安装源发布后使用；当前尚未上线。
powershell -ExecutionPolicy Bypass -c "irm https://data.doyoutrade.cloud/releases/dyt-ashare/install.ps1 | iex"
```

安装脚本检测 x64/ARM64，配置用户 PATH、安装 Skills，并注册每 6 小时执行的用户级更新任务。
启动 CLI 时也会检查更新周期。首次安装依赖 HTTPS 信任，后续更新由程序内置的 Ed25519 公钥验签。
更新会校验大小和 SHA256、启动新程序自检，然后原子切换当前版本。运行中的分析使用原版本，
下次命令使用新版本。失败保留原版本；已安装的旧版本可回滚。

```sh
# 第一版由服务运营方签发账户 Key；没有客户自助浏览器登录或在线支付。
dyt auth login                      # 本地隐藏输入；不把 Key 放入参数或聊天
dyt usage                           # 服务端账户权益、额度、用量
dyt coverage --symbol 600000.SH --from 2026-06-01 --to 2026-06-30
dyt minutes --symbol 600000.SH --from 2026-06-01 --to 2026-06-05 --out minutes.json
dyt analyze --input minutes.json     # 本地免费分析
dyt sync --symbol 600000.SH --from 2026-06-01 --to 2026-06-30 --dir ./research
dyt skills install                   # 默认 ~/.agents/skills
dyt skills install --dir ./my-skills
dyt doctor
dyt update                          # 手动立即执行验签更新
dyt update rollback                 # 回滚并暂停自动更新
dyt update enable
dyt update disable
```

命令结果为 stdout JSON；诊断和错误写入 stderr，失败返回非零退出码。
`--out` 原子写文件，stdout 仅返回路径、大小和 SHA256。`--api-url` 或 `DYT_ASHARE_API_URL`
选择服务。存储的 Key 与服务 origin 绑定；显式环境 Key `DYT_ASHARE_API_KEY` 可供 CI 使用。
`--home` / `DYT_ASHARE_HOME` 指定独立状态目录。CI 可设置 `DYT_ASHARE_NO_UPDATE=1` 固定版本，
便携程序未安装时不会自动发起更新。HTTPS 以外仅允许 loopback 测试服务。

同步按目录的不可变 ID 校验并保存 Parquet 及来源 manifest。重复同步校验后跳过已有文件，修订版
保留旧文件；分析只能读取当前 `catalog.json` 指向的文件，不能 glob 全部修订版。
完整月文件跨越历史权限边界时，服务返回 `restricted_downloads`，不签发越权下载。
分钟查询的纯日期 `--to` 包含当日收盘；时间戳使用上海时区。

数据为不复权价格、股数、人民币成交额；09:31 可能包含开盘集合竞价。日历未认证，
缺少文件不等于休市。`analyze` 给出原始价格变化、收盘回撤和数据质量计数，不能作为策略收益。
本版本分析输入为 `minutes --out` 生成的 JSON；Parquet 研究可使用自己的分析工具。

CLI 内嵌同版本的 `dyt-ashare-research` / `dyt-ashare-quality` Skills。更新会刷新本工具已登记的 Skills，
保留本地修改或同名外部目录，并在 `doctor` 中报告 pending。重新启动 Agent 后再核对其发现结果。

构建、验收与发布流程见 [DEVELOPING.md](DEVELOPING.md)。
