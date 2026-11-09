/** Public Workspace Service facts. Connection settings and roots stay with the host. */
export type WorkspaceMethod =
  | "capabilities"
  | "filesList"
  | "filesSearch"
  | "filesRead"
  | "filesWrite"
  | "gitRepository"
  | "gitStatus"
  | "gitDiff"
  | "gitBranches"
  | "gitLog"
  | "gitChangesNavigator"
  | "gitChangeBranch"
  | "gitCommit"
  | "gitPush"
  | "turnChanges";
export interface WorkspaceCapabilities {
  version: 1;
  methods: WorkspaceMethod[];
  mutations: boolean;
  allowedGitOperations?: string[] | null;
  turnChanges: boolean;
}
export interface WorkspaceFileEntry {
  name: string;
  path: string;
  kind: string;
  hidden: boolean;
}
export interface WorkspaceListing {
  workspaceId: string;
  path: string;
  entries: WorkspaceFileEntry[];
  truncated: boolean;
}
export interface WorkspaceFile {
  workspaceId: string;
  path: string;
  encoding: "base64";
  content: string;
  size: number;
  truncated: boolean;
  revision: string | null;
}
export interface WorkspaceGitChange {
  path: string;
  originalPath?: string | null;
  kind: string;
  indexStatus: string;
  workTreeStatus: string;
  isConflict: boolean;
}
export interface WorkspaceGitStatus {
  repositoryRoot: string;
  branch: string | null;
  head: string | null;
  detached: boolean;
  upstream: string | null;
  ahead: number;
  behind: number;
  remotes: string[];
  changes: WorkspaceGitChange[];
  lastCommitShort?: string | null;
  lastCommitSubject?: string | null;
  contractVersion: "1";
}
export interface WorkspaceGitBranch {
  name: string;
  isCurrent: boolean;
  head: string;
  upstream: string | null;
  isMerged: boolean | null;
  aheadOfUpstream: number | null;
  behindUpstream: number | null;
  isUpstreamGone: boolean;
  worktreePath: string | null;
}
export interface WorkspaceGitBranches {
  branches: WorkspaceGitBranch[];
  defaultBranch: string | null;
  contractVersion: "1";
}
export interface WorkspaceGitDiff {
  repositoryRoot: string;
  unifiedDiff: string;
  isBinary: boolean;
  truncated: boolean;
  contractVersion: "1";
}
export interface WorkspaceGitLog {
  head: string | null;
  commits: { id: string; author: string; date: string; subject: string }[];
  nextOffset: number | null;
}
export interface WorkspaceGitChangesFile {
  path: string;
  originalPath: string | null;
  status: string;
  additions: number | null;
  deletions: number | null;
  isBinary: boolean | null;
  isConflict: boolean;
}
export interface WorkspaceGitNavigator {
  repositories: {
    repositoryRoot: string;
    /** Host-authoritative path relative to the registered workspace, added by the host. */
    repositoryPath?: string;
    name: string;
    branch: string | null;
    head: string | null;
    detached: boolean;
    upstream: string | null;
    uncommitted: WorkspaceGitChangesFile[];
    unpushedCommits: {
      commit: string;
      shortCommit: string;
      subject: string;
      files: WorkspaceGitChangesFile[];
    }[];
    unpushedCommitsTruncated: boolean;
  }[];
  contractVersion: "1";
}
export interface WorkspaceFailure {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}
export type WorkspaceResult<T = unknown> =
  { result: T } | { error: WorkspaceFailure };
export interface WorkspaceDescriptor {
  connectionId: string;
  workspaceId: string;
  title: string;
  capabilities?: WorkspaceCapabilities;
}
export interface WorkspaceInventory {
  workspaces: WorkspaceDescriptor[];
}
export interface WorkspaceWriteReceipt {
  workspaceId: string;
  path: string;
  revision: string;
  size: number;
}
export interface WorkspaceTurnChanges {
  sessionId: string;
  turnId: string;
  metric: string;
  coverage: string;
  state: string;
  files: {
    path: string;
    additions?: number | null;
    deletions?: number | null;
    state: string;
    unifiedDiff?: string | null;
  }[];
  fileCount: number;
  additions?: number | null;
  deletions?: number | null;
  revision: number;
}
export interface WorkspaceRequests {
  capabilities: Record<string, never>;
  /** Directory entries, default 2000 and maximum 10000; truncated is not a cursor. */
  filesList: { path?: string; limit?: number };
  /** Searches the whole registered workspace; default 1000, maximum 10000, result path is empty. */
  filesSearch: { query: string; limit?: number };
  /** maxBytes=0 requests a complete snapshot; previews default to 1 MiB, maximum 4 MiB. */
  filesRead: { path: string; maxBytes?: number };
  /** Canonical base64 up to 4 MiB; empty content is a valid save. Revision is full lowercase SHA-256. */
  filesWrite: { path: string; content: string; expectedRevision: string };
  gitRepository: { repositoryPath?: string };
  gitStatus: { repositoryPath?: string };
  gitBranches: { repositoryPath?: string };
  gitLog: {
    repositoryPath?: string;
    head?: string;
    offset?: number;
    limit?: number;
  };
  gitChangesNavigator: { includeClean?: boolean };
  gitDiff: {
    repositoryPath?: string;
    paths?: string[];
    scope?: "all" | "staged" | "unstaged" | "committed" | "branch";
    /** Only committed/branch scopes use revision. */ revision?: string;
  };
  gitChangeBranch: {
    repositoryPath?: string;
    operation: "create" | "switch";
    name: string;
  };
  /** A commit requires at least one explicitly selected repository-relative path. */
  gitCommit: { repositoryPath?: string; message: string; paths: string[] };
  gitPush: { repositoryPath?: string; remote: string };
  turnChanges: { sessionId: string; turnId: string; includeDiff?: boolean };
}
export interface WorkspaceResponses {
  capabilities: WorkspaceCapabilities;
  filesList: WorkspaceListing;
  filesSearch: WorkspaceListing;
  filesRead: WorkspaceFile;
  filesWrite: WorkspaceWriteReceipt;
  gitRepository: {
    isRepository: boolean;
    repositoryRoot: string | null;
    contractVersion: "1";
  };
  gitStatus: WorkspaceGitStatus;
  gitBranches: WorkspaceGitBranches;
  gitLog: WorkspaceGitLog;
  gitChangesNavigator: WorkspaceGitNavigator;
  gitDiff: WorkspaceGitDiff;
  gitChangeBranch: { branch: string; created: boolean; contractVersion: "1" };
  gitCommit: {
    commit: string;
    shortCommit: string;
    subject: string;
    committedPaths: string[];
    contractVersion: "1";
  };
  gitPush: {
    remote: string;
    branch: string;
    upstream: string;
    head: string;
    createdUpstream: boolean;
    contractVersion: "1";
  };
  turnChanges: WorkspaceTurnChanges;
}
export interface WorkspaceListRequest {
  op: "workspace.list";
}
/** Only host-registered identities cross the browser boundary. No roots, URLs or credentials. */
export type WorkspaceCallRequest = {
  [M in WorkspaceMethod]: {
    op: "workspace.call";
    connectionId: string;
    workspaceId: string;
    method: M;
    request: WorkspaceRequests[M];
    rpcId: string;
  };
}[WorkspaceMethod];
export type WorkspaceCall = (
  method: WorkspaceMethod,
  request: Record<string, unknown>,
  rpcId: string,
) => Promise<unknown>;
