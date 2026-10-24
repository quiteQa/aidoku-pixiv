# Aidoku Pixiv 源

独立的 Rust / WebAssembly 源，支持 **网页登录**和 **Token 登录**。按 [Aidoku Community Sources](https://github.com/Aidoku-Community/sources) 的目录及订阅格式组织，需要 Aidoku 0.9 或更新版本。

## 添加源订阅

在 Aidoku 的设置 → 源列表中添加：

```
https://raw.githubusercontent.com/quiteQa/aidoku-pixiv/gh-pages/index.min.json
```

首次推送后，GitHub Actions 会在构建和测试通过时自动发布订阅文件到 `gh-pages`。订阅可用后，在浏览的添加源页面安装 Pixiv。也可以直接下载 [`Pixiv.aix`](https://github.com/quiteQa/aidoku-pixiv/raw/refs/heads/main/dist/Pixiv.aix)，按下方方式手动导入。

## 安装与登录

1. 将 `Pixiv.aix` 发送到安装了 Aidoku 的设备，从“文件”中通过分享菜单用 Aidoku 打开，或在 Aidoku 中导入该文件。
2. 打开 Pixiv 源的设置，通过内嵌的“登录方式”选择器选择网页登录或 Token 登录；“账号登录”区域仅显示当前方式对应的控件。布局参考 [Wenku8 源的原生登录控件](https://github.com/quiteQa/aidoku-wenku8/blob/main/res/settings.json)。
3. **网页登录**：点击“登录 Pixiv”，在 Pixiv 官方网页完成登录及验证。源会验证 `PHPSESSID` 会话，成功后关闭登录窗口。源不接收账号密码。登录后按钮变为“退出 Pixiv”。
4. **Token 登录**：选择 `Refresh Token` 或 `Access Token`，粘贴自己的 Token，点击“验证 Token”，在页面下方“登录状态”查看结果。支持粘贴带 `Bearer ` 前缀的 Access Token。

推荐使用 Refresh Token：源会换取 Access Token，在到期前续期，并保存 Pixiv 轮换返回的 Refresh Token。直接使用 Access Token 时，到期后需手动更换。网页登录得到的是网页会话，不能自动转换成 Refresh Token；Token 需要事先自行取得。

两套登录凭据独立保存，切换“登录方式”即可切换请求方式。网页登录成功时会自动选择网页登录。点击“退出 Token 登录”会清除输入的 Token 和续期缓存。Token 和 Cookie 存在 Aidoku 的源设置中；输入框掩码显示，不在源码、日志或图片请求中附带 Token。请勿分享含凭据的设置或备份。

## 功能与范围

- 每日、每周、每月和漫画排行榜。
- 关键词搜索、标签部分/完全匹配、标题与说明搜索，按时间升降序排列。
- 搜索框可直接输入作品 ID 或 `https://www.pixiv.net/artworks/作品ID`。
- 作者、标签、说明、发布日和多图阅读；支持原图与普通画质。
- 一个作品对应一本书，所有图片合为一章，按 Pixiv 原始顺序读取。
- 阅读设置 → 内容过滤中有“显示 AI 生成作品”和“显示成人向作品”两个开关，新安装时均关闭；账号自身权限仍由 Pixiv 控制。

内容过滤应用于排行榜、搜索、直接打开作品和阅读。AI 过滤依据 Pixiv 的 AI 生成标记，不推测未标记作品的创作方式。网页排行榜缺少过滤标记时，通过批量接口补充，保留原始排名和分页；不会为每个作品单独请求。升级时自动将旧版“隐藏成人作品”的选择迁移到“显示成人向作品”，保留原有偏好。

暂不支持小说、ugoira 动图和将多个投稿合并成漫画系列。Aidoku 收藏仅加入本地书库，不同步到 Pixiv 收藏。设备需要能够连接 `www.pixiv.net`、`accounts.pixiv.net`、`app-api.pixiv.net`、`oauth.secure.pixiv.net` 及图片域名 `*.pximg.net`。

如果网页会话失效，退出后重新网页登录；Access Token 失效则更新 Token。遇到频率限制请稍后重试。如果网页验证码无法完成，可使用 Token 登录。

第 4 版修复了排行榜显示“无法识别的响应”的问题：Pixiv 的 `ranking.php?format=json` 在 Aidoku 默认的 iPhone 浏览器标识下仍返回移动版 HTML 网页。排行榜请求单独使用桌面浏览器标识并声明接收 JSON；网页登录验证、搜索和作品详情仍使用 Aidoku 默认标识。错误提示区分 HTML 网页、空响应、无效 JSON 和读取失败，并标明失败的接口类型，不输出响应正文或凭据。

第 3 版已修复“网页已登录但源仍提示先登录”的会话保存问题：登录入口使用 `pixiv.net`，兼容 Aidoku 对 `.pixiv.net` 父域 Cookie 的提取；回调主动持久化 Cookie，并在 Pixiv 验证成功后保存登录标记。请求使用完整的站点会话 Cookie；Cloudflare 验证 Cookie 由客户端管理。旧版若已有 Cookie 但缺少登录标记，可在服务端验证后自动恢复，验证暂时失败时保留 Cookie；退出登录会清除会话。

更新到第 4 版后，已保存的登录状态会继续使用，返回排行榜重试即可。如果第 1/2 版从未捕获到 Cookie，请在源设置中再次打开“登录 Pixiv”，等待源验证并自动关闭窗口；不需要向他人提供 Cookie 或 Token。

## 构建与验证

需要 Rust stable、`wasm32-unknown-unknown` 目标、Python 3.11+；运行 WASM 集成测试还需要 Node.js。SDK 固定在 `vendor/`，不需要安装 Aidoku CLI。

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --locked
cargo fetch --locked
python scripts/licenses.py
python scripts/package.py
python scripts/build_list.py
```

生成 `dist/Pixiv.aix`。包内使用 Aidoku 官方 `Payload/main.wasm` 及资源布局。

`scripts/build_list.py` 按 Aidoku CLI 的格式生成 `public/index.json`、`index.min.json`、版本化的 `sources/multi.pixiv-v*.aix` 和 `icons/multi.pixiv-v*.png`。提交到 `main` 后自动校验、编译和发布，PR 仅校验并提供构建产物。

## 仓库结构

```text
sources/multi.pixiv/   # Cargo.toml、src/、res/：Pixiv 源
vendor/               # 固定版本的 Aidoku SDK、Schema 和许可证
scripts/              # 配置校验、许可证、打包和生成订阅
tests/                # WASM 认证及阅读集成测试
public/               # 源订阅发布目录，自动发布至 gh-pages
dist/Pixiv.aix        # 可手动导入的安装包
.github/workflows/    # GitHub Actions 构建与发布
```

## 检查命令

```sh
cargo fmt -p aidoku-pixiv --check
cargo clippy --release --locked -- -D warnings
node tests/wasm-auth.mjs
python -m pip install -r requirements-dev.txt
python scripts/verify.py
```

纯解析测试需要显式指定本机 Rust host target（可通过 `rustc -vV` 查看），例如 Windows GNU：

```sh
cargo test --lib --release --target x86_64-pc-windows-gnu --locked
```

使用 release 测试是因为 Aidoku SDK 依赖 WASM 宿主导入，优化需要裁剪原生测试未使用的宿主函数；完整认证流程在真实 WASM 二进制上通过 Node 宿主测试。

### 当前验证结果

- 成功编译 release WASM，官方 Schema 配置校验及 Clippy 检查通过。
- 9 项 Rust 解析测试和 28 项 WASM 集成测试，覆盖 Cookie 验证、过期会话、两种 Token、续期与轮换、认证失败重试、分页、详情、图片顺序、退出清理、内容过滤和旧设置迁移，以及父域 Cookie 提取、登录后重启、缺失标记恢复和验证失败后的会话保留。排行榜回归覆盖四个页签、翻页、iPhone HTML 响应和错误类型区分。
- 已通过不带用户凭据的只读请求复现 iPhone 标识返回 HTML、桌面标识返回 JSON；修复所用标识下四种排行榜前两页均返回 50 项，并核对默认移动标识下批量过滤元数据、搜索、作品详情和图片列表的 JSON 响应。
- **尚未使用真实账号在 Aidoku 设备上完成网页登录、Token 兑换及阅读验收。** 集成测试使用模拟 HTTP 响应，不等于真实账号联调通过。

## 开发参考

- [Aidoku 源开发文档](https://aidoku.github.io/aidoku-rs/book/)
- [Aidoku SDK](https://github.com/Aidoku/aidoku-rs)，固定提交见 `vendor/README.md`。
- [Aidoku 客户端网页登录实现](https://github.com/Aidoku/Aidoku/blob/main/Aidoku/App/Common/Settings/SettingView.swift)
- [PixivPy API 实现](https://github.com/upbit/pixivpy)，用于核对移动端 API 及认证参数。

源项目使用 MIT 许可。`vendor/aidoku` 和 JSON Schema 来自 Aidoku SDK，保留原项目 MIT 许可。本项目与 Pixiv、Aidoku 官方无隶属关系。
