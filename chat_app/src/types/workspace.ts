export interface Project {
  id: string;
  name: string;
  rootPath: string;
  description?: string | null;
  userId?: string | null;
  createdAt: Date;
  updatedAt: Date;
}

export interface ProjectRunTarget {
  id: string;
  label: string;
  kind: string;
  language?: string | null;
  cwd: string;
  command: string;
  source: string;
  confidence: number;
  isDefault?: boolean;
  entrypoint?: string | null;
  manifestPath?: string | null;
  requiredToolchains: string[];
}

export interface ProjectRunCatalog {
  projectId: string;
  status: 'analyzing' | 'ready' | 'empty' | 'error' | string;
  defaultTargetId?: string | null;
  targets: ProjectRunTarget[];
  errorMessage?: string | null;
  analyzedAt?: string | null;
  updatedAt?: string | null;
}

export interface ProjectRunToolchainOption {
  id: string;
  kind: string;
  label: string;
  version?: string | null;
  path: string;
  source: string;
  isDefault?: boolean;
}

export interface ProjectRunConfigFileSummary {
  kind: string;
  label: string;
  path: string;
  preview?: string | null;
  source: string;
}

export interface ProjectRunValidationIssue {
  kind: string;
  message: string;
  targetId?: string | null;
  targetLabel?: string | null;
  path?: string | null;
  hint?: string | null;
}

export interface ProjectRunCustomToolchain {
  kind: string;
  label: string;
  path: string;
}

export interface ProjectRunEnvironment {
  projectId: string;
  userId?: string | null;
  optionsByKind: Record<string, ProjectRunToolchainOption[]>;
  configFiles: ProjectRunConfigFileSummary[];
  validationIssues: ProjectRunValidationIssue[];
  selectedToolchains: Record<string, string>;
  customToolchains: Record<string, ProjectRunCustomToolchain>;
  envVars: Record<string, string>;
  updatedAt?: string | null;
}

export interface ProjectRunResolutionSuggestion {
  id: string;
  label: string;
  detail?: string | null;
  actionKind: 'select_toolchain' | 'switch_target';
  toolchainKind?: string | null;
  optionId?: string | null;
  targetId?: string | null;
}

export interface ProjectRunState {
  projectId: string;
  running: boolean;
  busy: boolean;
  status: string;
  terminalId?: string | null;
  terminalName?: string | null;
  cwd?: string | null;
  terminal: Terminal | null;
  instances?: ProjectRunInstance[];
}

export interface ProjectRunInstance {
  terminalId: string;
  terminalName: string;
  cwd: string;
  status: string;
  busy: boolean;
  running: boolean;
  terminal: Terminal | null;
}

export interface Terminal {
  id: string;
  name: string;
  cwd: string;
  kind?: string | null;
  userId?: string | null;
  projectId?: string | null;
  status: string;
  busy?: boolean;
  createdAt: Date;
  updatedAt: Date;
  lastActiveAt: Date;
}

export interface TerminalLog {
  id: string;
  terminalId: string;
  logType: string;
  content: string;
  createdAt: Date | string;
}

export interface RemoteConnection {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: 'private_key' | 'private_key_cert' | 'password';
  password?: string | null;
  privateKeyPath?: string | null;
  certificatePath?: string | null;
  defaultRemotePath?: string | null;
  hostKeyPolicy: 'strict' | 'accept_new';
  jumpEnabled: boolean;
  jumpConnectionId?: string | null;
  jumpHost?: string | null;
  jumpPort?: number | null;
  jumpUsername?: string | null;
  jumpPrivateKeyPath?: string | null;
  jumpCertificatePath?: string | null;
  jumpPassword?: string | null;
  userId?: string | null;
  createdAt: Date;
  updatedAt: Date;
  lastActiveAt: Date;
}

export interface ContactRecord {
  id: string;
  agentId: string;
  name: string;
  status: string;
  createdAt: Date;
  updatedAt: Date;
}

export interface FsEntry {
  name: string;
  path: string;
  isDir: boolean;
  writable?: boolean | null;
  size?: number | null;
  modifiedAt?: string | null;
}

export interface FsReadResult {
  path: string;
  name: string;
  size: number;
  contentType: string;
  isBinary: boolean;
  writable?: boolean | null;
  modifiedAt?: string | null;
  content: string;
}

export interface ProjectSearchHit {
  path: string;
  relativePath: string;
  line: number;
  column: number;
  text: string;
}

export interface CodeNavCapabilities {
  language: string;
  provider: string;
  supportsDefinition: boolean;
  supportsReferences: boolean;
  supportsDocumentSymbols: boolean;
  fallbackAvailable: boolean;
}

export interface CodeNavLocation {
  path: string;
  relativePath: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  preview: string;
  score: number;
}

export interface CodeNavLocationsResult {
  provider: string;
  language: string;
  mode: string;
  token?: string | null;
  locations: CodeNavLocation[];
}

export interface CodeNavDocumentSymbol {
  name: string;
  kind: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
}

export interface CodeNavDocumentSymbolsResult {
  provider: string;
  language: string;
  mode: string;
  symbols: CodeNavDocumentSymbol[];
}
