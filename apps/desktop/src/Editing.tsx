import { lazy, Suspense, useCallback, useEffect, useRef, useState } from 'react';
import { Copy, FilePlus2, GitBranch, History, Pencil, Save, Trash2 } from 'lucide-react';
import Dialog from './Dialog';
import { basename, displayPath, title, providerLabel } from './types';
import type { Bridge, ChangeOutcome, ChangePreview, CleanupPreview, Draft, DraftComparison, EditingState, FileKind, GitChanges, Inspection, Inventory, OwnerChoice, StructuralRequest } from './types';
const SourceViewer=lazy(()=>import('./SourceViewer'));
const Preview=lazy(()=>import('./Preview'));
const message=(e:unknown)=>e instanceof Error?e.message:String(e);
const reloadGuidance='Saved locally. Rooster cannot confirm what a running assistant loaded. Reload its configuration or start a new session as appropriate.';
type Operation='create'|'duplicate'|'rename'|'delete';
export default function Editing({bridge,inventory,inspection,ownerId,onChanged}:{bridge:Bridge;inventory:Inventory|null;inspection:Inspection|null;ownerId:string;onChanged:()=>void}) {
 const [state,setState]=useState<EditingState|null>(null),[version,setVersion]=useState(0);
 const [operation,setOperation]=useState<Operation|null>(null),[draft,setDraft]=useState<Draft|null>(null),[preview,setPreview]=useState<ChangePreview|null>(null);
 const [panel,setPanel]=useState<'drafts'|'history'|'git'|null>(null),[pending,setPending]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState('');
 const generation=inventory?.generation??null;
 const refresh=useCallback(()=>setVersion(v=>v+1),[]);
 const changed=()=>{refresh();onChanged();};
 useEffect(()=>{let live=true;bridge.edit('state',{generation}).then(s=>{if(live)setState(s);}).catch(e=>{if(live)setError(message(e));});return()=>{live=false;};},[bridge,generation,version]);
 async function run(action:()=>Promise<void>){setPending(true);setError('');try{await action();}catch(e){setError(message(e));}finally{setPending(false);}}
 const artifact=inspection?.artifact;
 const supported=artifact&&['instruction','skill','skill_metadata','agent','reference','markdown','rule','legacy_command'].includes(artifact.kind)&&inspection?.text!==null;
 const readOnly=inspection?.edit_reason??artifact?.read_only_reason??(!supported?'This file type is for inspection only.':null);
 const editable=Boolean(artifact&&supported&&!readOnly&&['repository','personal'].includes(artifact.provenance));
 return <>
  <div className="editing-toolbar" aria-label="File actions">
   <div><button className="quiet-button" disabled={!inventory||pending} onClick={()=>setOperation('create')}><FilePlus2 size={14}/>New</button>
   <button className="quiet-button" title={readOnly??'Edit source'} disabled={!editable||pending} onClick={()=>void run(async()=>{setDraft(await bridge.edit('open',{generation:inventory!.generation,target:{operation:'edit',artifact_id:artifact!.id}}));refresh();})}><Pencil size={14}/>Edit</button>
   <button className="quiet-button" disabled={!supported||pending} onClick={()=>setOperation('duplicate')}><Copy size={14}/>Duplicate</button>
   <button className="quiet-button" disabled={!editable||pending} onClick={()=>setOperation('rename')}>Rename</button>
   <button className="quiet-button" disabled={!editable||pending} onClick={()=>setOperation('delete')}><Trash2 size={14}/>Delete</button></div>
   <div><button className="quiet-button" onClick={()=>{refresh();setPanel('drafts');}}><Save size={14}/>Drafts{state?.drafts.length?` (${state.drafts.length})`:''}</button>
   <button className="quiet-button" onClick={()=>{refresh();setPanel('history');}}><History size={14}/>Recovery</button>
   <button className="quiet-button" disabled={!inventory?.repositories.checkouts.length} onClick={()=>setPanel('git')}><GitBranch size={14}/>Git changes</button></div>
  </div>
  {error&&<div className="banner error" role="alert"><span>{error}</span><button onClick={()=>setError('')}>Dismiss</button></div>}
  {notice&&<div className="banner" role="status"><span>{notice}</span><button onClick={()=>setNotice('')}>Dismiss</button></div>}
  {operation&&inventory&&<OperationDialog operation={operation} bridge={bridge} inventory={inventory} inspection={inspection} owners={state?.owners??[]} initialOwner={ownerId} onClose={()=>setOperation(null)} onDraft={d=>{setDraft(d);setOperation(null);refresh();}} onPreview={p=>{setPreview(p);setOperation(null);refresh();}}/>}
  {draft&&<DraftDialog key={draft.id} initial={draft} bridge={bridge} onClose={()=>{setDraft(null);refresh();}} onApplied={()=>{setNotice(reloadGuidance);changed();}}/>}
  {preview&&<ReviewDialog bridge={bridge} preview={preview} onClose={()=>setPreview(null)} onApplied={outcome=>{setNotice(outcome.status==='completed'?reloadGuidance:outcome.status==='restored'?'Original files restored. Refresh the assistant as appropriate.':'The operation needs recovery; saved originals and proposal are retained.');changed();}}/>}
  {panel==='drafts'&&<Dialog title="Saved drafts" onClose={()=>setPanel(null)}><p>Private drafts stay outside your repositories. Opening a draft never replaces its saved text with a newer file.</p><div className="saved-list">{state?.drafts.map(d=><button key={d.id} disabled={pending} onClick={()=>void run(async()=>{setDraft(await bridge.edit('read',{id:d.id}));setPanel(null);})}><strong>{basename(d.path)} · {providerLabel(d.provider)}</strong><small>{displayPath(d.path)}</small><small>{new Date(d.updated_at*1000).toLocaleString()}</small></button>)}</div>{!state?.drafts.length&&<p>No saved drafts.</p>}<p className="muted path">{state?.data_path}</p></Dialog>}
  {panel==='history'&&<HistoryDialog bridge={bridge} state={state} onClose={()=>setPanel(null)} onRefresh={refresh} onPreview={p=>{setPreview(p);setPanel(null);}}/>}
  {panel==='git'&&inventory&&<GitDialog bridge={bridge} inventory={inventory} ownerId={ownerId} onClose={()=>setPanel(null)}/>}
 </>;
}
function OwnerSelect({owners,value,onChange}:{owners:OwnerChoice[];value:string;onChange:(value:string)=>void}) {
 return <label className="field-label">Destination owner<select className="text-field" required value={value} onChange={e=>onChange(e.target.value)}><option value="">Choose a repository or personal root</option>{owners.map(o=><option value={o.id} key={o.id}>{displayPath(o.root)}{o.checkout?' · Git checkout':' · personal'}</option>)}</select></label>;
}
function OperationDialog({operation,bridge,inventory,inspection,owners,initialOwner,onClose,onDraft,onPreview}:{operation:Operation;bridge:Bridge;inventory:Inventory;inspection:Inspection|null;owners:OwnerChoice[];initialOwner:string;onClose:()=>void;onDraft:(d:Draft)=>void;onPreview:(p:ChangePreview)=>void}) {
 const [owner,setOwner]=useState(owners.some(o=>o.id===initialOwner)?initialOwner:''),[path,setPath]=useState(''),[kind,setKind]=useState<FileKind>('markdown'),[repairs,setRepairs]=useState<string[]>([]),[pending,setPending]=useState(false),[error,setError]=useState('');
 const a=inspection?.artifact,packageRoot=inspection?.package?.root;
 const impacted=inventory.artifacts.filter(other=>other.references.some(r=>r.target_id===a?.id||(packageRoot&&r.target&&displayPath(r.target).startsWith(displayPath(packageRoot)+'/'))));
 async function submit(e:React.FormEvent){e.preventDefault();setPending(true);setError('');try{
  if(operation==='create'){onDraft(await bridge.edit('open',{generation:inventory.generation,target:{operation:'create',owner_id:owner,path,kind}}));}
  else if(a){const request:StructuralRequest=operation==='duplicate'?{operation,artifact_id:a.id,owner_id:owner,path}:operation==='rename'?{operation,artifact_id:a.id,path,repair_links:repairs}:{operation,artifact_id:a.id};onPreview(await bridge.edit('prepare',{generation:inventory.generation,request}));}
 }catch(e){setError(message(e));}finally{setPending(false);}}
 return <Dialog title={operation==='create'?'Create a file':`${operation.charAt(0).toUpperCase()+operation.slice(1)} ${a?title(a):'file'}`} onClose={onClose} busy={pending}>
  <form onSubmit={e=>void submit(e)}>
   {(operation==='create'||operation==='duplicate')&&<OwnerSelect owners={owners} value={owner} onChange={setOwner}/>}
   {operation==='create'&&<label className="field-label">File type<select className="text-field" value={kind} onChange={e=>setKind(e.target.value as FileKind)}><option value="markdown">Markdown document</option><option value="instruction">Instruction ({inventory.provider==='claude'?'CLAUDE.md':'AGENTS.md'})</option><option value="skill">Skill package (SKILL.md)</option><option value="agent">{inventory.provider==='claude'?'Claude agent (.md)':'Codex agent (.toml)'}</option>{inventory.provider==='claude'?<><option value="rule">Rule (.claude/rules)</option><option value="legacy_command">Legacy command (.claude/commands)</option></>:<option value="skill_metadata">Skill metadata (agents/openai.yaml)</option>}</select></label>}
   {operation==='rename'&&<p>Owner: <strong className="path">{a&&displayPath(a.owner.root)}</strong>. Renames stay inside this owner.</p>}
   {operation!=='delete'&&<label className="field-label">Relative destination path<input className="text-field" required autoFocus value={path} onChange={e=>setPath(e.target.value)} placeholder={kind==='skill'&&operation==='create'?(inventory.provider==='claude'?'.claude/skills/example/SKILL.md':'.agents/skills/example/SKILL.md'):'docs/example.md'}/></label>}
   {operation==='create'&&<p className="muted">Use {providerLabel(inventory.provider)}’s native layout under the selected owner. New skills start with SKILL.md; supporting files stay grouped in their package.</p>}
   {packageRoot&&a?.kind==='skill'&&<div className="package-notice"><strong>Complete skill package</strong><p className="path">{displayPath(packageRoot)}</p><p>This includes every file and directory: references, scripts, binary assets, and empty folders. The next preview lists them all.</p></div>}
   {operation==='duplicate'&&<p>The copy is independent. Its native name and metadata are preserved for you to review.</p>}
   {(operation==='rename'||operation==='delete')&&<section><h3>Known incoming references ({impacted.length})</h3>{impacted.map(other=><div className="reference-choice" key={other.id}>{operation==='rename'&&other.owner.id===a?.owner.id?<label><input type="checkbox" checked={repairs.includes(other.id)} onChange={e=>setRepairs(ids=>e.target.checked?[...ids,other.id]:ids.filter(id=>id!==other.id))}/>{displayPath(other.path)}</label>:<span>{displayPath(other.path)}{other.owner.id!==a?.owner.id?' · different owner; manual repair':''}</span>}</div>)}{operation==='rename'&&<p className="muted">Select supported inline Markdown links to repair. Ambiguous syntax stays unchanged with a warning. You can also select moved documents below to rebase their outgoing links.</p>}{operation==='rename'&&inventory.artifacts.filter(other=>other.owner.id===a?.owner.id&&other.references.length>0&&!impacted.some(i=>i.id===other.id)&&(other.id===a?.id||other.package_id===a?.package_id&&a?.package_id)).map(other=><label className="reference-choice" key={other.id}><input type="checkbox" checked={repairs.includes(other.id)} onChange={e=>setRepairs(ids=>e.target.checked?[...ids,other.id]:ids.filter(id=>id!==other.id))}/>{displayPath(other.path)}</label>)}</section>}
   {operation==='delete'&&<p>Review the complete deletion next. Apply retains originals for conflict-aware restore.</p>}
   {error&&<p className="form-error" role="alert">{error}</p>}
   <div className="modal-actions"><button type="button" className="quiet-button" onClick={onClose} disabled={pending}>Cancel</button><button className="primary-button" disabled={pending}>{pending?'Preparing…':operation==='create'?'Start draft':'Review changes'}</button></div>
  </form>
 </Dialog>;
}
function DraftDialog({initial,bridge,onClose,onApplied}:{initial:Draft;bridge:Bridge;onClose:()=>void;onApplied:()=>void}) {
 const [draft,setDraft]=useState(initial),[text,setText]=useState(initial.text),[pending,setPending]=useState(false),[saving,setSaving]=useState(false),[error,setError]=useState(''),[tab,setTab]=useState<'write'|'preview'|'compare'>('write');
 const [comparison,setComparison]=useState<DraftComparison|null>(null),[preview,setPreview]=useState<ChangePreview|null>(null),[completed,setCompleted]=useState(false),[discarding,setDiscarding]=useState(false);
 const current=useRef(initial),chain=useRef<Promise<Draft>>(Promise.resolve(initial)),alive=useRef(true);
 useEffect(()=>{alive.current=true;return()=>{alive.current=false;};},[]);
 const flush=useCallback((value:string)=>{
  const job=chain.current.catch(()=>current.current).then(async()=>{
   if(current.current.text===value)return current.current;
   if(alive.current)setSaving(true);
   try{const next=await bridge.edit('save',{id:current.current.id,revision:current.current.revision,text:value});current.current=next;if(alive.current)setDraft(next);return next;}
   finally{if(alive.current)setSaving(false);}
  });chain.current=job;return job;
 },[bridge]);
 useEffect(()=>{if(text===draft.text||pending||preview)return;const timer=setTimeout(()=>{void flush(text).catch(e=>setError(message(e)));},650);return()=>clearTimeout(timer);},[text,draft.text,pending,preview,flush]);
 async function run(action:()=>Promise<void>){setPending(true);setError('');try{await action();}catch(e){setError(message(e));}finally{setPending(false);}}
 async function close(){await run(async()=>{await flush(text);onClose();});}
 async function compare(){await run(async()=>{await flush(text);setComparison(await bridge.edit('compare',{id:draft.id}));setTab('compare');});}
 async function rebase(reload:boolean){await run(async()=>{const d=await flush(text);if(!comparison?.current_token)return;const next=await bridge.edit('rebase',{id:d.id,revision:d.revision,current_token:comparison.current_token});current.current=next;setDraft(next);if(reload){const value=comparison.current_text??'';setText(value);await flush(value);}setComparison(null);setTab('write');});}
 if(preview)return <ReviewDialog bridge={bridge} preview={preview} onClose={()=>{if(completed)onClose();else setPreview(null);}} onApplied={async outcome=>{
  if(outcome.status==='completed'){setCompleted(true);try{await bridge.edit('discard',{id:current.current.id,revision:current.current.revision});}catch(e){setError(`Files saved; draft retained: ${message(e)}`);}onApplied();}
 }}/>;
 return <Dialog title={`Edit ${basename(draft.path)} · ${providerLabel(draft.provider)}`} wide busy={pending} onClose={()=>void close()}>
  <p className="path draft-path">{displayPath(draft.path)}</p>
  <div className="draft-toolbar"><div className="tabs" role="tablist" aria-label="Draft views">{(['write','preview','compare'] as const).map(t=><button key={t} role="tab" aria-selected={tab===t} disabled={pending||(t==='preview'&&!/\.(md|markdown)$/i.test(displayPath(draft.path)))} onClick={()=>t==='compare'?void compare():setTab(t)}>{t==='write'?'Source':t==='preview'?'Markdown preview':'Compare current file'}</button>)}</div><span role="status">{saving?'Saving draft…':text!==draft.text?'Unsaved draft':'Draft saved locally'}</span></div>
  {error&&<p className="form-error" role="alert">{error} Your draft is retained.</p>}
  {tab==='write'?<div className="draft-editor"><Suspense fallback={<p>Loading editor…</p>}><SourceViewer text={text} path={displayPath(draft.path)} onChange={setText} locked={pending}/></Suspense></div>
   :tab==='preview'?<div className="draft-editor"><Suspense fallback={<p>Loading preview…</p>}><Preview text={text}/></Suspense></div>
   :<section>{comparison?.conflict?<p className="form-error" role="alert">{comparison.conflict}</p>:<p>The source still matches this draft’s base.</p>}<Diff before={comparison?.current_text??null} after={text} beforeLabel="Current file" afterLabel="Your draft"/>{comparison?.conflict&&comparison.can_rebase&&<div className="modal-actions"><button className="quiet-button" disabled={pending} onClick={()=>void rebase(true)}>Replace draft with current file</button><button className="primary-button" disabled={pending} onClick={()=>void rebase(false)}>Keep draft on this current base</button></div>}{comparison?.conflict&&!comparison.can_rebase&&<p>Ownership or checkout changes require reopening the source explicitly. This draft remains available; copy its text if you need a new draft.</p>}</section>}
  <p className="muted">Drafts are private. Native metadata is validated when you review changes; applying always checks the source again.</p>
  {discarding&&<div className="package-notice"><p>Discard this draft and its unsaved text? Source files will not change.</p><button className="quiet-button" disabled={pending} onClick={()=>setDiscarding(false)}>Keep editing</button> <button className="quiet-button danger" disabled={pending} onClick={()=>void run(async()=>{await chain.current.catch(()=>{});await bridge.edit('discard',{id:current.current.id,revision:current.current.revision});onClose();})}>Confirm discard</button></div>}
  <div className="modal-actions draft-actions"><button className="quiet-button" disabled={pending} onClick={()=>void run(async()=>{const value=await bridge.chooseDraftSource();if(value!==null){setText(value);await flush(value);}})}>Import text…</button><button className="quiet-button danger" disabled={pending} onClick={()=>setDiscarding(true)}>Discard draft</button><span className="spacer"/><button className="quiet-button" disabled={pending} onClick={()=>void close()}>Keep draft & close</button><button className="quiet-button" disabled={pending||text===draft.text} onClick={()=>void run(async()=>{await flush(text);})}>Save draft</button><button className="primary-button" disabled={pending} onClick={()=>void run(async()=>{const d=await flush(text);setPreview(await bridge.edit('prepare_draft',{id:d.id,revision:d.revision}));})}>{pending?'Working…':'Review changes'}</button></div>
 </Dialog>;
}
function Diff({before,after,beforeLabel='Before',afterLabel='After'}:{before:string|null;after:string|null;beforeLabel?:string;afterLabel?:string}) {
 const a=before??'',b=after??'';let prefix=0,suffix=0;
 while(prefix<a.length&&prefix<b.length&&a[prefix]===b[prefix])prefix++;
 while(suffix<a.length-prefix&&suffix<b.length-prefix&&a[a.length-1-suffix]===b[b.length-1-suffix])suffix++;
 const content=(s:string)=> <>{s.slice(0,prefix)}<mark>{s.slice(prefix,s.length-suffix)}</mark>{suffix?s.slice(s.length-suffix):''}</>;
 return <div className="diff-columns"><section><h4>{beforeLabel}</h4><pre className="before">{before===null?<em>Absent / no text</em>:content(a)}</pre></section><section><h4>{afterLabel}</h4><pre className="after">{after===null?<em>Absent / no text</em>:content(b)}</pre></section></div>;
}
function ReviewDialog({bridge,preview,onClose,onApplied}:{bridge:Bridge;preview:ChangePreview;onClose:()=>void;onApplied:(outcome:ChangeOutcome)=>void|Promise<void>}) {
 const [pending,setPending]=useState(false),[error,setError]=useState(''),[outcome,setOutcome]=useState<ChangeOutcome|null>(null),[restore,setRestore]=useState(false);
 const status=outcome?.status??preview.status;
 async function apply(reverse:boolean){setPending(true);setError('');try{const result=await bridge.edit(reverse?'restore':'apply',{id:preview.id});setOutcome(result);await onApplied(result);}catch(e){setError(message(e));}finally{setPending(false);}}
 const canForward=['prepared','applying','recovery_required'].includes(status),canRestore=['completed','applying','recovery_required','restoring'].includes(status);
 return <Dialog title={restore?'Review restore':'Review changes'} wide busy={pending} onClose={onClose}>
  <p><strong>{preview.files.length} affected paths</strong> · {status.replaceAll('_',' ')} · <span className="mono">{preview.id}</span></p>
  {preview.warnings.length>0&&<div className="package-notice"><strong>Review warnings</strong><ul>{preview.warnings.map((w,i)=><li key={i}>{w}</li>)}</ul></div>}
  <div className="change-files">{preview.files.map((file,i)=><details key={i} open={preview.files.length<=4}><summary><span className={`change-action ${file.action}`}>{restore?`undo ${file.action}`:file.action}</span>{displayPath(file.path)}{file.directory?' /':''}</summary><p className="muted">{file.before_bytes??0} → {file.after_bytes??0} bytes · modes {file.before_mode?.toString(8)??'—'} → {file.after_mode?.toString(8)??'—'}</p>{!file.directory&&(file.before_text!==null||file.after_text!==null)?<Diff before={restore?file.after_text:file.before_text} after={restore?file.before_text:file.after_text} beforeLabel={restore?'Expected current':'Before'} afterLabel={restore?'Restore original':'After'}/>:!file.directory?<dl className="binary-hashes"><dt>Before SHA-256</dt><dd>{file.before_sha256??'Absent'}</dd><dt>After SHA-256</dt><dd>{file.after_sha256??'Absent'}</dd></dl>:null}</details>)}</div>
  {error&&<p className="form-error" role="alert">{error} No conflicting external revision is overwritten. Compare the source or review Recovery.</p>}
  {outcome&&<div className={outcome.error?'form-error':'save-outcome'} role="status"><strong>{outcome.status.replaceAll('_',' ')}</strong>{outcome.error&&<p>{outcome.error}</p>}{outcome.status==='completed'?<p>Saved {outcome.changed.length} paths.</p>:outcome.status!=='restored'?<p>{outcome.changed.length} paths recorded as applied; {outcome.pending.length} steps lack a recorded apply. An interrupted step may already have changed.</p>:null}{outcome.status==='completed'&&<p>{reloadGuidance}</p>}{outcome.status==='restored'&&<p>Original files restored.</p>}</div>}
  <div className="modal-actions"><button className="quiet-button" disabled={pending} onClick={onClose}>Close</button>{canRestore&&!restore&&<button className="quiet-button" disabled={pending} onClick={()=>setRestore(true)}>Review restore</button>}{restore&&canRestore?<button className="primary-button" disabled={pending} onClick={()=>void apply(true)}>{pending?'Restoring…':'Restore original files'}</button>:canForward&&<button className="primary-button" disabled={pending} onClick={()=>void apply(false)}>{pending?'Applying…':status==='prepared'?'Apply reviewed changes':'Finish interrupted change'}</button>}</div>
 </Dialog>;
}
function HistoryDialog({bridge,state,onClose,onRefresh,onPreview}:{bridge:Bridge;state:EditingState|null;onClose:()=>void;onRefresh:()=>void;onPreview:(p:ChangePreview)=>void}) {
 const [days,setDays]=useState(30),[cleanup,setCleanup]=useState<CleanupPreview|null>(null),[error,setError]=useState(''),[pending,setPending]=useState(false);
 async function run(action:()=>Promise<void>){setPending(true);setError('');try{await action();}catch(e){setError(message(e));}finally{setPending(false);}}
 return <Dialog title="Recovery history" wide busy={pending} onClose={onClose}><p>Review a saved proposal, finish an interrupted change, or restore originals. Newer external edits remain protected.</p><div className="saved-list">{state?.history.map(h=><button disabled={pending} key={h.id} onClick={()=>void run(async()=>onPreview(await bridge.edit('preview',{id:h.id})))}><strong>{h.status.replaceAll('_',' ')} · {h.changes} paths</strong><small>{h.id} · {new Date(h.updated_at*1000).toLocaleString()}</small>{h.error&&<span className="form-error">{h.error}</span>}</button>)}</div>{!state?.history.length&&<p>No saved changes.</p>}
 <section className="retention"><h3>Recovery retention</h3><p>Cleanup is explicit. Prepared drafts and unresolved operations are always retained.</p><label className="field-label">Completed/restored records older than (days)<input className="text-field" type="number" min="0" max="36500" value={days} onChange={e=>{setDays(Number(e.target.value));setCleanup(null);}}/></label><button className="quiet-button" disabled={pending} onClick={()=>void run(async()=>setCleanup(await bridge.edit('cleanup_preview',{days})))}>Preview cleanup</button>{cleanup&&<><p>{cleanup.candidates.length} eligible records. Removing them permanently removes their backups and restore capability; source files stay unchanged.</p><ul>{cleanup.candidates.map(c=><li key={c.id}>{c.id} · {c.status}</li>)}</ul><button className="quiet-button danger" disabled={pending||!cleanup.candidates.length} onClick={()=>void run(async()=>{const result=await bridge.edit('cleanup',{days:cleanup.days,ids:cleanup.candidates.map(c=>c.id)});setCleanup(null);onRefresh();if(result.error)setError(`${result.removed.length} removed; ${result.pending.length} retained. ${result.error}`);})}>Delete listed recovery records</button></>}</section>{error&&<p className="form-error" role="alert">{error}</p>}<p className="muted path">{state?.data_path}</p></Dialog>;
}
function GitDialog({bridge,inventory,ownerId,onClose}:{bridge:Bridge;inventory:Inventory;ownerId:string;onClose:()=>void}) {
 const [owner,setOwner]=useState(inventory.repositories.checkouts.some(c=>c.id===ownerId)?ownerId:inventory.repositories.checkouts[0]?.id??''),[changes,setChanges]=useState<GitChanges|null>(null),[error,setError]=useState('');
 useEffect(()=>{let live=true;setChanges(null);setError('');if(owner)bridge.edit('git',{generation:inventory.generation,owner_id:owner}).then(c=>{if(live)setChanges(c);}).catch(e=>{if(live)setError(message(e));});return()=>{live=false;};},[bridge,inventory.generation,owner]);
 return <Dialog title="Git changes" wide onClose={onClose}><label className="field-label">Checkout<select className="text-field" value={owner} onChange={e=>setOwner(e.target.value)}>{inventory.repositories.checkouts.map(c=><option value={c.id} key={c.id}>{displayPath(c.path)}</option>)}</select></label><p>Local staged and working-tree changes. Rooster does not commit, push, or switch branches.</p>{error?<p className="form-error" role="alert">{error}</p>:changes?<><p>Branch: {changes.branch??'Detached HEAD'} · {changes.head?.slice(0,12)??'No commits yet'}</p>{changes.entries.length?<table className="git-table"><thead><tr><th>Index</th><th>Worktree</th><th>Path</th></tr></thead><tbody>{changes.entries.map((entry,i)=><tr key={i}><td>{entry.index_status.trim()||'—'}</td><td>{entry.worktree_status.trim()||'—'}</td><td>{entry.original_path?`${displayPath(entry.original_path)} → `:''}{displayPath(entry.path)}</td></tr>)}</tbody></table>:<p>Working tree clean.</p>}<p className="muted">M modified · A added · D deleted · R renamed · ? untracked · U unresolved</p></>:<p role="status">Reading Git status…</p>}</Dialog>;
}
