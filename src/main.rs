use std::{
    error::Error,
    io,
    process::Command,
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    // prelude::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
    Terminal,
};

const TAB_TITLES: [&str; 2] = ["Git", "Graph"];
const GIT_TAB_INDEX: usize = 0;
const GRAPH_TAB_INDEX: usize = 1;
const GRAPH_COMMIT_LIMIT: usize = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileSection {
    Staged,
    Unstaged,
    Untracked,
}

impl FileSection {
    // 返回左侧分组标题文本。
    fn title(self) -> &'static str {
        match self {
            Self::Staged => "STAGED",
            Self::Unstaged => "UNSTAGED",
            Self::Untracked => "UNTRACKED",
        }
    }

    // 返回右侧 diff 标题里展示的模式文本。
    fn mode_label(self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Unstaged => "unstaged",
            Self::Untracked => "untracked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusPane {
    Files,
    Diff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GraphFocusPane {
    Commits,
    Files,
    Diff,
}

impl GraphFocusPane {
    // Graph 页内循环切换焦点。
    fn next(self) -> Self {
        match self {
            Self::Commits => Self::Files,
            Self::Files => Self::Diff,
            Self::Diff => Self::Commits,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GraphRefKind {
    Head,
    Branch,
    Remote,
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitAction {
    PullFfOnly,
    PushCurrentBranch,
    Branches,
}

impl GitAction {
    // Git actions popup 里显示的文案。
    fn label(self) -> &'static str {
        match self {
            Self::PullFfOnly => "Pull (ff-only)",
            Self::PushCurrentBranch => "Push current branch",
            Self::Branches => "Branches",
        }
    }
}

const GIT_ACTIONS: [GitAction; 3] = [
    GitAction::PullFfOnly,
    GitAction::PushCurrentBranch,
    GitAction::Branches,
];

#[derive(Debug, Clone)]
struct GraphRefChip {
    label: String,
    kind: GraphRefKind,
}

#[derive(Debug, Clone)]
struct GitFile {
    status_code: String,
    display_status: String,
    path: String,
    diff_path: String,
    section: FileSection,
}

#[derive(Debug, Clone)]
struct CommitFile {
    status: String,
    path: String,
    diff_path: String,
}

#[derive(Debug, Clone)]
struct GraphCommit {
    graph_prefix: String,
    sha: String,
    short_sha: String,
    subject: String,
    author: String,
    date: String,
    refs: Vec<GraphRefChip>,
    body: Vec<String>,
    changed_files: Vec<CommitFile>,
    details_loaded: bool,
}

#[derive(Debug)]
struct App {
    branch_name: String,
    active_tab: usize,
    focus: FocusPane,
    files: Vec<GitFile>,
    selected_index: usize,
    diff_lines: Vec<String>,
    diff_scroll: u16,
    graph_focus: GraphFocusPane,
    graph_commits: Vec<GraphCommit>,
    selected_commit_index: usize,
    selected_commit_file_index: usize,
    graph_diff_lines: Vec<String>,
    graph_diff_scroll: u16,
    status_message: String,
    git_action_popup_open: bool,
    selected_git_action_index: usize,
    commit_input_popup_open: bool,
    commit_input: String,
    push_confirm_popup_open: bool,
    branch_popup_open: bool,
    branches: Vec<String>,
    selected_branch_index: usize,
    should_quit: bool,
}

impl App {
    // 初始化应用状态，并首次加载 Git 和 Graph 两个视图的数据。
    fn new() -> Self {
        let mut app = Self {
            branch_name: get_branch_name(),
            active_tab: GIT_TAB_INDEX,
            focus: FocusPane::Files,
            files: Vec::new(),
            selected_index: 0,
            diff_lines: Vec::new(),
            diff_scroll: 0,
            graph_focus: GraphFocusPane::Commits,
            graph_commits: Vec::new(),
            selected_commit_index: 0,
            selected_commit_file_index: 0,
            graph_diff_lines: Vec::new(),
            graph_diff_scroll: 0,
            status_message: String::new(),
            git_action_popup_open: false,
            selected_git_action_index: 0,
            commit_input_popup_open: false,
            commit_input: String::new(),
            push_confirm_popup_open: false,
            branch_popup_open: false,
            branches: Vec::new(),
            selected_branch_index: 0,
            should_quit: false,
        };
        app.reload_files(None, None);
        app.reload_graph_commits();
        app
    }

    // 取出当前左侧选中的工作区文件。
    fn selected_file(&self) -> Option<&GitFile> {
        self.files.get(self.selected_index)
    }

    // 取出当前弹窗里选中的分支名。
    fn selected_branch(&self) -> Option<&str> {
        self.branches
            .get(self.selected_branch_index)
            .map(|branch| branch.as_str())
    }

    // 取出当前 Git action。
    fn selected_git_action(&self) -> GitAction {
        GIT_ACTIONS[self.selected_git_action_index.min(GIT_ACTIONS.len().saturating_sub(1))]
    }

    // 取出当前 Graph 页选中的 commit。
    fn selected_graph_commit(&self) -> Option<&GraphCommit> {
        self.graph_commits.get(self.selected_commit_index)
    }

    // 取出当前 Graph 页选中的文件。
    fn selected_graph_file(&self) -> Option<&CommitFile> {
        self.selected_graph_commit()
            .and_then(|commit| commit.changed_files.get(self.selected_commit_file_index))
    }

    // 左侧选中项向下移动，并刷新右侧 diff。
    fn move_selection_down(&mut self) {
        if !self.files.is_empty() && self.selected_index + 1 < self.files.len() {
            self.selected_index += 1;
            self.diff_scroll = 0;
            self.refresh_selected_diff();
        }
    }

    // 左侧选中项向上移动，并刷新右侧 diff。
    fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.diff_scroll = 0;
            self.refresh_selected_diff();
        }
    }

    // Graph 左栏选中的 commit 向下移动。
    fn move_graph_commit_down(&mut self) {
        if !self.graph_commits.is_empty() && self.selected_commit_index + 1 < self.graph_commits.len() {
            self.selected_commit_index += 1;
            self.selected_commit_file_index = 0;
            self.graph_diff_scroll = 0;
            self.refresh_selected_graph_details();
        }
    }

    // Graph 左栏选中的 commit 向上移动。
    fn move_graph_commit_up(&mut self) {
        if self.selected_commit_index > 0 {
            self.selected_commit_index -= 1;
            self.selected_commit_file_index = 0;
            self.graph_diff_scroll = 0;
            self.refresh_selected_graph_details();
        }
    }

    // Graph 中栏选中的文件向下移动。
    fn move_graph_file_down(&mut self) {
        let file_count = self
            .selected_graph_commit()
            .map(|commit| commit.changed_files.len())
            .unwrap_or(0);

        if file_count > 0 && self.selected_commit_file_index + 1 < file_count {
            self.selected_commit_file_index += 1;
            self.graph_diff_scroll = 0;
            self.refresh_selected_graph_diff();
        }
    }

    // Graph 中栏选中的文件向上移动。
    fn move_graph_file_up(&mut self) {
        if self.selected_commit_file_index > 0 {
            self.selected_commit_file_index -= 1;
            self.graph_diff_scroll = 0;
            self.refresh_selected_graph_diff();
        }
    }

    // Git actions popup 中的选中项向下移动。
    fn move_git_action_down(&mut self) {
        if self.selected_git_action_index + 1 < GIT_ACTIONS.len() {
            self.selected_git_action_index += 1;
        }
    }

    // Git actions popup 中的选中项向上移动。
    fn move_git_action_up(&mut self) {
        if self.selected_git_action_index > 0 {
            self.selected_git_action_index -= 1;
        }
    }

    // 在 commit 输入框里追加一个字符。
    fn append_commit_input(&mut self, ch: char) {
        self.commit_input.push(ch);
    }

    // 删除 commit 输入框最后一个字符。
    fn backspace_commit_input(&mut self) {
        self.commit_input.pop();
    }

    // 工作区右侧 diff 向下滚动。
    fn scroll_diff_down(&mut self) {
        self.diff_scroll = self.diff_scroll.saturating_add(1);
    }

    // 工作区右侧 diff 向上滚动。
    fn scroll_diff_up(&mut self) {
        self.diff_scroll = self.diff_scroll.saturating_sub(1);
    }

    // Graph 右栏 diff 向下滚动。
    fn scroll_graph_diff_down(&mut self) {
        self.graph_diff_scroll = self.graph_diff_scroll.saturating_add(1);
    }

    // Graph 右栏 diff 向上滚动。
    fn scroll_graph_diff_up(&mut self) {
        self.graph_diff_scroll = self.graph_diff_scroll.saturating_sub(1);
    }

    // 根据当前选中项重新加载工作区右侧 diff 内容。
    fn refresh_selected_diff(&mut self) {
        self.diff_lines = self
            .selected_file()
            .map(get_diff_lines)
            .unwrap_or_else(|| vec![String::from("No changed files")]);
    }

    // 重新读取 git 状态，并尽量把选中项保持在原文件上。
    fn reload_files(&mut self, preferred_path: Option<&str>, preferred_section: Option<FileSection>) {
        self.files = parse_git_status(&get_git_status());

        self.selected_index = preferred_path
            .and_then(|path| {
                preferred_section.and_then(|section| {
                    self.files
                        .iter()
                        .position(|file| file.diff_path == path && file.section == section)
                })
            })
            .or_else(|| preferred_path.and_then(|path| self.files.iter().position(|file| file.diff_path == path)))
            .unwrap_or(0)
            .min(self.files.len().saturating_sub(1));

        self.diff_scroll = 0;
        self.refresh_selected_diff();
    }

    // 重新加载 Graph 页提交列表，并刷新当前详情与右侧 diff。
    fn reload_graph_commits(&mut self) {
        self.graph_commits = get_graph_commits();
        self.selected_commit_index = self
            .selected_commit_index
            .min(self.graph_commits.len().saturating_sub(1));
        self.selected_commit_file_index = 0;
        self.graph_diff_scroll = 0;
        self.refresh_selected_graph_details();
    }

    // 确保当前 commit 的详情和改动文件已加载，再刷新右侧文件 diff。
    fn refresh_selected_graph_details(&mut self) {
        let Some((index, sha, loaded)) = self
            .graph_commits
            .get(self.selected_commit_index)
            .map(|commit| (self.selected_commit_index, commit.sha.clone(), commit.details_loaded))
        else {
            self.graph_diff_lines = vec![String::from("No commits found")];
            return;
        };

        if !loaded {
            let (body, changed_files) = load_commit_details(&sha);
            if let Some(commit) = self.graph_commits.get_mut(index) {
                commit.body = body;
                commit.changed_files = changed_files;
                commit.details_loaded = true;
            }
        }

        let file_count = self
            .graph_commits
            .get(index)
            .map(|commit| commit.changed_files.len())
            .unwrap_or(0);

        self.selected_commit_file_index = self.selected_commit_file_index.min(file_count.saturating_sub(1));
        self.refresh_selected_graph_diff();
    }

    // 根据当前 commit 和文件重新加载 Graph 右侧 diff。
    fn refresh_selected_graph_diff(&mut self) {
        let Some(commit) = self.selected_graph_commit() else {
            self.graph_diff_lines = vec![String::from("No commits found")];
            return;
        };

        let Some(file) = commit.changed_files.get(self.selected_commit_file_index) else {
            self.graph_diff_lines = vec![String::from("No changed files for this commit")];
            return;
        };

        self.graph_diff_lines = get_commit_file_diff_lines(&commit.sha, file);
    }

    // 手动刷新整个应用里依赖 Git 的视图状态。
    fn refresh_all(&mut self) {
        self.branch_name = get_branch_name();
        self.reload_files(None, None);
        self.reload_graph_commits();
        self.status_message = String::from("Refreshed");
    }

    // 打开 Git actions popup。
    fn open_git_action_popup(&mut self) {
        self.git_action_popup_open = true;
        self.selected_git_action_index = 0;
    }

    // 关闭 Git actions popup。
    fn close_git_action_popup(&mut self) {
        self.git_action_popup_open = false;
    }

    // 打开 commit 输入框，并清空旧输入。
    fn open_commit_input_popup(&mut self) {
        self.commit_input_popup_open = true;
        self.commit_input.clear();
    }

    // 关闭 commit 输入框。
    fn close_commit_input_popup(&mut self) {
        self.commit_input_popup_open = false;
        self.commit_input.clear();
    }

    // 提交 commit 输入框内容，进入 push 确认弹窗。
    fn submit_commit_input(&mut self) {
        if self.commit_input.trim().is_empty() {
            self.status_message = String::from("Commit message cannot be empty");
            return;
        }

        self.commit_input_popup_open = false;
        self.push_confirm_popup_open = true;
    }

    // 打开分支弹窗，并把高亮定位到当前分支。
    fn open_branch_popup(&mut self) {
        self.branches = get_local_branches();
        self.selected_branch_index = self
            .branches
            .iter()
            .position(|branch| branch == &self.branch_name)
            .unwrap_or(0)
            .min(self.branches.len().saturating_sub(1));
        self.branch_popup_open = true;

        if self.branches.is_empty() {
            self.status_message = String::from("No branches found");
        }
    }

    // 关闭分支弹窗。
    fn close_branch_popup(&mut self) {
        self.branch_popup_open = false;
    }

    // 关闭 push 确认弹窗。
    fn close_push_confirm_popup(&mut self) {
        self.push_confirm_popup_open = false;
    }

    // 执行当前选中的 Git action。
    fn run_selected_git_action(&mut self) {
        match self.selected_git_action() {
            GitAction::PullFfOnly => self.pull_ff_only(),
            GitAction::PushCurrentBranch => self.open_commit_input_popup(),
            GitAction::Branches => {
                self.git_action_popup_open = false;
                self.open_branch_popup();
            }
        }
    }

    // 执行 ff-only pull，并刷新 Git / Graph 两个视图。
    fn pull_ff_only(&mut self) {
        match pull_ff_only() {
            Ok(_) => {
                self.branch_name = get_branch_name();
                self.reload_files(None, None);
                self.reload_graph_commits();
                self.git_action_popup_open = false;
                self.status_message = String::from("Pull completed");
            }
            Err(error) => {
                self.git_action_popup_open = false;
                self.status_message = format!("Pull failed: {error}");
            }
        }
    }

    // 确认后执行 commit 和 push，并关闭相关弹窗。
    fn commit_and_push(&mut self) {
        let message = self.commit_input.trim().to_string();
        if message.is_empty() {
            self.push_confirm_popup_open = false;
            self.status_message = String::from("Commit message cannot be empty");
            return;
        }

        if self.staged_count() == 0 {
            self.push_confirm_popup_open = false;
            self.git_action_popup_open = false;
            self.commit_input.clear();
            self.status_message = String::from("No staged changes to commit");
            return;
        }

        match commit_with_message(&message) {
            Ok(_) => {
                self.branch_name = get_branch_name();
                self.reload_files(None, None);
                self.reload_graph_commits();
            }
            Err(error) => {
                self.push_confirm_popup_open = false;
                self.git_action_popup_open = false;
                self.commit_input.clear();
                self.status_message = format!("Commit failed: {error}");
                return;
            }
        }

        match push_current_branch() {
            Ok(_) => {
                self.branch_name = get_branch_name();
                self.reload_files(None, None);
                self.reload_graph_commits();
                self.push_confirm_popup_open = false;
                self.git_action_popup_open = false;
                self.commit_input.clear();
                self.status_message = String::from("Committed and pushed");
            }
            Err(error) => {
                self.push_confirm_popup_open = false;
                self.git_action_popup_open = false;
                self.commit_input.clear();
                self.status_message = format!("Push failed: {error}");
            }
        }
    }

    // 暂存当前选中的文件，并刷新左右两侧内容。
    fn stage_selected(&mut self) {
        let Some(file) = self.selected_file().cloned() else {
            self.status_message = String::from("Nothing to stage");
            return;
        };

        if file.section == FileSection::Staged {
            self.status_message = format!("{} is already staged", file.path);
            return;
        }

        match run_git_path_command(&["add", "--"], &file.diff_path) {
            Ok(_) => {
                self.reload_files(Some(&file.diff_path), Some(FileSection::Staged));
                self.status_message = format!("Staged {}", file.path);
            }
            Err(error) => {
                self.status_message = format!("Stage failed: {error}");
            }
        }
    }

    // 取消暂存当前选中的文件，并刷新左右两侧内容。
    fn unstage_selected(&mut self) {
        let Some(file) = self.selected_file().cloned() else {
            self.status_message = String::from("Nothing to unstage");
            return;
        };

        if file.section != FileSection::Staged {
            self.status_message = format!("{} is not in staged changes", file.path);
            return;
        }

        match unstage_file(&file) {
            Ok(message) => {
                self.reload_files(Some(&file.diff_path), None);
                self.status_message = format!("{} {}", message, file.path);
            }
            Err(error) => {
                self.status_message = format!("Unstage failed: {error}");
            }
        }
    }

    // 切换到弹窗中选中的分支，并刷新 Git / Graph 两个视图。
    fn switch_selected_branch(&mut self) {
        let Some(branch) = self.selected_branch().map(str::to_owned) else {
            self.status_message = String::from("No branch selected");
            return;
        };

        if branch == self.branch_name {
            self.status_message = format!("Already on {branch}");
            self.branch_popup_open = false;
            self.git_action_popup_open = false;
            return;
        }

        match checkout_branch(&branch) {
            Ok(_) => {
                self.branch_name = get_branch_name();
                self.reload_files(None, None);
                self.reload_graph_commits();
                self.branch_popup_open = false;
                self.git_action_popup_open = false;
                self.status_message = format!("Switched to {branch}");
            }
            Err(error) => {
                self.status_message = format!("Switch failed: {error}");
            }
        }
    }

    // 统计 staged 分组的文件数。
    fn staged_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.section == FileSection::Staged)
            .count()
    }

    // 统计 unstaged 分组的文件数。
    fn unstaged_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.section == FileSection::Unstaged)
            .count()
    }

    // 统计 untracked 分组的文件数。
    fn untracked_count(&self) -> usize {
        self.files
            .iter()
            .filter(|file| file.section == FileSection::Untracked)
            .count()
    }
}

