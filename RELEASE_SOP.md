# dytcn 发布 SOP

这份 SOP 适用于每一次 CLI、内置 Skill、更新器或打包脚本发布。版本号是更新器的
不可变身份；只改代码而不递增版本，客户端会按设计跳过该包。

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

## 3. 本地门禁和密钥检查

```sh
scripts/test.sh
python3 scripts/e2e_update.py
gitleaks git --redact --no-banner --log-opts='origin/main..HEAD'
```

`e2e_update.py` 必须覆盖安装、更新、Skill 保留、签名清单篡改拒绝、回滚和后台更新。
生产私钥只能从受控本地路径读取，不能进入仓库、命令行参数记录或构建产物。

确认私钥派生出的 Ed25519 公钥与仓库中的 `release-public-key.hex` 一致，并使用
OpenSSL 3：

```sh
OPENSSL=/opt/homebrew/opt/openssl@3/bin/openssl
$OPENSSL pkey -in "$RELEASE_KEY" -pubout -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n'
cat release-public-key.hex
```

两行必须完全一致。

## 4. 构建并打包

每个支持目标都执行 `cargo build --release --locked --target <target>`，然后让打包脚本
从 Cargo 版本自动生成签名清单：

```sh
DYT_OPENSSL="$OPENSSL" python3 scripts/package.py \
  --binary target/<target>/release/dytcn \
  --target <target> \
  --base-url https://data.doyoutrade.cloud/releases/dyt-ashare \
  --key "$RELEASE_KEY" \
  --out dist
```

检查 `dist/<version>/<target>/release.json` 和 `dist/stable-<target>.json`：payload
版本、目标平台、文件大小和 SHA256 必须与产物一致；清单中的 URL 必须仍指向
`data.doyoutrade.cloud`。

## 5. 发布顺序

先上传不可变的 `<version>/<target>/` 目录（包括二进制、Skill 和 `release.json`），
再上传对应的 `stable-<target>.json`。稳定清单必须最后发布，避免客户端先看到清单而
下载不到资产。禁止覆盖已经发布的版本目录。

签名清单有效期为 7 天；到期前即使没有二进制变化，也要重新签名并更新稳定清单。

## 6. 发布后验收

至少使用一台仍在旧版本的隔离安装目录：

```sh
DYT_ASHARE_HOME="$HOME/.local/share/dyt-ashare-acceptance" dytcn update
DYT_ASHARE_HOME="$HOME/.local/share/dyt-ashare-acceptance" dytcn --version
DYT_ASHARE_HOME="$HOME/.local/share/dyt-ashare-acceptance" dytcn doctor
```

核对 `updated: true`、新版本号、激活二进制路径和 Skill 版本。随后用无凭据的隔离
home 调用一次 `coverage` 或 `minutes`，确认游客查询走线上新 CLI；再用已认证路径确认
需要账号的 `sync` 仍然拒绝无凭据调用。最后记录稳定清单、二进制 SHA256、源代码 SHA、
目标平台和验收时间。

## 7. 合并和回滚

提交前再次运行 `scripts/test.sh` 和 secret scan，然后按仓库 `AGENTS.md` 使用
`scripts/land.sh` 创建并合并 PR。线上异常时先保留已发布版本目录，使用客户端
`dytcn update rollback` 回到上一版本；不要删除仍可能被运行中进程使用的目录。
