## Status

**Alpha.** 单进程 Rust 二进制实现 — 无插件，无 IPC。Files, Git 和 Graph 均为原生实现。

# Seen

**A beautiful TUI for your Git workflow.**

`seen` is a fast, aggressive, and stunning Git TUI. We don’t try to replace your IDE. Instead, we focus on the 10% of the time when an IDE becomes dead weight: **pure code reading, status review, and blazing-fast commits.**

---

### Why Seen?

When AI writes most of your code, an IDE's surface shrinks to four things: browsing files, reading files, searching, and walking git diffs before a commit. `seen` is a terminal workbench for exactly that — and nothing else.

* **No** bloat.

* **No** plugins.
* **No** language server.
* **Just pure Git productivity.**

## What's in the box

* **Dual-pane layout** — File tree on the left, preview on the right. Keep your context focused.

* **Keyboard first** — All features are accessible via shortcuts. Say goodbye to the mouse.
* **Git Powerhouse** — Stage/unstage, unified or side-by-side diffs, and push/force-push support.
* **Smart Tracking** — Real-time awareness of your staged files, unstaged changes, and Git status.
* **Native Performance** — Built in Rust as a single, zero-IPC binary. Supports macOS, Linux, and Windows.

## Install

**Via npm (Recommended):**

Bash

```
# Run without installing
npx @seen-tui/cli

# Or install globally
npm install -g @seen-tui/cli
seen
```

*The correct native binary for your platform is selected automatically.*

**Build from source:**

Bash

```
cargo build --release
# Run from inside any git repo:
cd your-git-repo
/path/to/target/release/seen
```

## Keybindings

| **Key**      | **Action**                              |  |  |
| -------------------- | ----------------------------------------------- | -- | -- |
| **q, ESC**     | Quit                                          |
| **1 / 2** | Tab navigation (Files / Search / Git / Graph) |
| **Tab**           | Switch focused panel                          |


## Status

**Alpha.** Single-process Rust binary — no plugins, no IPC. Files, Git, and Graph are all host-native.