// 调用 git status --porcelain 获取可解析的状态输出。
fn get_git_status() -> String {
    match Command::new("git").args(["status", "--porcelain"]).output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        Err(_) => String::new(),
    }
}

// 把 git status 输出解析成界面可用的文件列表。
fn parse_git_status(output: &str) -> Vec<GitFile> {
    let mut files = Vec::new();

    for line in output.lines().filter(|line| !line.is_empty()) {
        let (status, rest) = line.split_at(2);
        let path = rest.trim_start().to_string();
        let diff_path = extract_diff_path(&path);
        let status_chars: Vec<char> = status.chars().collect();
        let x = status_chars.first().copied().unwrap_or(' ');
        let y = status_chars.get(1).copied().unwrap_or(' ');

        if x == '?' && y == '?' {
            files.push(GitFile {
                status_code: status.to_string(),
                display_status: String::from("??"),
                path,
                diff_path,
                section: FileSection::Untracked,
            });
            continue;
        }

        if x != ' ' {
            files.push(GitFile {
                status_code: status.to_string(),
                display_status: x.to_string(),
                path: path.clone(),
                diff_path: diff_path.clone(),
                section: FileSection::Staged,
            });
        }

        if y != ' ' {
            files.push(GitFile {
                status_code: status.to_string(),
                display_status: y.to_string(),
                path,
                diff_path,
                section: FileSection::Unstaged,
            });
        }
    }

    files
}

