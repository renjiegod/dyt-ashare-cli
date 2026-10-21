# dytcn 发布 SOP

这份 SOP 适用于每一次 CLI、内置 Skill、更新器或打包脚本发布。版本号是更新器的
不可变身份；只改代码而不递增版本，客户端会按设计跳过该包。

发布通道是**未签名的哈希清单 + 仓库锚点**：客户端只有在网上取到的通道清单字节与仓库
里、随 `v<version>` 标签发布的锚点一致时才接受该发布。通道没有有效期，不需要私钥，
也不需要定期重新签名。

## 1. 建立发布分支

```sh
git fetch origin main
git worktree add ../dyt-ashare-cli-release -b chore/release-X.Y.Z origin/main
cd ../dyt-ashare-cli-release
```

只在这个分支修改和发布。不要直接提交 `main`。

## 2. 先改版本，再改发布内容

在 [`Cargo.toml`](Cargo.toml) 修改 `[package].version`，并运行一次：

```sh
cargo check
```

这会同步 [`Cargo.lock`](Cargo.lock) 中的 `dyt-ashare-cli` 版本。`scripts/package.py`
现在默认从 `Cargo.toml` 读取版本；如果显式传入 `--version`，它必须与 `Cargo.toml`
完全相同。

`scripts/check_release.py` 已接入 `scripts/test.sh`。它会检查：

* `Cargo.toml` 与 `Cargo.lock` 版本一致；
* 分支或工作区改动涉及 `src/`、`skills/`、打包脚本或 Cargo 元数据时，版本相对
  `origin/main` 必须发生变化；
* 打包命令不能把产物标成另一个版本。

因此发布代码漏改版本会在本地门禁阶段失败。

## 3. 本地门禁

```sh
scripts/test.sh
python3 scripts/e2e_update.py
gitleaks git --redact --no-banner --log-opts='origin/main..HEAD'
```

`e2e_update.py` 必须覆盖安装、更新、Skill 保留、清单篡改拒绝、锚点缺失拒绝、资产损坏
拒绝、回滚和后台更新。仓库里不再有发布私钥，因此不再需要 OpenSSL。

## 4. 构建并打包

每个支持目标都执行 `cargo build --release --locked --target <target>`，然后让打包脚本
生成通道清单与仓库锚点：

```sh
python3 scripts/package.py \
  --binary target/<target>/release/dytcn \
  --target <target> \
  --base-url https://data.doyoutrade.cloud/releases/dyt-ashare \
  --out dist
```

打包脚本输出：

* `dist/<version>/<target>/`：不可变产物（`dytcn`、`skills/dyt-ashare/SKILL.md`）与
  `release.json`（通道清单本身）；
* `dist/channel-<target>.json`：客户端读取的通道清单；
* `dist/releases/<version>/<target>.sha256`：仓库锚点，内容是清单字节的 SHA-256；
* `dist/install.sh`、`dist/install.ps1`：引导安装器。

清单是确定性的：同样的产物永远得到同样的字节和同样的锚点。发布前核对
`dist/<version>/<target>/release.json` 与 `dist/channel-<target>.json` 字节一致，锚点文件
里的摘要等于清单的 SHA-256，清单里的 URL 必须仍指向 `data.doyoutrade.cloud`。

## 5. 发布锚点并打标签

锚点是通道信任的根，必须在发布清单之前进入仓库：

```sh
mkdir -p releases/<version>
cp dist/releases/<version>/<target>.sha256 releases/<version>/
git add releases/<version>
git commit -m "chore: publish the <version> release channel anchors"
scripts/land.sh
git tag v<version>
git push origin v<version>
```

客户端按 `<anchor base>/v<version>/releases/<version>/<target>.sha256` 取锚点。标签是
锚点的不可变引用；不要移动或覆盖已发布的标签。

## 6. 发布顺序

先上传不可变的 `<version>/<target>/` 目录（包括二进制、Skill 和 `release.json`），
再上传 `channel-<target>.json`。通道清单必须最后发布，避免客户端先看到清单而
下载不到资产。禁止覆盖已经发布的版本目录。

同时删除已退役的 `stable-<target>.json`：0.1.5 及更早的客户端只能解析旧的签名信封，
保留旧通道只会让它们反复尝试更新。**只删除已经在锚点通道上重新发布过的目标**；尚未
重新发布的目标继续由旧签名通道服务，安装器两种格式都接受，等它以锚点通道重新发布
后再删除它的 `stable-<target>.json`。

## 7. 发布后验收

至少使用一台仍在旧版本的隔离安装目录：

```sh
DYT_ASHARE_HOME="$HOME/.local/share/dyt-ashare-acceptance" dytcn update
DYT_ASHARE_HOME="$HOME/.local/share/dyt-ashare-acceptance" dytcn --version
DYT_ASHARE_HOME="$HOME/.local/share/dyt-ashare-acceptance" dytcn doctor
```

核对 `updated: true`、新版本号、激活二进制路径、Skill 版本，以及 `doctor` 里
`updates.channel.verification` 为 `sha256+anchor`。随后用无凭据的隔离 home 调用一次
`coverage` 或 `minutes`，确认游客查询走线上新 CLI；再用已认证路径确认需要账号的
`sync` 仍然拒绝无凭据调用。最后记录通道清单、锚点、二进制 SHA-256、源代码 SHA、
目标平台和验收时间。

## 8. 合并和回滚

提交前再次运行 `scripts/test.sh` 和 secret scan，然后按仓库 `AGENTS.md` 使用
`scripts/land.sh` 创建并合并 PR。线上异常时先保留已发布版本目录，使用客户端
`dytcn update rollback` 回到上一版本；不要删除仍可能被运行中进程使用的目录。
