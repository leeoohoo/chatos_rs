using ChatOS.Core.Domain;
using ChatOS.Core.State;
using CommunityToolkit.Mvvm.ComponentModel;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Settings;

public sealed class LocalizationViewModel : ObservableObject
{
    private readonly AppPreferencesManager _preferences;
    private readonly IUiDispatcher _dispatcher;

    public LocalizationViewModel(AppPreferencesManager preferences, IUiDispatcher dispatcher)
    {
        _preferences = preferences;
        _dispatcher = dispatcher;
        _preferences.Changed += OnPreferencesChanged;
    }

    public InterfaceLanguage Language => _preferences.Current.Language;

    public string SignIn => Text("登录", "Sign in");
    public string SignInTitle => Text("登录 ChatOS", "Sign in to ChatOS");
    public string SignInDescription => Text(
        "使用你的 ChatOS 账号连接工作区。访问令牌只保存在 Windows 凭据管理器。",
        "Connect to your workspace with your ChatOS account. Access tokens are stored only in Windows Credential Manager.");
    public string Username => Text("用户名", "Username");
    public string UsernamePlaceholder => Text("请输入用户名", "Enter your username");
    public string Password => Text("密码", "Password");
    public string PasswordPlaceholder => Text("请输入密码", "Enter your password");
    public string Contacts => Text("联系人", "Contacts");
    public string Projects => Text("项目", "Projects");
    public string Local => Text("本机", "Local");
    public string Remote => Text("远端", "Remote");
    public string Chat => Text("聊天", "Chat");
    public string Files => Text("文件", "Files");
    public string Git => "Git";
    public string Plan => Text("计划", "Plan");
    public string Run => Text("运行", "Run");
    public string Notepad => Text("记事本", "Notepad");
    public string Artifacts => Text("插件产物", "Plugin artifacts");
    public string Artifact => Text("产物", "Artifacts");
    public string Open => Text("打开", "Open");
    public string SaveAs => Text("另存为", "Save as");
    public string Refresh => Text("刷新", "Refresh");
    public string AllPluginArtifacts => Text(
        "所有当前插件会话产生的文件",
        "Files produced by all current plugin sessions");
    public string CurrentVisualSessionArtifacts => Text(
        "当前视觉会话产生的文件",
        "Files produced by the current visual session");
    public string NoPluginArtifacts => Text(
        "暂时没有可打开的插件产物",
        "No plugin artifacts are available yet");
    public string ProcessingFile => Text("正在处理文件…", "Processing file…");
    public string AiExecutionView => Text("AI 执行画面", "AI execution view");
    public string CloseCurrentView => Text("关闭当前画面", "Close current view");
    public string WaitingForPluginFrame => Text("正在等待插件画面…", "Waiting for the plugin frame…");
    public string Settings => Text("设置", "Settings");
    public string Back => Text("返回", "Back");
    public string SignOut => Text("退出登录", "Sign out");
    public string RefreshResources => Text("刷新资源", "Refresh resources");
    public string CreateResource => Text("新建资源", "Create resource");
    public string CreateLocalTerminal => Text("新建本机终端", "New local terminal");
    public string Terminal => Text("终端", "Terminal");
    public string StopTerminal => Text("停止终端", "Stop terminal");
    public string TerminalCommandPlaceholder => Text("输入命令后按 Enter", "Enter a command and press Enter");
    public string Send => Text("发送", "Send");
    public string Cancel => Text("取消", "Cancel");
    public string Create => Text("创建", "Create");
    public string New => Text("新建", "New");
    public string Delete => Text("删除", "Delete");
    public string Rename => Text("重命名", "Rename");
    public string Select => Text("选择", "Select");
    public string Execute => Text("执行", "Run");
    public string Status => Text("状态", "Status");
    public string Total => Text("总计", "Total");
    public string Tasks => Text("任务", "Tasks");
    public string Documents => Text("文档", "Documents");
    public string AcceptanceCriteria => Text("验收标准", "Acceptance criteria");
    public string ExecutionPlan => Text("执行计划", "Execution plan");
    public string ProjectPlan => Text("项目计划", "Project plan");
    public string RefreshPlan => Text("刷新计划", "Refresh plan");
    public string ConfirmExecution => Text("确认执行", "Confirm execution");
    public string PlanningFeedbackPlaceholder => Text("可选：给规划过程补充要求", "Optional: add requirements for planning");
    public string IncludePrerequisiteDependents => Text("同时包含依赖项", "Include prerequisite dependencies");
    public string CreateExecutionPlan => Text("生成执行计划", "Create execution plan");
    public string RemoteConnections => Text("远端连接", "Remote connections");
    public string RemoteCredentialNotice => Text(
        "SSH 凭据只保存在本机 Credential Manager",
        "SSH credentials are stored only in Windows Credential Manager");
    public string SftpFiles => Text("SFTP 文件", "SFTP files");
    public string RemoteTerminal => Text("远程终端", "Remote terminal");
    public string ConnectionInformation => Text("连接信息", "Connection details");
    public string Name => Text("名称", "Name");
    public string LocalWorkspace => Text("本机工作区", "Local workspace");
    public string Host => Text("主机", "Host");
    public string Port => Text("端口", "Port");
    public string AuthenticationMethod => Text("认证方式", "Authentication");
    public string PrivateKey => Text("私钥", "Private key");
    public string PrivateKeyAndCertificate => Text("私钥 + 证书", "Private key + certificate");
    public string LocalOnlyPassword => Text("密码（仅本机）", "Password (local only)");
    public string PrivateKeyPath => Text("私钥路径", "Private key path");
    public string CertificatePath => Text("证书路径", "Certificate path");
    public string DefaultRemoteDirectory => Text("默认远端目录", "Default remote directory");
    public string HostKeyPolicy => Text("主机密钥策略", "Host key policy");
    public string StrictVerification => Text("严格校验", "Strict verification");
    public string AcceptFirstUse => Text("首次接受", "Accept first use");
    public string JumpHost => Text("跳板机", "Jump host");
    public string ConnectThroughJumpHost => Text("通过跳板机连接", "Connect through a jump host");
    public string ReuseSavedConnection => Text("复用已保存连接（可选）", "Reuse a saved connection (optional)");
    public string JumpHostAddress => Text("跳板机主机", "Jump host");
    public string JumpHostLocalOnlyPassword => Text("跳板机密码（仅本机）", "Jump host password (local only)");
    public string JumpHostPrivateKey => Text("跳板机私钥", "Jump host private key");
    public string JumpHostCertificate => Text("跳板机证书", "Jump host certificate");
    public string VerificationCodePlaceholder => Text("输入二次验证码", "Enter verification code");
    public string TestConnection => Text("测试连接", "Test connection");
    public string Save => Text("保存", "Save");
    public string ParentDirectory => Text("上一级", "Parent");
    public string NewDirectory => Text("新建目录", "New directory");
    public string Upload => Text("上传", "Upload");
    public string FilePreview => Text("文件预览", "File preview");
    public string SshVerificationRefreshPlaceholder => Text("输入 SSH 二次验证码后刷新", "Enter the SSH verification code, then refresh");
    public string RemoteVerificationRetryPlaceholder => Text("输入二次验证码后重新执行", "Enter the verification code, then run again");
    public string RemoteCommandPlaceholder => Text("输入远端命令，Enter 执行", "Enter a remote command and press Enter");
    public string RefreshDirectory => Text("刷新目录", "Refresh directory");
    public string NewFileOrFolder => Text("新建文件或文件夹", "New file or folder");
    public string NewFile => Text("新建文件", "New file");
    public string NewFolder => Text("新建文件夹", "New folder");
    public string SearchFilesAndContent => Text("搜索文件名和内容", "Search file names and content");
    public string Search => Text("搜索", "Search");
    public string SearchResults => Text("搜索结果", "Search results");
    public string Edit => Text("编辑", "Edit");
    public string Line => Text("行", "Line");
    public string Column => Text("列", "Column");
    public string FindDefinition => Text("查定义", "Go to definition");
    public string FindReferences => Text("查引用", "Find references");
    public string BinaryPreviewOnly => Text("二进制文件仅支持预览元数据", "Binary files support metadata preview only");
    public string BinaryEditorSafetyNotice => Text(
        "为了避免损坏文件，Windows 客户端不会把二进制内容放进文本编辑器。",
        "To avoid file corruption, the Windows client does not load binary content into the text editor.");
    public string SelectFileToView => Text("选择文件查看内容", "Select a file to view its contents");
    public string ReadOnlyPreviewNotice => Text("默认以只读预览打开，需要修改时再点击“编辑”。", "Files open in read-only preview by default. Choose Edit when changes are needed.");
    public string Pull => Text("拉取", "Pull");
    public string Push => Text("推送", "Push");
    public string RefreshGitStatus => Text("刷新 Git 状态", "Refresh Git status");
    public string CancelGitOperation => Text("取消当前 Git 操作", "Cancel current Git operation");
    public string NotGitRepository => Text("这个项目还不是 Git 仓库", "This project is not a Git repository yet");
    public string GitInitializationNotice => Text(
        "初始化只会创建 .git 元数据，不会删除或覆盖项目文件。默认分支名称为 main。",
        "Initialization only creates .git metadata and will not delete or overwrite project files. The default branch is main.");
    public string InitializeGitRepository => Text("初始化 Git 仓库", "Initialize Git repository");
    public string Changes => Text("修改", "Changes");
    public string StageAll => Text("全部暂存", "Stage all");
    public string UnstageAll => Text("全部取消暂存", "Unstage all");
    public string Staged => Text("暂存", "Staged");
    public string WorkingTree => Text("工作区", "Working tree");
    public string WorkingTreeDiff => Text("工作区 Diff", "Working tree diff");
    public string StagedDiff => Text("暂存 Diff", "Staged diff");
    public string Stage => Text("暂存", "Stage");
    public string Unstage => Text("取消暂存", "Unstage");
    public string NoWorkingTreeChanges => Text("工作区没有修改", "No working tree changes");
    public string CreateCommit => Text("创建提交", "Create commit");
    public string CommitMessagePlaceholder => Text("说明这次修改解决了什么", "Describe what this change accomplishes");
    public string CommitStagedChanges => Text("提交暂存修改", "Commit staged changes");
    public string DiffReadOnlyNotice => Text("Diff 只读预览；暂存和工作区内容分别查看。", "Diff is read-only; staged and working-tree changes are shown separately.");
    public string SelectChangeToViewDiff => Text("选择修改查看 Diff", "Select a change to view its diff");
    public string SplitDiffNotice => Text("同一个文件可能同时有暂存修改和工作区修改。", "A file may have both staged and working-tree changes.");
    public string NoTextDiff => Text("这部分修改没有文本 Diff。", "This change has no text diff.");
    public string Branches => Text("分支", "Branches");
    public string Switch => Text("切换", "Switch");
    public string Merge => Text("合并", "Merge");
    public string Remotes => Text("远程仓库", "Remotes");
    public string Add => Text("添加", "Add");
    public string EditRemote => Text("编辑远程仓库", "Edit remote");
    public string RemoveRemote => Text("移除远程仓库", "Remove remote");
    public string FastForwardOnlyNotice => Text("拉取只允许 fast-forward，不会自动合并或覆盖本地修改。", "Pull only allows fast-forward and will not merge or overwrite local changes automatically.");
    public string RecentCommits => Text("最近提交", "Recent commits");
    public string ProjectRun => Text("项目运行", "Project run");
    public string TargetsSuffix => Text("个目标", "targets");
    public string Reanalyze => Text("重新分析", "Analyze again");
    public string RefreshRunStatus => Text("刷新运行状态", "Refresh run status");
    public string FavoriteProject => Text("设为常用项目", "Favorite project");
    public string FavoriteProjectDescription => Text("开启后，点击桌面宠物即可查看这个项目的最近消息并直接发送新消息。", "When enabled, select the desktop pet to view recent messages and send new messages for this project.");
    public string RunPreflight => Text("运行预检", "Run preflight");
    public string NoBlockingRunIssues => Text("没有发现阻塞问题，可以启动当前目标。", "No blocking issues were found. The selected target can start.");
    public string RunTargets => Text("运行目标", "Run targets");
    public string NoRunTargets => Text("没有识别到运行目标。点击“重新分析”扫描项目入口和清单文件。", "No run targets were detected. Choose Analyze again to scan project entry points and manifest files.");
    public string StartNewInstance => Text("启动新实例", "Start new instance");
    public string RunInstances => Text("运行实例", "Run instances");
    public string RunInstanceDescription => Text("每次启动都会创建独立进程。运行中的实例可直接停止，结束后的实例仍保留日志供查看。", "Each start creates an independent process. Running instances can be stopped, and completed instances retain their logs.");
    public string NoRunInstances => Text("还没有运行实例。", "No run instances yet.");
    public string Stop => Text("停止", "Stop");
    public string RequiredEnvironment => Text("启动所需环境", "Required environment");
    public string NoManualToolchain => Text("没有需要手动选择的工具链，当前目标可使用系统自动发现的环境。", "No toolchain requires manual selection; the selected target can use the automatically detected environment.");
    public string EnvironmentVariables => Text("环境变量", "Environment variables");
    public string AddVariable => Text("添加变量", "Add variable");
    public string NoEnvironmentVariables => Text("当前没有自定义环境变量。", "No custom environment variables.");
    public string EnvironmentVariableNamePlaceholder => Text("变量名，例如 PORT", "Variable name, for example PORT");
    public string Value => Text("值", "Value");
    public string DeleteEnvironmentVariable => Text("删除环境变量", "Delete environment variable");
    public string SaveEnvironment => Text("保存环境配置", "Save environment");
    public string ProjectConfigurationFiles => Text("项目配置文件", "Project configuration files");
    public string LocalApprovalRequired => Text("需要本机审批", "Local approval required");
    public string LocalOperationAwaitingApproval => Text("本机操作等待审批", "Local operation awaiting approval");
    public string Deny => Text("拒绝", "Deny");
    public string AllowOnce => Text("本次允许", "Allow once");
    public string AllowForSession => Text("本会话允许", "Allow for session");
    public string LowRisk => Text("低风险", "Low risk");
    public string MediumRisk => Text("需注意", "Caution");
    public string HighRisk => Text("高风险", "High risk");
    public string DefaultApprovalReason => Text("请确认是否允许执行此操作。", "Confirm whether this operation is allowed.");
    public string ApprovalContinuesTask => Text("处理后会立即继续或终止对应任务", "The related task will continue or stop immediately after this decision");
    public string QueuedApprovals(int count) => Text(
        $"当前还有 {count} 个审批排队等待处理",
        $"{count} more approval{(count == 1 ? string.Empty : "s")} waiting in the queue");
    public string RunningGuidanceAvailable => Text("执行中 · 可以继续发送引导", "Running · you can continue sending guidance");
    public string RefreshConversation => Text("刷新会话", "Refresh conversation");
    public string AiExecutionProcess => Text("AI 执行过程", "AI execution process");
    public string StartNewConversation => Text("开始一段新对话", "Start a new conversation");
    public string NoConversationMessages => Text("这个会话还没有消息。", "This conversation has no messages yet.");
    public string MessageComposerPlaceholder => Text("发送消息；执行中会作为追加指导", "Send a message; while running, it will be appended as guidance");
    public string AddAttachment => Text("添加附件", "Add attachment");
    public string RemoveAttachment => Text("移除附件", "Remove attachment");
    public string NoAvailableConversation => Text("这个资源还没有可用会话", "This resource has no available conversation");
    public string ConversationPreparationNotice => Text("项目会自动准备默认会话；联系人需要已有会话。", "Projects prepare a default conversation automatically; contacts require an existing conversation.");
    public string TaskDetails => Text("任务详情", "Task details");
    public string AuthoritativeTaskStatus => Text("状态与运行过程来自服务端权威数据", "Status and run events come from authoritative server data");
    public string RunResult => Text("运行结果", "Run result");
    public string ExecutionProcess => Text("执行过程", "Execution process");
    public string LoadEarlierProcess => Text("加载更早过程", "Load earlier events");
    public string TaskActions => Text("任务操作", "Task actions");
    public string OptionalCancelReason => Text("取消原因（可选）", "Cancellation reason (optional)");
    public string CancelTask => Text("取消任务", "Cancel task");
    public string OptionalRetryInstruction => Text("重试补充说明（可选）", "Retry instructions (optional)");
    public string RetryRun => Text("重试运行", "Retry run");
    public string OperationAllowed => Text("操作已允许", "Operation allowed");
    public string OperationDenied => Text("操作已拒绝", "Operation denied");
    public string AiApproval => Text("AI 审批", "AI approval");
    public string SessionAuthorization => Text("本会话授权", "Session authorization");
    public string ApprovalPolicy => Text("审批策略", "Approval policy");
    public string ExecutionContinued => Text("已继续执行", "Execution continued");
    public string ExecutionStopped => Text("已停止执行", "Execution stopped");
    public string AdditionalQueuedApprovals(int count) => Text(
        $"另有 {count} 个审批正在等待",
        $"{count} additional approval{(count == 1 ? string.Empty : "s")} waiting");
    public string ApprovalTaskOutcome => Text("处理后对应任务会立即继续或终止", "The related task will continue or stop immediately after the decision");
    public string ViewTaskDetails => Text("查看任务详情", "View task details");
    public string Replying => Text("正在回复…", "Replying…");
    public string Remove => Text("移除", "Remove");
    public string CloseNotepad => Text("关闭记事本", "Close notepad");
    public string SearchNotes => Text("搜索标题、标签和内容", "Search titles, tags, and content");
    public string AllNotes => Text("全部笔记", "All notes");
    public string Notes => Text("笔记", "Notes");
    public string NewNote => Text("新建笔记", "New note");
    public string Title => Text("标题", "Title");
    public string Preview => Text("预览", "Preview");
    public string Split => Text("分栏", "Split");
    public string Export => Text("导出", "Export");
    public string Tags => Text("标签", "Tags");
    public string CommaSeparated => Text("用逗号分隔", "Separate with commas");
    public string SelectOrCreateNote => Text("选择或新建一篇笔记", "Select or create a note");
    public string Confirm => Text("确认", "Confirm");
    public string AccountAndSettings => Text("账号和设置", "Account and settings");
    public string General => Text("常规", "General");
    public string Connection => Text("连接", "Connection");
    public string Models => Text("模型", "Models");
    public string SyncModels => Text("同步模型", "Sync models");
    public string SaveSettings => Text("保存设置", "Save settings");
    public string LocalApprovalModel => Text("本机自动审批模型", "Local approval model");
    public string ModelRequestRetries => Text("模型请求最大重试次数", "Maximum model request retries");
    public string NoModelSelected => Text("未选择，自动审批回退用户", "Not selected; automatic approval asks the user");
    public string ClearSelection => Text("清除选择", "Clear selection");
    public string Plugins => Text("插件", "Plugins");
    public string Sandbox => Text("沙箱", "Sandbox");
    public string Approval => Text("审批", "Approval");
    public string ApprovalMode => Text("审批模式", "Approval mode");
    public string AskEveryTime => Text("需要确认", "Ask every time");
    public string AutomaticApproval => Text("自动审批", "Automatic approval");
    public string FullControl => Text("完全控制", "Full control");
    public string PendingApprovals => Text("等待处理", "Pending approvals");
    public string ApprovalHistory => Text("审批记录", "Approval history");
    public string Pet => Text("宠物", "Pet");
    public string LanguageLabel => Text("界面语言", "Interface language");
    public string ThemeLabel => Text("外观", "Appearance");
    public string FontSizeLabel => Text("字体大小", "Font size");
    public string PetEnabledLabel => Text("显示全局宠物", "Show global pet");
    public string SystemTheme => Text("跟随系统", "System");
    public string LightTheme => Text("浅色", "Light");
    public string DarkTheme => Text("深色", "Dark");
    public string SimplifiedChinese => "简体中文";
    public string English => "English";
    public string SettingsDescription => Text(
        "这些设置会立即生效并保存在本机。",
        "These settings take effect immediately and are stored on this device.");

    public string Text(string chinese, string english) =>
        Language == InterfaceLanguage.English ? english : chinese;

    private async void OnPreferencesChanged(object? sender, AppPreferences preferences)
    {
        await _dispatcher.InvokeAsync(() => OnPropertyChanged(string.Empty));
    }
}