// 处理 rename 场景，取 diff 命令真正要看的目标路径。
fn extract_diff_path(path: &str) -> String {
    path.rsplit(" -> ").next().unwrap_or(path).to_string()
}

// 根据文件所在分组获取右侧要展示的真实 diff 文本。
fn get_diff_lines(file: &GitFile) -> Vec<String> {
    if file.section == FileSection::Untracked {
        return vec![format!(
            "Untracked file: {}\n\nStage it first if you want to inspect a patch here.",
            file.path
        )];
    }

    let mut command = Command::new("git");
    command.args(["diff", "--no-ext-diff", "--no-color"]);

    if file.section == FileSection::Staged {
        command.arg("--cached");
    }

    command.arg("--");
    command.arg(&file.diff_path);

    match command.output() {
        Ok(output) => collect_visible_diff_lines(
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
            &file.path,
            Some(file.section.mode_label()),
        ),
        Err(error) => vec![format!("Failed to run git diff: {error}")],
    }
}

// 读取 Graph 左栏提交列表，包含 graph 前缀、refs、作者、日期和标题。
fn get_graph_commits() -> Vec<GraphCommit> {
    let format = "%x1f%H%x1f%h%x1f%D%x1f%an%x1f%ad%x1f%s";
    let limit = GRAPH_COMMIT_LIMIT.to_string();

    let output = run_git_capture(&[
        "log",
        "--all",
        "--date-order",
        "--decorate=short",
        "--graph",
        "--date=short",
        &format!("--pretty=format:{format}"),
        "-n",
        &limit,
    ])
    .unwrap_or_default();

    output
        .lines()
        .filter_map(parse_graph_commit_line)
        .collect()
}

