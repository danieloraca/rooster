export type Provider='codex'|'claude';
export const providerLabel=(provider:Provider|undefined)=>provider==='claude'?'Claude Code':'Codex';
// Native paths are display-only in the UI. Commands use inventory and registration IDs.
export type NativePath = string | { unix_bytes: number[] } | { windows_wide: number[] };
export type Kind = 'instruction' | 'skill' | 'agent' | 'legacy_agent' | 'skill_metadata' | 'provider_config' | 'plugin_manifest' | 'reference' | 'supporting_file' | 'markdown' | 'rule' | 'legacy_command';
export type Provenance = 'repository' | 'personal' | 'system' | 'installed_plugin' | 'compatibility' | 'managed' | 'synced';
export type Validation = 'valid' | 'malformed' | 'unsupported' | 'unavailable';
export interface Workspace { id: string; name: string; roots: { id: string; path: NativePath }[] }
export interface Settings { workspaces: Workspace[]; config_path: string; include_personal: boolean }
export interface ScanRequest { provider: Provider; workspace_id: string | null; all_markdown: boolean; include_personal: boolean }
export interface Checkout { id: string; path: NativePath; branch: string | null; head: string | null; head_state: string; kind: string; root_ids: string[] }
export interface Reference { destination: string; line: number; target: NativePath | null; target_id: string | null; status: string; fragment_checked: boolean }
export interface SnapshotInfo { sha256: string; utf8: boolean; has_bom: boolean; newline: string; identity: { size: number } }
export interface Artifact {
  id: string; kind: Kind; provider: string | null; path: NativePath; physical_path: NativePath;
  owner: { id: string; root: NativePath; checkout_id: string | null; head: string | null; branch: string | null };
  provenance: Provenance; source_root: NativePath; scope_directory: NativePath; scope_checkout_id: string | null;
  package_id: string | null; name: string | null; description: string | null; declared_role_names: string[];
  role_declaration_count: number; validation: Validation; unknown_fields: string[]; read_only_reason: string | null;
  snapshot: SnapshotInfo | null; ignored: boolean | null; references: Reference[];
}
export interface Diagnostic { severity: 'info' | 'warning' | 'error'; code: string; message: string; artifact_id: string | null; path: NativePath; line: number | null }
export interface Context { id: string; label: string; checkout_id: string; path: NativePath }
export interface SkillPackage { id: string; root: NativePath; physical_root: NativePath; skill_id: string; member_ids: string[] }
export interface Inventory {
  provider: Provider;
  generation: number; status: 'complete' | 'partial' | 'cancelled';
  repositories: { checkouts: Checkout[]; issues: { path: NativePath; message: string }[]; roots: {id:string;path:NativePath}[]; worktree_candidates: {path:NativePath}[] };
  artifacts: Artifact[]; packages: SkillPackage[]; contexts: Context[]; diagnostics: Diagnostic[];
  sources: {path:NativePath;provenance:Provenance;available:boolean}[];
  linked_sources: {path:NativePath;target:NativePath|null;inspected:boolean;reason:string}[];
}
export interface Inspection { edit_reason?: string | null; generation: number; artifact: Artifact; metadata: unknown; text: string | null; snapshot: SnapshotInfo | null; package: SkillPackage | null; related: Artifact[]; diagnostics: Diagnostic[] }
export interface Availability {artifact_id:string;state:string;reason:string;implicit_invocation:boolean|null}
export interface Scope { context: NativePath; checkout_id: string | null; assumed_config_layers:string[]; instructions: {artifact_id:string;bytes_included:number;reason:string}[]; skills:Availability[]; agents:Availability[]; unknowns:string[]; instruction_byte_limit:number|null }
export interface Status {running:number|null;completed:number;generation:number;progress:{phase:string;path:NativePath;visited:number;artifacts:number}|null;invalidated:boolean;error:string|null;watch_errors:string[]}
export interface Bridge {
  edit<K extends keyof EditingCommands>(action:K,args:EditingCommands[K][0]):Promise<EditingCommands[K][1]>;
  chooseDraftSource():Promise<string|null>;
  settings(): Promise<Settings>;
  chooseWorkspace(name: string): Promise<Settings | null>;
  removeLocation(rootId: string): Promise<Settings>;
  start(request: ScanRequest): Promise<number>;
  status(): Promise<Status>;
  cancel(ticket:number):Promise<void>;
  inventory(generation:number):Promise<Inventory>;
  inspect(generation:number,id:string):Promise<Inspection>;
  assess(generation:number,id:string):Promise<Scope>;
  search(generation:number,query:string):Promise<string[]>;
}
export function displayPath(path: NativePath): string {
  if (typeof path === 'string') return path;
  if ('unix_bytes' in path) return new TextDecoder().decode(new Uint8Array(path.unix_bytes));
  return path.windows_wide.map(unit => String.fromCharCode(unit)).join('');
}
export function basename(path: NativePath): string { return displayPath(path).split(/[\\/]/).filter(Boolean).at(-1) ?? displayPath(path); }
export const kindLabels: Record<Kind,string> = {instruction:'Instruction',skill:'Skill',agent:'Agent',legacy_agent:'Legacy agent',skill_metadata:'Skill metadata',provider_config:'Provider settings',plugin_manifest:'Plugin manifest',reference:'Reference',supporting_file:'Supporting file',markdown:'Markdown',rule:'Rule',legacy_command:'Legacy command'};
export function title(artifact:Artifact):string { return artifact.name ?? basename(artifact.path); }

