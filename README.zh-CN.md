<p align="center">
  <img src="apps/desktop/public/capto-mark.png" alt="Capto" width="220" />
</p>

<h1 align="center">Capto</h1>

<p align="center">
  <strong>超轻量 Windows 屏幕录制</strong><br />
  <a href="https://github.com/MathewSachin/Captura">Captura</a> 的精神续作。
</p>

<p align="center">
  <a href="README.md">English</a> ·
  <a href="https://ciryanna.github.io/Capto/">Website</a> ·
  <a href="https://github.com/ciryanna/Capto/releases">Releases</a>
</p>

## 为什么选 Capto

| | |
|:---:|:---|
| 🪟 | **模式 + Windows 特调** — 显示器 / 窗口 / 区域；Windows 优先采集与音频路径，而不是泛平台凑合实现。 |
| 🎬 | **MP4 与 GIF** — 录制 MP4（自动 NVENC / QSV / AMF / libx264）或导出 GIF。 |
| ✨ | **Overlay** — 鼠标点击高亮、按键显示、摄像头画中画，以及光标开关与实时预览。 |
| 🎞️ | **自研捆绑 FFmpeg（`capto-ffmpeg`）** — Capto 自有、版本钉死、可验证的 FFmpeg 侧车；编码只走这一份，不碰系统 PATH。 |
| 🤖 | **CLI + 双 Agent 集成** — 完备的 `capto` 本机控制面，加上 [`capto-agent-skill`](https://www.npmjs.com/package/capto-agent-skill)（Agent Skills）与 [`capto-dsh-plugin`](https://www.npmjs.com/package/capto-dsh-plugin)（DeepSeek Harness 工具），agent 可直接 doctor → record → stop → 取输出。 |
| 🔒 | **开源 · 本地 · 安全** — MIT；无上传 SDK；文件只留在本机。 |

## 安装

从 [Releases](https://github.com/ciryanna/Capto/releases) 下载 x64 / arm64 NSIS 安装包。内嵌经验证的 FFmpeg（[`capto-ffmpeg`](https://github.com/ciryanna/capto-ffmpeg)）以及 CLI：`<安装目录>\cli\capto.exe`；安装时会把该 `cli` 目录写入用户 **PATH**，新开终端即可直接运行 `capto`（不再单独发布 CLI）。

### AI / Agent 支持

Capto 提供两种 agent 集成，都走同一个本机 `capto` CLI 控制面。先装上面的桌面端，再按你的 agent 选择：

**Agent Skill — [`capto-agent-skill`](https://www.npmjs.com/package/capto-agent-skill)**

适用于任何兼容 [Agent Skills](https://agentskills.io) 的 agent（Claude Code、Cursor …）：

```bash
npm install capto-agent-skill
```

skill 会从 `node_modules/capto-agent-skill/skills/capto/SKILL.md` 被自动发现（agentskills / skills-npm 约定），教会 agent doctor → record → stop → 收集输出的完整流程。

**DeepSeek Harness 插件 — [`capto-dsh-plugin`](https://www.npmjs.com/package/capto-dsh-plugin)**

为 dsh profile 提供一等公民的 `capto_*` 工具（status / record / shot / config / outputs …）：

```bash
# npm 管理的 profile：安装 …
npm install --prefix ~/.dsh/profiles/web capto-dsh-plugin
```

…然后在 profile 的 `cordis.patch.yml` 里启用：

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

…或走官方 pnpm / bundle 流程（通过 `dsh.bundle` 自动接线）：

```bash
dsh plugin --profile web add capto-dsh-plugin
dsh --profile web --dump-config
```

重启 `dsh web` 后，agent 工具集里就会出现 `capto_*`。配置参考与工具表见 [`packages/capto-dsh-plugin`](packages/capto-dsh-plugin)。

## 开发者

**Elwina Vardal** · [elwina.work](https://www.elwina.work) · [GitHub](https://github.com/elwina)

## 许可

MIT（全新实现，不 fork Captura 源码）