// 解析 git log --graph 的单行输出。
fn parse_graph_commit_line(line: &str) -> Option<GraphCommit> {
    let separator_index = line.find('\u{1f}')?;
    let graph_prefix = line[..separator_index].to_string();
    let payload = &line[separator_index + '\u{1f}'.len_utf8()..];
    let mut parts = payload.split('\u{1f}');

    let sha = parts.next()?.to_string();
    let short_sha = parts.next()?.to_string();
    let refs = parse_ref_chips(parts.next().unwrap_or_default());
    let author = parts.next().unwrap_or_default().to_string();
    let date = parts.next().unwrap_or_default().to_string();
    let subject = parts.next().unwrap_or_default().to_string();

    Some(GraphCommit {
        graph_prefix,
        sha,
        short_sha,
        subject,
        author,
        date,
        refs,
        body: Vec::new(),
        changed_files: Vec::new(),
        details_loaded: false,
    })
}

// 解析一条 ref 文本，拆成可着色的 chip。
fn parse_ref_chips(value: &str) -> Vec<GraphRefChip> {
    value
        .split(',')
        .flat_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return Vec::new();
            }

            if let Some(branch) = part.strip_prefix("HEAD -> ") {
                return vec![
                    GraphRefChip {
                        label: String::from("HEAD"),
                        kind: GraphRefKind::Head,
                    },
                    GraphRefChip {
                        label: branch.to_string(),
                        kind: GraphRefKind::Branch,
                    },
                ];
            }

            if let Some(tag) = part.strip_prefix("tag: ") {
                return vec![GraphRefChip {
                    label: tag.to_string(),
                    kind: GraphRefKind::Tag,
                }];
            }

            let kind = if part.contains('/') {
                GraphRefKind::Remote
            } else {
                GraphRefKind::Branch
            };

            vec![GraphRefChip {
                label: part.to_string(),
                kind,
            }]
        })
        .collect()
}

// 读取某个 commit 的完整 message 和 changed files。
fn load_commit_details(sha: &str) -> (Vec<String>, Vec<CommitFile>) {
    let body = run_git_capture(&["show", "--format=%B", "--no-patch", sha])
        .map(|output| {
            let lines = output
                .lines()
                .map(|line| line.to_string())
                .collect::<Vec<_>>();
            if lines.is_empty() {
                vec![String::from("No commit message")]
            } else {
                lines
            }
        })
        .unwrap_or_else(|error| vec![format!("Failed to load commit message: {error}")]);

    let files = run_git_capture(&["diff-tree", "--no-commit-id", "--name-status", "-r", "--root", sha])
        .map(|output| {
            output
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(parse_commit_file)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    (body, files)
}

// 解析 commit changed files 的一行 name-status 输出。
fn parse_commit_file(line: &str) -> CommitFile {
    let parts = line.split('\t').collect::<Vec<_>>();
    let status = parts.first().copied().unwrap_or_default().to_string();

    match parts.as_slice() {
        [status, old_path, new_path, ..] if status.starts_with('R') => CommitFile {
            status: (*status).to_string(),
            path: format!("{} -> {}", old_path, new_path),
            diff_path: (*new_path).to_string(),
        },
        [status, path, ..] => CommitFile {
            status: (*status).to_string(),
            path: (*path).to_string(),
            diff_path: (*path).to_string(),
        },
        _ => CommitFile {
            status,
            path: line.to_string(),
            diff_path: line.to_string(),
        },
    }
}

// 读取 Graph 页当前 commit 当前文件的 diff。
fn get_commit_file_diff_lines(sha: &str, file: &CommitFile) -> Vec<String> {
    match Command::new("git")
        .args(["show", "--format=", "--no-ext-diff", "--no-color", sha, "--"])
        .arg(&file.diff_path)
        .output()
    {
        Ok(output) => collect_visible_diff_lines(
            &String::from_utf8_lossy(&output.stdout),
            &String::from_utf8_lossy(&output.stderr),
            &file.path,
            Some(&file.status),
        ),
        Err(error) => vec![format!("Failed to run git show: {error}")],
    }
}

// 汇总并过滤 diff 文本，隐藏不需要的头部行。
fn collect_visible_diff_lines(
    stdout: &str,
    stderr: &str,
    label: &str,
    mode_label: Option<&str>,
) -> Vec<String> {
    let stderr = stderr.trim();

    if !stdout.trim().is_empty() {
        let lines = stdout
            .lines()
            .filter(|line| !should_hide_diff_line(line))
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        if lines.is_empty() {
            vec![match mode_label {
                Some(mode) => format!("No visible diff lines for {label} ({mode})"),
                None => format!("No visible diff lines for {label}"),
            }]
        } else {
            lines
        }
    } else if !stderr.is_empty() {
        vec![stderr.to_string()]
    } else {
        vec![match mode_label {
            Some(mode) => format!("No diff output for {label} ({mode})"),
            None => format!("No diff output for {label}"),
        }]
    }
}

// 判断某一行是否属于 diff 头部元信息；这些行在右侧面板里不展示。
fn should_hide_diff_line(line: &str) -> bool {
    line.starts_with("diff --git ")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
}

// 读取本地分支列表，用于分支弹窗展示。
fn get_local_branches() -> Vec<String> {
    run_git_capture(&["branch", "--format=%(refname:short)"])
        .map(|output| {
            output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| line.to_string())
                .collect()
        })
        .unwrap_or_default()
}

// 读取当前分支的 upstream 文本，用于 push 确认弹窗展示。
fn get_upstream_ref() -> Option<String> {
    run_git_capture(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

// 执行 ff-only pull。
fn pull_ff_only() -> Result<(), String> {
    run_git_command(&["pull", "--ff-only"])
}

// 执行单行 commit。
fn commit_with_message(message: &str) -> Result<(), String> {
    run_git_command(&["commit", "-m", message])
}

// 推送当前分支到其 upstream。
fn push_current_branch() -> Result<(), String> {
    run_git_command(&["push"])
}

// 切换到目标分支，优先使用 git switch。
fn checkout_branch(branch: &str) -> Result<(), String> {
    run_git_command(&["switch", branch]).or_else(|switch_error| {
        run_git_command(&["checkout", branch])
            .map_err(|checkout_error| format!("{switch_error}; fallback failed: {checkout_error}"))
    })
}

// 获取当前分支名，用于界面显示和刷新。
fn get_branch_name() -> String {
    run_git_capture(&["branch", "--show-current"])
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("detached"))
}

// 执行 git 命令并返回 stdout 文本。
fn run_git_capture(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(String::from("git command failed"))
        } else {
            Err(stderr)
        }
    }
}