export type FileKind='markdown'|'instruction'|'skill'|'skill_metadata'|'agent'|'rule'|'legacy_command';
export type DraftTarget={operation:'edit';artifact_id:string}|{operation:'create';owner_id:string;path:string;kind:FileKind};
export interface Draft {provider:Provider;id:string;revision:number;target:DraftTarget;path:NativePath;original:string|null;text:string;updated_at:number}
export interface DraftSummary {provider:Provider;id:string;revision:number;path:NativePath;updated_at:number}
export interface DraftComparison {draft:Draft;current_text:string|null;current_token:string|null;conflict:string|null;can_rebase:boolean}
export type ChangeStatus='prepared'|'applying'|'completed'|'recovery_required'|'restoring'|'restored';
export interface ChangeFile {path:NativePath;action:string;directory:boolean;before_sha256:string|null;after_sha256:string|null;before_text:string|null;after_text:string|null;before_bytes:number|null;after_bytes:number|null;before_mode:number|null;after_mode:number|null}
export interface ChangePreview {id:string;status:ChangeStatus;files:ChangeFile[];warnings:string[]}
export interface ChangeOutcome {id:string;status:ChangeStatus;changed:NativePath[];pending:NativePath[];error:string|null}
export interface HistoryEntry {id:string;status:ChangeStatus;changes:number;error:string|null;updated_at:number}
export interface OwnerChoice {id:string;root:NativePath;checkout:boolean}
export interface EditingState {owners:OwnerChoice[];drafts:DraftSummary[];history:HistoryEntry[];data_path:string}
export type StructuralRequest={operation:'duplicate';artifact_id:string;owner_id:string;path:string}|{operation:'rename';artifact_id:string;path:string;repair_links:string[]}|{operation:'delete';artifact_id:string};
export interface CleanupPreview {days:number;candidates:HistoryEntry[]}
export interface CleanupOutcome {removed:string[];pending:string[];error:string|null}
export interface GitChanges {owner_id:string;root:NativePath;branch:string|null;head:string|null;entries:{path:NativePath;original_path:NativePath|null;index_status:string;worktree_status:string}[]}
export interface EditingCommands {
 state:[{generation:number|null},EditingState];
 open:[{generation:number;target:DraftTarget},Draft];
 read:[{id:string},Draft];
 save:[{id:string;revision:number;text:string},Draft];
 compare:[{id:string},DraftComparison];
 rebase:[{id:string;revision:number;current_token:string},Draft];
 prepare_draft:[{id:string;revision:number},ChangePreview];
 discard:[{id:string;revision:number},null];
 prepare:[{generation:number;request:StructuralRequest},ChangePreview];
 preview:[{id:string},ChangePreview];
 apply:[{id:string},ChangeOutcome];
 restore:[{id:string},ChangeOutcome];
 cleanup_preview:[{days:number},CleanupPreview];
 cleanup:[{days:number;ids:string[]},CleanupOutcome];
 git:[{generation:number;owner_id:string},GitChanges];
}
