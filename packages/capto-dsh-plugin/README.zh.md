# capto-dsh-plugin

[English](README.md)

为 **[DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness)（一切皆插件）提供 Capto 录屏控制的一等公民工具**。

[![npm version](https://img.shields.io/npm/v/capto-dsh-plugin?style=flat-square)](https://www.npmjs.com/package/capto-dsh-plugin)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/ciryanna/Capto/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/ciryanna/Capto/actions/workflows/ci.yml)
[![Windows](https://img.shields.io/badge/platform-Windows%2010%2B-0078D4?style=flat-square&logo=windows&logoColor=white)](https://github.com/ciryanna/Capto/releases)

`capto-dsh-plugin` 注册一组类型化的 `capto_*` 工具（`capto_status`、`capto_record_start`、`capto_shot` …），让 Harness agent 在任意对话里直接驱动**纯本地的 [Capto](https://github.com/ciryanna/Capto) Windows 录屏软件**——不 shell 出去、不碰 FFmpeg、不上云。

- 🎬 **14 个类型化工具** —— status / doctor / open / list / shot / record / config / outputs，每个都有严格的参数 schema 和规范的 JSON 输出
- 🧭 **面向 agent 的失败归一化** —— 每个 CLI 退出码都归一成 `Error: capto exited <code> (<errorCode>): <message>`；`desktopUnavailable`（exit 2）会直接告诉模型下一步（`capto_open` → 等待 → 重试）
- 🔌 **零构建步骤** —— 纯 ESM，从 npm、tarball 或 git 检出装上都可直接运行
- 🔒 **纯本地** —— 只通过 localhost 控制面与正在运行的 Capto 桌面通信；绝不 spawn FFmpeg、绝不上传任何东西

## 环境要求

- **Windows 10+**，已安装 Capto 桌面端——[安装包](https://github.com/ciryanna/Capto/releases)内置 FFmpeg 与 `capto` CLI，并把 `cli\` 加入 PATH
- 正在运行的 **DeepSeek Harness** profile（`dsh web`、`dsh --profile headless` …）
- Node ≥ 18（harness 运行时）

## 安装

### npm（装进 profile）

```bash
npm install --prefix ~/.dsh/profiles/web capto-dsh-plugin
```

在 profile 的 `cordis.patch.yml` 里启用：

```yaml
- insert:
    - id: capto
      name: capto-dsh-plugin
      config:
        command: ['capto']            # 或 capto.exe 的绝对路径
        timeoutMs: 120000
        noLaunch: false
        autoOpen: false
```

校验配置树合成后重启 GUI：

```bash
dsh --profile web --dump-config
# 重启 `dsh web` —— agent 工具集里就会出现 capto_* 工具
```

### dsh plugin（bundle 形态）

包声明了 [`dsh.bundle`](cordis.patch.yml) 清单，官方 pnpm 流程可一键接线：

```bash
dsh plugin --profile web add capto-dsh-plugin
dsh --profile web --dump-config   # 出现 "# == capto-dsh-plugin" 层
```

上面的 npm 方式适用于 npm 管理的 profile，二者等价。

### 从检出目录（开发态）

```bash
npm install --prefix ~/.dsh/profiles/web "file:D:/path/to/Capto/packages/capto-dsh-plugin"
```

npm ≥ 7 会把 `file:` 依赖装成链接，源码改动在下一次重启 GUI 时生效。

卸载：`npm uninstall --prefix ~/.dsh/profiles/web capto-dsh-plugin`（或 `dsh plugin --profile web remove capto-dsh-plugin`），再删掉 patch 行。

## 配置

| 键 | 默认 | 说明 |
|---|---:|---|
| `command` | `['capto']` | CLI argv 前缀：`['capto']`（安装版 PATH）、`['D:\\…\\capto.exe']`、或开发态 `['cargo','run','-p','capto-cli','--']` |
| `timeoutMs` | `120000` | 单次调用超时（毫秒）。必须大于 CLI 自动拉起的 45s 等待，冷启动桌面时才不会提前掐断 |
| `noLaunch` | `false` | 总是传 `--no-launch`，绝不自动拉起桌面 |
| `autoOpen` | `false` | exit 2（`desktopUnavailable`）时自动 `capto open` → 等约 3s → 重试一次 |

非法配置（空 `command`、非正 `timeoutMs`、非布尔 flag）会在插件加载时直接失败并给出可操作的报错。

## 工具

| 工具 | 用途 |
|---|---|
| `capto_status` | 会话快照 `{ state, elapsedMs, outputPath, … }` —— 录制前必查 |
| `capto_doctor` | 环境就绪度 `{ ffmpegOk, controlPlane, … }` |
| `capto_open` | 打开桌面窗口（exit 2 的恢复入口） |
| `capto_list` | 枚举 `displays` / `windows` / `audio` / `encoders` |
| `capto_shot` | 截图 → `{ path }`（绝对 PNG） |
| `capto_record_start` | 开始录制（source、display/window/region、format、fps、quality、encoder、mic、loopback、cursor） |
| `capto_record_stop` / `capto_record_pause` / `capto_record_resume` | 录制控制 |
| `capto_config_get` / `capto_config_set` / `capto_config_path` | 读 / 改设置（camelCase 键） |
| `capto_outputs_recent` / `capto_outputs_open` | 最近输出 / 在资源管理器打开文件 |

每个工具都声明规范 JSON 输出和美观渲染；只读工具标记为并发安全。`tool:capto` 提示段教会 agent：start 前先 `capto_status`、绝不重复 start、exit 2 用 `capto_open` 恢复、结束必 `capto_record_stop`、用 `capto_outputs_recent` 找文件。

## 模型体验

- **模型看到什么** —— 14 个工具 schema 加一小段 `tool:capto` 指引：插件激活期间每个请求有固定的小额 token 开销。
- **失败** —— 以 `Error: …` 抛出并携带 CLI 退出码与错误码。`desktopUnavailable` 的报错会给出模型的下一步（`capto_open` → 等 3–5s → 重试；仍失败就请用户打开 Capto）。
- **取消** —— 调用感知工具信号；被取消的调用抛 `AbortError`，不会留下孤儿 CLI 子进程。

## 开发与验证

```bash
cd packages/capto-dsh-plugin
npm install            # 测试依赖（@deepseek-ai/dsh-tools、schemastery）
node test/smoke.mjs    # 11 项检查：契约、配置、参数映射、失败归一化、
                       # 超时、autoOpen 恢复、真实 CLI
```

`test/fixtures/fake-capto.mjs` 是一个按 JSON 信封契约吐数据的假 `capto` CLI，测试无需桌面即可跑。若存在 `target/debug/capto.exe`，还会跑一条真实 CLI 检查（两种控制面状态都接受）。CI 在 Windows 上对着刚构建的 CLI 跑全套，Ubuntu 上只跑 fake 路径，外加 `npm pack --dry-run`。

## 已知限制与后续工作

- **仅限 Windows（设计使然）** —— Capto 是 Windows 录屏软件；其他平台无意义。
- **单一桌面会话** —— 工具只驱动那一个 Capto 桌面进程；不存在第二条采集管线（这是 Capto 的保证，由其控制面强制）。
- **结果里没有图片预览** —— `capto_shot` 只返回路径；harness 目前不支持工具结果内渲染图片。
- **`autoOpen` 是尽力而为** —— 只在 `capto open` 后重试一次；僵死的桌面（陈旧进程）仍需按 Capto skill 文档手动杀掉重启。

## 许可证

MIT —— 属于 [Capto](https://github.com/ciryanna/Capto) 项目的一部分。见 [LICENSE](LICENSE)。