// 执行不关心 stdout 内容的 git 命令，并尽量返回更完整的错误信息。
fn run_git_command(args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        match (stderr.is_empty(), stdout.is_empty()) {
            (false, false) => Err(format!("{} | {}", stderr, stdout)),
            (false, true) => Err(stderr),
            (true, false) => Err(stdout),
            (true, true) => Err(String::from("git command failed")),
        }
    }
}

// 执行带路径参数的 git 命令，例如 add / restore / rm --cached。
fn run_git_path_command(prefix_args: &[&str], path: &str) -> Result<(), String> {
    let output = Command::new("git")
        .args(prefix_args)
        .arg(path)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(String::from("git command failed"))
        } else {
            Err(stderr)
        }
    }
}

// 取消暂存文件；对新增文件补一个 rm --cached 的兜底分支。
fn unstage_file(file: &GitFile) -> Result<&'static str, String> {
    match run_git_path_command(&["restore", "--staged", "--"], &file.diff_path) {
        Ok(_) => Ok("Unstaged"),
        Err(restore_error) if file.display_status == "A" => {
            run_git_path_command(&["rm", "--cached", "--"], &file.diff_path)
                .map(|_| "Removed from index")
                .map_err(|rm_error| format!("{restore_error}; fallback failed: {rm_error}"))
        }
        Err(error) => Err(error),
    }
}

// 初始化终端，运行应用主循环，最后恢复终端状态。
fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_app(&mut terminal);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result?;
    Ok(())
}

// 事件循环：持续绘制界面并处理用户输入。
fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let mut app = App::new();

    while event::poll(Duration::from_millis(0))? {
        let _ = event::read()?;
    }

    while !app.should_quit {
        terminal.draw(|frame| draw_ui(frame, &app))?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    handle_key_event(&mut app, key);
                }
            }
        }
    }

    Ok(())
}

// 按弹窗优先级分发按键，避免互相串扰。
fn handle_key_event(app: &mut App, key: KeyEvent) {
    if app.commit_input_popup_open {
        handle_commit_input_key_event(app, key);
        return;
    }

    if app.push_confirm_popup_open {
        handle_push_confirm_key_event(app, key);
        return;
    }

    if app.git_action_popup_open {
        handle_git_action_popup_key_event(app, key);
        return;
    }

    if app.branch_popup_open {
        handle_branch_popup_key_event(app, key);
        return;
    }

    match app.active_tab {
        GIT_TAB_INDEX => handle_git_tab_key_event(app, key),
        GRAPH_TAB_INDEX => handle_graph_tab_key_event(app, key),
        _ => {}
    }
}

// 处理 Git 页的按键。
fn handle_git_tab_key_event(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('1') => app.active_tab = GIT_TAB_INDEX,
        KeyCode::Char('2') => app.active_tab = GRAPH_TAB_INDEX,
        KeyCode::Char('g') => app.open_git_action_popup(),
        KeyCode::Char('r') => app.refresh_all(),
        KeyCode::Tab => {
            app.focus = match app.focus {
                FocusPane::Files => FocusPane::Diff,
                FocusPane::Diff => FocusPane::Files,
            };
        }
        KeyCode::Char('j') | KeyCode::Down => match app.focus {
            FocusPane::Files => app.move_selection_down(),
            FocusPane::Diff => app.scroll_diff_down(),
        },
        KeyCode::Char('k') | KeyCode::Up => match app.focus {
            FocusPane::Files => app.move_selection_up(),
            FocusPane::Diff => app.scroll_diff_up(),
        },
        KeyCode::Char('s') => app.stage_selected(),
        KeyCode::Char('u') => app.unstage_selected(),
        _ => {}
    }
}

// 处理 Graph 页的按键。
fn handle_graph_tab_key_event(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('1') => app.active_tab = GIT_TAB_INDEX,
        KeyCode::Char('2') => app.active_tab = GRAPH_TAB_INDEX,
        KeyCode::Char('g') => app.open_git_action_popup(),
        KeyCode::Char('r') => app.refresh_all(),
        KeyCode::Tab => app.graph_focus = app.graph_focus.next(),
        KeyCode::Char('j') | KeyCode::Down => match app.graph_focus {
            GraphFocusPane::Commits => app.move_graph_commit_down(),
            GraphFocusPane::Files => app.move_graph_file_down(),
            GraphFocusPane::Diff => app.scroll_graph_diff_down(),
        },
        KeyCode::Char('k') | KeyCode::Up => match app.graph_focus {
            GraphFocusPane::Commits => app.move_graph_commit_up(),
            GraphFocusPane::Files => app.move_graph_file_up(),
            GraphFocusPane::Diff => app.scroll_graph_diff_up(),
        },
        KeyCode::Enter => match app.graph_focus {
            GraphFocusPane::Commits => app.refresh_selected_graph_details(),
            GraphFocusPane::Files => app.refresh_selected_graph_diff(),
            GraphFocusPane::Diff => {}
        },
        _ => {}
    }
}

// 处理 Git actions popup 打开后的专属按键。
fn handle_git_action_popup_key_event(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.close_git_action_popup(),
        KeyCode::Char('j') | KeyCode::Down => app.move_git_action_down(),
        KeyCode::Char('k') | KeyCode::Up => app.move_git_action_up(),
        KeyCode::Enter => app.run_selected_git_action(),
        _ => {}
    }
}

// 处理分支弹窗打开后的专属按键。
fn handle_branch_popup_key_event(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.close_branch_popup(),
        KeyCode::Char('j') | KeyCode::Down => {
            if !app.branches.is_empty() && app.selected_branch_index + 1 < app.branches.len() {
                app.selected_branch_index += 1;
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if app.selected_branch_index > 0 {
                app.selected_branch_index -= 1;
            }
        }
        KeyCode::Enter => app.switch_selected_branch(),
        _ => {}
    }
}

