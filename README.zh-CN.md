# Seen

**为你的人类 Git 工作流打造的精美 TUI**

`seen` 是一个极速、激进且高颜值的 Git 终端用户界面。我们不试图取代 IDE，而是专注于那 10% IDE 变得 Dead Weight 之后，开发者最需要的一件事：**纯粹的代码阅读、状态审查与极速提交。**

---

### Why Seen?

当 AI 完成了绝大部分代码编写工作，IDE 的 90% 功能变得冗余。你不再需要一个沉重的编辑器来写代码，你需要的是一个轻量级的工作台，专注于：浏览文件、阅读逻辑、全局搜索、以及提交代码前的 Git 审查。

## What's in the box

* **双面板布局** — 左侧文件树，右侧信息预览，让你的视角保持连贯、专注。
* **键盘操作** — 所有功能皆可通过快捷键瞬间完成，无需在键盘和鼠标之间频繁切换。
* **Git 工作流引擎** — 支持 `stage`/`unstage` 操作，清晰的 Unified/Side-by-side Diff 对比，支持 `push`/`force-push`。
* **智能状态追踪** — 实时了解你的暂存区、未暂存文件以及当前 Git 状态。

* **跨平台** — 支持 macOS (arm64, x64), Linux (arm64, x64), Windows (x64)。原生 Rust 二进制包，零 IPC 延迟。

## Install

**Via npm (推荐):**

Bash

```
# 无需安装直接运行
npx @seen-tui/cli

# 或者全局安装
npm install -g @seen-tui/cli
seen
```

*系统将自动为你选择对应的原生二进制程序。*

**从源码构建:**

Bash

```
cargo build --release
# 在任何 git 仓库中运行:
cd your-git-repo
/path/to/target/release/seen
```

## Keybindings

| **Key**      | **Action**                          |
| -------------------- | ------------------------------------------- |
| **q, ESC**     | 退出                                      |
| **1 / 2** | 切换标签页 ( Git / Graph) |
| **Tab**           | 切换焦点面板                              |


## Status

**Alpha.** 单进程 Rust 二进制实现 — 无插件，无 IPC。Files, Git 和 Graph 均为原生实现。