// 处理 commit 输入框打开后的专属按键。
fn handle_commit_input_key_event(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.close_commit_input_popup(),
        KeyCode::Backspace => app.backspace_commit_input(),
        KeyCode::Enter => app.submit_commit_input(),
        KeyCode::Char(ch) => app.append_commit_input(ch),
        _ => {}
    }
}

// 处理 push 确认弹窗打开后的专属按键。
fn handle_push_confirm_key_event(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => app.close_push_confirm_popup(),
        KeyCode::Enter => app.commit_and_push(),
        _ => {}
    }
}

// 组织整屏布局：标题栏、Tab、主体区、底部状态栏。
fn draw_ui(frame: &mut ratatui::Frame, app: &App) {
    let area = frame.area();
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // [新增] 标题栏高度
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(area);
    // 1. 渲染标题栏 (新增)
    render_title_bar(frame, app, sections[0]);
    render_tabs(frame, app, sections[1]);

    match app.active_tab {
        GIT_TAB_INDEX => render_git_view(frame, app, sections[2]),
        GRAPH_TAB_INDEX => render_graph_view(frame, app, sections[2]),
        _ => {}
    }

    render_status_bar(frame, app, sections[3]);

    if app.git_action_popup_open {
        render_git_action_popup(frame, app);
    }
    if app.branch_popup_open {
        render_branch_popup(frame, app);
    }
    if app.commit_input_popup_open {
        render_commit_input_popup(frame, app);
    }
    if app.push_confirm_popup_open {
        render_push_confirm_popup(frame, app);
    }
}
fn render_title_bar(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let dir_name = std::env::current_dir()
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().to_string()))
        .unwrap_or_else(|| String::from("."));

    let header_text = format!("seen - ~/{} — git: {}", dir_name, app.branch_name);
    let paragraph = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Rgb(90, 99, 116)));

    frame.render_widget(paragraph, area);
}
// 渲染 Git 页两栏布局。
fn render_git_view(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(36), Constraint::Min(20)])
        .split(area);

    render_file_list(frame, app, body[0]);
    render_diff_panel(frame, app, body[1]);
}

// 渲染 Graph 页三栏布局。
fn render_graph_view(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(25),
            Constraint::Percentage(35),
        ])
        .split(area);

    render_graph_commit_list(frame, app, columns[0]);
    render_graph_commit_detail(frame, app, columns[1]);
    render_graph_file_diff(frame, app, columns[2]);
}

// 渲染顶部 Tab 条。
fn render_tabs(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let titles = TAB_TITLES
        .iter()
        .enumerate()
        .map(|(index, title)| Line::from(format!("{} {}", index + 1, title)))
        .collect::<Vec<_>>();

    let tabs = Tabs::new(titles)
        .select(app.active_tab)
        .block(Block::default().borders(Borders::BOTTOM))
        .style(Style::default().fg(Color::Gray))
        .highlight_style(
            Style::default()
                .fg(Color::Rgb(255, 145, 77))
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::raw("  "));

    frame.render_widget(tabs, area);
}

// 渲染左侧文件列表，并按分组拼出所有 ListItem。
fn render_file_list(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let mut items = Vec::new();

    push_section_items(
        &mut items,
        app,
        FileSection::Staged,
        app.staged_count(),
        app.focus == FocusPane::Files,
    );
    push_section_items(
        &mut items,
        app,
        FileSection::Unstaged,
        app.unstaged_count(),
        app.focus == FocusPane::Files,
    );
    push_section_items(
        &mut items,
        app,
        FileSection::Untracked,
        app.untracked_count(),
        app.focus == FocusPane::Files,
    );

    if items.is_empty() {
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "Working tree clean",
            Style::default().fg(Color::DarkGray),
        )])));
    }

    let title = if app.focus == FocusPane::Files {
        "Files [focus]"
    } else {
        "Files"
    };

    let list = List::new(items).block(Block::default().borders(Borders::RIGHT).title(title));
    frame.render_widget(list, area);
}

// 追加某个分组的标题和文件项到左侧列表中。
fn push_section_items(
    items: &mut Vec<ListItem<'static>>,
    app: &App,
    section: FileSection,
    count: usize,
    files_focused: bool,
) {
    let section_files = app.files.iter().enumerate().filter(|(_, file)| file.section == section);

    let has_files = section_files.clone().next().is_some();
    if !has_files {
        return;
    }

    items.push(ListItem::new(Line::from(vec![Span::styled(
        format!("{} ({})", section.title(), count),
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )])));

    for (index, file) in section_files {
        let is_selected = index == app.selected_index;
        let base_style = if is_selected {
            let mut style = Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(64, 39, 27));
            if files_focused {
                style = style.add_modifier(Modifier::BOLD);
            }
            style
        } else {
            Style::default().fg(Color::Rgb(215, 215, 215))
        };

        let status_style = status_style(&file.display_status);
        let line = Line::from(vec![
            Span::styled(" ", base_style),
            Span::styled(format!("{:>2} ", file.display_status), base_style.patch(status_style)),
            Span::styled(file.path.clone(), base_style),
        ]);

        items.push(ListItem::new(line));
    }

    items.push(ListItem::new(Line::from("")));
}

// 渲染 Graph 左栏：提交图、refs 和 subject。
fn render_graph_commit_list(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let title = if app.graph_focus == GraphFocusPane::Commits {
        "Graph [focus]"
    } else {
        "Graph"
    };

    let inner_height = area.height.saturating_sub(2) as usize;
    let visible_capacity = inner_height.max(1);
    let (start, end) = visible_range(app.graph_commits.len(), app.selected_commit_index, visible_capacity);

    let items = if app.graph_commits.is_empty() {
        vec![ListItem::new(Line::from("No commits found"))]
    } else {
        app.graph_commits[start..end]
            .iter()
            .enumerate()
            .map(|(offset, commit)| {
                let index = start + offset;
                let is_selected = index == app.selected_commit_index;
                build_graph_commit_item(commit, is_selected)
            })
            .collect::<Vec<_>>()
    };

    let list = List::new(items).block(Block::default().borders(Borders::RIGHT).title(title));
    frame.render_widget(list, area);
}

// 生成 Graph 左栏一条 commit 行。
fn build_graph_commit_item(commit: &GraphCommit, is_selected: bool) -> ListItem<'static> {
    let row_style = if is_selected {
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(64, 39, 27))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(215, 215, 215))
    };

    let mut spans = graph_prefix_spans(&commit.graph_prefix, is_selected);
    spans.push(Span::styled(
        format!("{} ", commit.short_sha),
        row_style.patch(Style::default().fg(Color::DarkGray)),
    ));
    spans.extend(build_ref_chip_spans(&commit.refs));
    spans.push(Span::styled(commit.subject.clone(), row_style));

    ListItem::new(Line::from(spans))
}

// 把 git log --graph 前缀渲染成更清晰的 lane 轨道。
fn graph_prefix_spans(prefix: &str, is_selected: bool) -> Vec<Span<'static>> {
    prefix
        .chars()
        .map(|ch| {
            let fg = match ch {
                '*' => Color::Rgb(255, 145, 77),
                '|' => Color::Rgb(120, 120, 120),
                '/' | '\\' => Color::Rgb(150, 150, 150),
                '_' => Color::Rgb(100, 100, 100),
                _ => Color::DarkGray,
            };

            let mut style = Style::default().fg(fg);
            if is_selected {
                style = style.bg(Color::Rgb(64, 39, 27));
            }
            if ch == '*' {
                style = style.add_modifier(Modifier::BOLD);
            }
            Span::styled(ch.to_string(), style)
        })
        .collect()
}

// 把 refs 渲染成更像标签的 chip。
fn build_ref_chip_spans(refs: &[GraphRefChip]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    for chip in refs {
        let (fg, bg) = match chip.kind {
            GraphRefKind::Head => (Color::Black, Color::Rgb(255, 145, 77)),
            GraphRefKind::Branch => (Color::Rgb(190, 255, 190), Color::Rgb(24, 60, 32)),
            GraphRefKind::Remote => (Color::Rgb(180, 220, 255), Color::Rgb(24, 44, 72)),
            GraphRefKind::Tag => (Color::Rgb(255, 200, 255), Color::Rgb(68, 28, 72)),
        };

        spans.push(Span::styled(
            format!(" {} ", chip.label),
            Style::default()
                .fg(fg)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }

    spans
}

// 渲染 Graph 中栏：commit metadata、message 和 changed files。
fn render_graph_commit_detail(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(5)])
        .split(area);

    render_graph_commit_meta(frame, app, columns[0]);
    render_graph_commit_files(frame, app, columns[1]);
}

// 渲染 Graph 中栏上半部分的 commit 详情。
fn render_graph_commit_meta(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let lines = if let Some(commit) = app.selected_graph_commit() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    commit.short_sha.clone(),
                    Style::default()
                        .fg(Color::Rgb(255, 145, 77))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(commit.date.clone(), Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![
                Span::styled("Author: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    commit.author.clone(),
                    Style::default().fg(Color::Rgb(220, 220, 220)),
                ),
            ]),
            Line::from(""),
        ];

        if commit.body.is_empty() {
            lines.push(Line::from("Loading commit message..."));
        } else {
            lines.extend(commit.body.iter().map(|line| Line::from(line.clone())));
        }

        lines
    } else {
        vec![Line::from("No commit selected")]
    };

    let paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::RIGHT | Borders::BOTTOM).title("Commit detail"))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

// 渲染 Graph 中栏下半部分的 changed files 列表。
fn render_graph_commit_files(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let title = if app.graph_focus == GraphFocusPane::Files {
        "Changed files [focus]"
    } else {
        "Changed files"
    };

    let inner_height = area.height.saturating_sub(2) as usize;
    let file_count = app
        .selected_graph_commit()
        .map(|commit| commit.changed_files.len())
        .unwrap_or(0);
    let visible_capacity = inner_height.max(1);
    let (start, end) = visible_range(file_count, app.selected_commit_file_index, visible_capacity);

    let items = if let Some(commit) = app.selected_graph_commit() {
        if commit.changed_files.is_empty() {
            vec![ListItem::new(Line::from("No changed files"))]
        } else {
            commit.changed_files[start..end]
                .iter()
                .enumerate()
                .map(|(offset, file)| {
                    let index = start + offset;
                    build_graph_file_item(file, index == app.selected_commit_file_index)
                })
                .collect::<Vec<_>>()
        }
    } else {
        vec![ListItem::new(Line::from("No commit selected"))]
    };

    let list = List::new(items).block(Block::default().borders(Borders::RIGHT).title(title));
    frame.render_widget(list, area);
}

// 生成 Graph 中栏一条 changed file 行。
fn build_graph_file_item(file: &CommitFile, is_selected: bool) -> ListItem<'static> {
    let base_style = if is_selected {
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(64, 39, 27))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(215, 215, 215))
    };

    let display_status = file.status.chars().next().unwrap_or('?').to_string();
    let line = Line::from(vec![
        Span::styled(" ", base_style),
        Span::styled(
            format!("{:>2} ", display_status),
            base_style.patch(status_style(&display_status)),
        ),
        Span::styled(file.path.clone(), base_style),
    ]);

    ListItem::new(line)
}

// 给左侧状态字符设置颜色。
fn status_style(status: &str) -> Style {
    match status {
        "A" | "??" => Style::default().fg(Color::Green),
        "M" => Style::default().fg(Color::Rgb(255, 170, 0)),
        "D" => Style::default().fg(Color::Red),
        "R" => Style::default().fg(Color::Cyan),
        _ => Style::default().fg(Color::Yellow),
    }
}

// 渲染工作区右侧 diff 面板和滚动内容。
fn render_diff_panel(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let title = app.selected_file().map_or_else(
        || String::from("Diff"),
        |file| {
            format!(
                "{} · {} · {}",
                file.path,
                file.section.mode_label(),
                file.status_code
            )
        },
    );

    let lines = app
        .diff_lines
        .iter()
        .map(|line| style_diff_line(line))
        .collect::<Vec<_>>();

    let block = Block::default().title(if app.focus == FocusPane::Diff {
        format!("{} [focus]", title)
    } else {
        title
    });

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.diff_scroll, 0));

    frame.render_widget(paragraph, area);
}

// 渲染 Graph 右栏当前文件 diff。
fn render_graph_file_diff(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let title = if let Some(commit) = app.selected_graph_commit() {
        if let Some(file) = app.selected_graph_file() {
            format!("{} · {}", file.path, commit.short_sha)
        } else {
            format!("{} · no file", commit.short_sha)
        }
    } else {
        String::from("Commit diff")
    };

    let lines = app
        .graph_diff_lines
        .iter()
        .map(|line| style_diff_line(line))
        .collect::<Vec<_>>();

    let block = Block::default().title(if app.graph_focus == GraphFocusPane::Diff {
        format!("{} [focus]", title)
    } else {
        title
    });

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.graph_diff_scroll, 0));

    frame.render_widget(paragraph, area);
}

// 渲染 Git actions popup。
fn render_git_action_popup(frame: &mut ratatui::Frame, app: &App) {
    let popup_area = centered_rect(frame.area(), 34, 28);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Git actions")
        .style(Style::default().bg(Color::Rgb(18, 18, 18)));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let items = GIT_ACTIONS
        .iter()
        .enumerate()
        .map(|(index, action)| {
            let is_selected = index == app.selected_git_action_index;
            let style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Rgb(64, 39, 27))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(215, 215, 215))
            };

            ListItem::new(Line::from(vec![
                Span::styled(" ", style),
                Span::styled(action.label().to_string(), style),
            ]))
        })
        .collect::<Vec<_>>();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

// 渲染分支选择弹窗，覆盖在主界面之上。
fn render_branch_popup(frame: &mut ratatui::Frame, app: &App) {
    let popup_area = centered_rect(frame.area(), 60, 55);
    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Branches")
        .style(Style::default().bg(Color::Rgb(18, 18, 18)));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let popup_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);

    let visible_capacity = popup_chunks[0].height.max(1) as usize;
    let (start, end) = visible_range(app.branches.len(), app.selected_branch_index, visible_capacity);

    let items = if app.branches.is_empty() {
        vec![ListItem::new(Line::from("No branches found"))]
    } else {
        app.branches[start..end]
            .iter()
            .enumerate()
            .map(|(offset, branch)| {
                let index = start + offset;
                let is_current = branch == &app.branch_name;
                let is_selected = index == app.selected_branch_index;
                let mut style = if is_selected {
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(64, 39, 27))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(215, 215, 215))
                };

                if is_current && !is_selected {
                    style = style.add_modifier(Modifier::BOLD);
                }

                let marker_style = if is_selected {
                    Style::default()
                        .fg(Color::Rgb(255, 145, 77))
                        .bg(Color::Rgb(64, 39, 27))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(255, 145, 77))
                };

                let prefix = if is_current { "*" } else { " " };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", prefix), marker_style),
                    Span::styled(branch.clone(), style),
                ]))
            })
            .collect::<Vec<_>>()
    };

    let list = List::new(items);
    let help = Paragraph::new("Enter switch  Esc close")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(list, popup_chunks[0]);
    frame.render_widget(help, popup_chunks[1]);
}

// 渲染 commit 输入弹窗。
fn render_commit_input_popup(frame: &mut ratatui::Frame, app: &App) {
    let popup_area = centered_rect(frame.area(), 56, 18);
    frame.render_widget(Clear, popup_area);

    let input_text = if app.commit_input.is_empty() {
        String::from(" ")
    } else {
        app.commit_input.clone()
    };

    let paragraph = Paragraph::new(Text::from(vec![
        Line::from("Enter a single-line commit message:"),
        Line::from(""),
        Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Rgb(255, 145, 77)).add_modifier(Modifier::BOLD)),
            Span::styled(input_text, Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from("Enter continue  Esc cancel  Backspace delete"),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Commit message")
            .style(Style::default().bg(Color::Rgb(18, 18, 18))),
    )
    .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, popup_area);
}

// 渲染 push 确认弹窗。
fn render_push_confirm_popup(frame: &mut ratatui::Frame, app: &App) {
    let popup_area = centered_rect(frame.area(), 52, 28);
    frame.render_widget(Clear, popup_area);

    let upstream = get_upstream_ref().unwrap_or_else(|| String::from("<no upstream>"));
    let lines = vec![
        Line::from(vec![
            Span::styled("Branch: ", Style::default().fg(Color::DarkGray)),
            Span::styled(app.branch_name.clone(), Style::default().fg(Color::Rgb(220, 220, 220))),
        ]),
        Line::from(vec![
            Span::styled("Upstream: ", Style::default().fg(Color::DarkGray)),
            Span::styled(upstream, Style::default().fg(Color::Rgb(220, 220, 220))),
        ]),
        Line::from(vec![
            Span::styled("Message: ", Style::default().fg(Color::DarkGray)),
            Span::styled(app.commit_input.clone(), Style::default().fg(Color::Rgb(220, 220, 220))),
        ]),
        Line::from(""),
        Line::from("This will create a commit and then push to the upstream."),
        Line::from("Press Enter to confirm, Esc to cancel."),
    ];

    let paragraph = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Confirm commit and push")
                .style(Style::default().bg(Color::Rgb(18, 18, 18))),
        )
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, popup_area);
}

// 给 diff 每一行按类型着色。
fn style_diff_line(line: &str) -> Line<'static> {
    let style = if line.starts_with('+') {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") {
        Style::default().fg(Color::Blue)
    } else if line.starts_with("new file mode") || line.starts_with("deleted file mode") {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Rgb(220, 220, 220))
    };

    Line::from(Span::styled(line.to_string(), style))
}

// 计算一个列表的可见窗口，尽量保留少量前后上下文，滚动更平滑。
fn visible_range(total: usize, selected: usize, capacity: usize) -> (usize, usize) {
    if total == 0 || capacity == 0 || total <= capacity {
        return (0, total);
    }

    let padding = capacity.min(3) / 2;
    let mut start = selected.saturating_sub(padding);
    let mut end = start + capacity;

    if selected >= end.saturating_sub(padding) {
        end = (selected + padding + 1).min(total);
        start = end.saturating_sub(capacity);
    }

    if end > total {
        end = total;
        start = end.saturating_sub(capacity);
    }

    (start, end)
}

// 计算居中弹窗区域。
fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);

    horizontal[1]
}

// 超长文本截断，避免底部状态栏内容互相覆盖。
fn truncate_text(value: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let char_count = value.chars().count();
    if char_count <= max_width {
        return value.to_string();
    }

    if max_width == 1 {
        return String::from("…");
    }

    let mut truncated = value.chars().take(max_width - 1).collect::<String>();
    truncated.push('…');
    truncated
}

// 渲染底部快捷键和右侧统计信息。
fn render_status_bar(frame: &mut ratatui::Frame, app: &App, area: Rect) {
    let shortcuts = if app.commit_input_popup_open {
        "type msg  Enter ok  Esc cancel"
    } else if app.push_confirm_popup_open {
        "Enter confirm  Esc cancel"
    } else if app.git_action_popup_open {
        "j/k action  Enter run  Esc close"
    } else if app.branch_popup_open {
        "j/k branch  Enter switch  Esc close"
    } else if app.active_tab == GIT_TAB_INDEX {
        "g actions  r refresh  s stage  u unstage  Tab focus  j/k nav  1/2 tabs  q quit"
    } else {
        "g actions  r refresh  Tab focus  j/k nav  Enter reload  1/2 tabs  q quit"
    };

    let counts = format!(
        "{} staged · {} modified · {} untracked",
        app.staged_count(),
        app.unstaged_count(),
        app.untracked_count()
    );

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(counts.chars().count() as u16 + 1)])
        .split(area);

    let left_width = chunks[0].width as usize;
    let left_text = if app.status_message.is_empty() {
        truncate_text(shortcuts, left_width)
    } else {
        truncate_text(&format!("{}   {}", shortcuts, app.status_message), left_width)
    };

    let left = Paragraph::new(left_text).style(Style::default().fg(Color::DarkGray));
    let right = Paragraph::new(counts).style(
        Style::default()
            .fg(Color::Rgb(255, 145, 77))
            .add_modifier(Modifier::BOLD),
    );

    frame.render_widget(left, chunks[0]);
    frame.render_widget(right, chunks[1]);
}
