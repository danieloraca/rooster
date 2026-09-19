import { lazy, Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { AlertTriangle, BookOpen, Check, ChevronRight, CircleHelp, FileCode2, FileText, Folder, FolderPlus, GitBranch, Layers, LoaderCircle, LockKeyhole, RefreshCw, Search, Settings2, SlidersHorizontal, Sparkles, X } from 'lucide-react';
import { useDesktop } from './useDesktop';
import Details from './Details';
import ArtifactList, { ArtifactIcon, relativeLabel } from './ArtifactList';
import { libraryRows, matchesCategory, rankLibraryRows } from './library';
import type { Category } from './library';
import Editing from './Editing';
import RegisteredLocations from './RegisteredLocations';
import { basename, displayPath, kindLabels, title, providerLabel } from './types';
import type { Bridge, Inspection, Scope, Provider } from './types';
const SourceViewer=lazy(()=>import('./SourceViewer'));
const Preview=lazy(()=>import('./Preview'));
const categories:{id:Category;label:string;icon:typeof Layers}[]=[{id:'all',label:'All artifacts',icon:Layers},{id:'instruction',label:'Instructions & rules',icon:BookOpen},{id:'skill',label:'Skills & commands',icon:Sparkles},{id:'agent',label:'Agents',icon:FileCode2},{id:'settings',label:'Settings',icon:Settings2}];
const errorMessage=(e:unknown)=>e instanceof Error?e.message:String(e);
export default function App({bridge}:{bridge:Bridge}) {
 const desktop=useDesktop(bridge);
 const {inventory,status,settings,busy}=desktop;
 const [category,setCategory]=useState<Category>('all');
 const [source,setSource]=useState('all');
 const [query,setQuery]=useState('');
 const [searchRank,setSearchRank]=useState<Map<string,number>|null>(null);
 const [searching,setSearching]=useState(false);
 const [attention,setAttention]=useState(false);
 const [selected,setSelected]=useState<string|null>(null);
 const [inspection,setInspection]=useState<Inspection|null>(null);
 const [viewError,setViewError]=useState<string|null>(null);
 const [tab,setTab]=useState<'source'|'preview'|'details'>('preview');
 const [contextId,setContextId]=useState('');
 const [scope,setScope]=useState<Scope|null>(null);
 const [addOpen,setAddOpen]=useState(false);
 const [issuesOpen,setIssuesOpen]=useState(false);
 const searchInput=useRef<HTMLInputElement>(null);
 const checkouts=inventory?.repositories.checkouts;
 const artifacts=inventory?.artifacts;
 const currentProvider=inventory?.provider??desktop.selection.provider;
 const providerReady=inventory?.provider===desktop.selection.provider;
 const selectedRepo=checkouts?.find(repo=>repo.id===source);
 const sourceArtifacts=useMemo(()=>artifacts?.filter(a=>source==='all'||(source==='personal'?a.provenance!=='repository':a.owner.checkout_id===source))??[],[artifacts,source]);
 const visible=useMemo(()=>{
   const result=libraryRows(sourceArtifacts,inventory?.packages??[],category,a=>(!attention||a.validation!=='valid'||!!inventory?.diagnostics.some(d=>d.artifact_id===a.id))&&(!query.trim()||!!searchRank?.has(a.id)));
   if(query.trim())result.rows=rankLibraryRows(result.rows,searchRank);
   return result;
 },[sourceArtifacts,category,attention,inventory,query,searchRank]);
 const contexts=inventory?.contexts.filter(context=>context.checkout_id===source)??[];
 const diagnosticCount=(inventory?.diagnostics.length??0)+(inventory?.repositories.issues.length??0);
 useEffect(()=>{
   if(!inventory)return;
   setSelected(previous=>previous&&inventory.artifacts.some(a=>a.id===previous)?previous:null);
   if(source!=='all'&&source!=='personal'&&!inventory.repositories.checkouts.some(repo=>repo.id===source))setSource('all');
 },[inventory,source]);
 useEffect(()=>{
   const list=inventory?.contexts.filter(context=>context.checkout_id===source)??[];
   setContextId(previous=>list.some(context=>context.id===previous)?previous:list[0]?.id??'');
 },[inventory,source]);
 useEffect(()=>{
   let live=true;setScope(null);
   if(inventory&&contextId)bridge.assess(inventory.generation,contextId).then(result=>{if(live)setScope(result);}).catch(e=>{if(live)setViewError(errorMessage(e));});
   return()=>{live=false;};
 },[bridge,inventory,contextId]);
 useEffect(()=>{
   let live=true;setInspection(null);setViewError(null);
   if(inventory&&selected)bridge.inspect(inventory.generation,selected).then(result=>{
     if(live){setInspection(result);if(!/\.(md|markdown)$/i.test(displayPath(result.artifact.path)))setTab('source');}
   }).catch(e=>{if(live)setViewError(errorMessage(e));});
   return()=>{live=false;};
 },[bridge,inventory,selected]);
 useEffect(()=>{
   let live=true;
   if(!query.trim()||!inventory){setSearchRank(null);setSearching(false);return;}
   setSearching(true);setSearchRank(null);
   const timer=setTimeout(()=>{
     bridge.search(inventory.generation,query.trim()).then(ids=>{if(live)setSearchRank(new Map(ids.map((id,index)=>[id,index])));})
       .catch(e=>{if(live){desktop.setError(errorMessage(e));setSearchRank(new Map());}}).finally(()=>{if(live)setSearching(false);});
   },180);
   return()=>{live=false;clearTimeout(timer);};
 },[bridge,inventory,query,desktop.setError]);
 useEffect(()=>{
   const key=(event:KeyboardEvent)=>{if((event.metaKey||event.ctrlKey)&&event.key==='k'){event.preventDefault();searchInput.current?.focus();}};
   window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);
 },[]);
 function choose(id:string,related=false){
   setSelected(id);setTab('preview');
   if(related){setCategory('all');setQuery('');setAttention(false);const a=artifacts?.find(a=>a.id===id);setSource(a?.owner.checkout_id??'personal');}
 }
 async function removeLocation(rootId:string){
   await desktop.remove(rootId);
   setSource('all');setSelected(null);setInspection(null);setContextId('');setScope(null);setViewError(null);
 }
 const activeCategory=categories.find(item=>item.id===category)!;
 const artifact=inspection?.artifact;
 const markdown=artifact&&/\.(md|markdown)$/i.test(displayPath(artifact.path));
 return <div className="app-shell">
   <aside className="sidebar" aria-label="Workspace navigation">
     <div className="brand"><div className="brand-mark" aria-hidden="true">R</div><div><strong>Rooster</strong><span>Your AI workspace</span></div></div>
     <div className="workspace-selector"><label htmlFor="workspace">Workspace</label><select id="workspace" value={desktop.selection.workspace_id??''} disabled={busy} onChange={event=>void desktop.refresh({workspace_id:event.target.value||null})}><option value="">All workspaces</option>{settings?.workspaces.map(workspace=><option key={workspace.id} value={workspace.id}>{workspace.name}</option>)}</select></div>
     <div className="workspace-selector provider-selector"><label htmlFor="provider">Provider</label><select id="provider" value={desktop.selection.provider} disabled={busy} onChange={event=>{setSelected(null);setInspection(null);setCategory('all');setQuery('');void desktop.refresh({provider:event.target.value as Provider});}}><option value="codex">Codex</option><option value="claude">Claude Code</option></select></div>
     <nav className="category-nav" aria-label="Artifact categories">{categories.map(item=><button className={category===item.id?'active':''} key={item.id} onClick={()=>setCategory(item.id)}><item.icon size={17}/><span>{item.label}</span><span className="nav-count">{sourceArtifacts.filter(a=>matchesCategory(a.kind,item.id)).length}</span></button>)}</nav>
     <div className="sidebar-section"><span>Locations</span><button className="icon-button" aria-label="Add workspace" title="Add workspace" disabled={busy} onClick={()=>setAddOpen(true)}><FolderPlus size={16}/></button></div>
     <nav className="location-nav" aria-label="Repository navigation"><button className={source==='all'?'selected':''} onClick={()=>setSource('all')}><Layers size={16}/><span>All locations</span></button><button className={source==='personal'?'selected':''} onClick={()=>setSource('personal')}><Settings2 size={16}/><span>Personal & installed</span></button><RegisteredLocations workspaces={settings?.workspaces??[]} workspaceId={desktop.selection.workspace_id} busy={busy} onRemove={removeLocation}/>{!!checkouts?.length&&<h2 className="location-group-label">Repositories</h2>}{checkouts?.map(repo=><button key={repo.id} title={displayPath(repo.path)} className={source===repo.id?'selected':''} onClick={()=>setSource(repo.id)}><Folder size={16}/><span>{basename(repo.path)}<small><GitBranch size={11}/>{repo.branch??repo.head_state}</small></span></button>)}</nav>
     {!checkouts?.length&&<p className="sidebar-hint">Add a repository or a parent folder to discover its guidance.</p>}
     <div className="sidebar-bottom"><button className="add-workspace" disabled={busy} onClick={()=>setAddOpen(true)}><FolderPlus size={16}/> Add workspace</button><div className="local-note"><span className="dot"/> Local files · reviewed changes</div></div>
   </aside>
   <main className="workspace-main">
     <header className="workspace-header"><div><div className="breadcrumb">{selectedRepo?basename(selectedRepo.path):source==='personal'?'Personal configuration':'Your library'}<ChevronRight size={13}/><span>{providerLabel(currentProvider)}</span></div><h1>{activeCategory.label}</h1></div><div className="header-actions"><button className={diagnosticCount?'quiet-button attention':'quiet-button'} onClick={()=>setIssuesOpen(true)}><AlertTriangle size={15}/>{diagnosticCount?`${diagnosticCount} ${diagnosticCount===1?'issue':'issues'}`:'Diagnostics'}</button><button className="quiet-button" onClick={()=>void desktop.refresh()} disabled={busy}><RefreshCw size={15} className={busy?'spin':''}/>Refresh</button></div></header>
     <Editing bridge={bridge} inventory={providerReady?inventory:null} inspection={inspection?.artifact.id===selected&&inspection?.generation===inventory?.generation?inspection:null} ownerId={source} onChanged={()=>void desktop.refresh()}/>
     {(desktop.error||status.error)&&<div className="banner error" role="alert"><AlertTriangle size={16}/><span>{desktop.error??status.error}</span><button onClick={()=>void desktop.refresh()} disabled={busy}>Retry</button></div>}
     {busy?<div className="scan-banner" role="status"><LoaderCircle size={15} className="spin"/><span>{status.progress?`Scanning ${status.progress.phase} · ${status.progress.visited.toLocaleString()} visited`:'Starting scan…'}</span><button onClick={()=>void desktop.cancel()} disabled={status.running===null}>Cancel</button><div className="scan-progress"/></div>
       :status.invalidated?<div className="banner" role="status"><RefreshCw size={15}/><span>Files changed. You’re viewing the previous snapshot.</span><button onClick={()=>void desktop.refresh()}>Refresh now</button></div>
       :inventory&&inventory.status!=='complete'?<div className="banner warning" role="status"><AlertTriangle size={15}/><span>{inventory.status==='cancelled'?'Scan cancelled. Results may be incomplete.':'Some locations could not be read. Results are incomplete.'}</span><button onClick={()=>setIssuesOpen(true)}>View details</button></div>:null}
     {status.watch_errors.length>0&&<div className="banner warning"><span>Some locations cannot be watched. Refresh manually to check for changes.</span><button onClick={()=>setIssuesOpen(true)}>Details</button></div>}
     <div className="library-toolbar"><label className="search-box"><Search size={17}/><input ref={searchInput} aria-label="Search artifacts" placeholder="Search names, paths, and content…" value={query} maxLength={512} onChange={event=>setQuery(event.target.value)}/>{query?<button aria-label="Clear search" onClick={()=>setQuery('')}><X size={14}/></button>:<kbd>⌘ K</kbd>}</label><button className={`filter-button ${attention?'enabled':''}`} aria-pressed={attention} onClick={()=>setAttention(!attention)} title="Filter artifacts with diagnostics"><SlidersHorizontal size={15}/><span>Needs attention</span></button></div>
     <div className="browse-layout">
       <section className="artifact-panel" aria-label="Artifact list"><div className="list-caption"><span>{searching?'Searching…':`${visible.count} ${visible.count===1?'artifact':'artifacts'}`}</span><span>NAME</span></div>
         <div className="artifact-list"><ArtifactList rows={visible.rows} selected={selected} filtered={!!query.trim()||attention} onSelect={choose}/>
           {visible.count===0&&!searching&&<div className="list-empty"><Search size={23}/><strong>{query||attention?'No matches':'No artifacts here yet'}</strong><p>{query||attention?'Try another search or clear your filters.':busy?'Discovery is running.':'Choose another location or add a workspace.'}</p></div>}
         </div>
         <div className="list-options"><label><input type="checkbox" checked={desktop.selection.all_markdown} disabled={busy} onChange={event=>void desktop.refresh({all_markdown:event.target.checked})}/> Include ordinary Markdown</label><label><input type="checkbox" checked={desktop.selection.include_personal} disabled={busy} onChange={event=>void desktop.refresh({include_personal:event.target.checked})}/> Personal & installed sources</label></div>
       </section>
       <section className="document-panel" aria-label="Document inspector">
         {viewError?<div className="empty-inspector"><AlertTriangle size={30}/><h2>Unable to inspect this selection</h2><p>{viewError}</p><button className="primary-button" onClick={()=>void desktop.refresh()} disabled={busy}>Refresh inventory</button></div>
         :selected&&!inspection?<div className="empty-inspector" role="status"><LoaderCircle className="spin"/>Loading source…</div>
         :inspection&&artifact?<><header className="document-header"><div className="file-heading"><ArtifactIcon artifact={artifact}/><div><h2>{title(artifact)}</h2><p title={displayPath(artifact.path)}>{relativeLabel(artifact)}</p></div></div><span className="read-only">{inspection.edit_reason?<><LockKeyhole size={12}/>Read-only source</>:<>Review before applying</>}</span></header>
           {artifact.description&&<p className="file-description">{artifact.description}</p>}
           <div className="document-controls"><div className="tabs" role="tablist" aria-label="Document views">{(['preview','source','details'] as const).map(view=><button key={view} role="tab" aria-selected={tab===view} disabled={view==='preview'&&(!markdown||inspection.text===null)} onClick={()=>setTab(view)}>{view.charAt(0).toUpperCase()+view.slice(1)}</button>)}</div>{artifact.validation==='valid'?<span className="validation"><Check size={12}/>Valid</span>:<span className="validation warning"><AlertTriangle size={12}/>{artifact.validation}</span>}</div>
           {selectedRepo&&<div className="context-control"><label htmlFor="context">Directory context</label><select id="context" value={contextId} onChange={event=>setContextId(event.target.value)}>{contexts.map(context=><option key={context.id} value={context.id}>{context.label}</option>)}</select><CircleHelp size={14} aria-label="Scope is an estimate; live loading is unknown"/></div>}
           <div className="document-content" role="tabpanel"><Suspense fallback={<div className="loading">Loading viewer…</div>}>{tab==='details'?<Details view={inspection} scope={scope} onSelect={id=>choose(id,true)}/>:inspection.text===null?<div className="empty-inspector"><FileText size={28}/><h2>{inspection.snapshot?'Binary supporting file':'Source unavailable'}</h2><p>{inspection.snapshot?`${inspection.snapshot.identity.size} bytes. View Details for the snapshot and package information.`:'This file could not be read. View Details for the diagnostic.'}</p></div>:tab==='source'?<SourceViewer text={inspection.text} path={displayPath(artifact.path)}/>:<Preview key={artifact.id} text={inspection.text}/>}</Suspense></div>
           <footer className="document-footer"><span>{kindLabels[artifact.kind]} · {inspection.snapshot?.identity.size.toLocaleString()??'—'} bytes</span><button onClick={()=>setTab('details')}>{inspection.related.length?`${inspection.related.length} related files`:'Scope & source details'}<ChevronRight size={12}/></button></footer>
         </>:<div className="empty-inspector"><div className="empty-symbol"><BookOpen size={29}/></div><h2>{settings?.workspaces.length?'Your configuration, in one place':'Meet your AI workspace'}</h2><p>{settings?.workspaces.length?'Select an instruction, skill, or agent to inspect its source and understand where it applies.':'Add a folder to explore instructions, skills, and agents across your repositories.'}</p><button className="primary-button" onClick={()=>setAddOpen(true)} disabled={busy}><FolderPlus size={16}/>Add workspace</button><span className="empty-caption">Native files stay in their original locations.</span></div>}
       </section>
     </div>
     <footer className="status-bar"><span><span className={`dot ${inventory?.status==='complete'?'':'muted-dot'}`}/>{busy?'Scanning':inventory?inventory.status==='complete'?'Inventory ready':`${inventory.status} inventory`:'Ready'}{inventory?` · ${inventory.repositories.checkouts.length} repositories · ${inventory.artifacts.length} artifacts`:''}</span><span>{providerLabel(currentProvider)} · local files</span></footer>
   </main>
   {addOpen&&<AddWorkspace onClose={()=>setAddOpen(false)} onAdd={desktop.add}/>}
   {issuesOpen&&<Modal title="Inventory diagnostics" onClose={()=>setIssuesOpen(false)}><p className="muted">Discovery issues and compatibility details from the shared Rust inventory.</p>{diagnosticCount===0&&!status.watch_errors.length?<p className="clear-state"><Check size={18}/>No inventory issues reported.</p>:<ul className="diagnostic-list">{inventory?.repositories.issues.map((issue,i)=><li key={`repo-${i}`}><strong>{displayPath(issue.path)}</strong><p>{issue.message}</p></li>)}{inventory?.diagnostics.map((d,i)=><li key={i} className={d.severity}><strong>{d.code.replaceAll('_',' ')}</strong><p>{d.message}</p><small>{displayPath(d.path)}{d.line?`:${d.line}`:''}</small>{d.artifact_id&&<button onClick={()=>{choose(d.artifact_id!,true);setIssuesOpen(false);setTab('details');}}>Inspect file</button>}</li>)}{status.watch_errors.map(error=><li key={error}><p>{error}</p></li>)}</ul>}<details><summary>Source availability & links</summary><ul className="diagnostic-list">{inventory?.sources.map((root,index)=><li key={index}><strong>{displayPath(root.path)}</strong><small>{root.provenance.replaceAll('_',' ')} · {root.available?'available':'not available'}</small></li>)}{inventory?.linked_sources.map((link,index)=><li key={`link-${index}`}><strong>{displayPath(link.path)}</strong><p>{link.reason}</p></li>)}</ul></details><p className="muted">A complete inventory does not establish what a running AI session loaded.</p></Modal>}
 </div>;
}
function Modal({title,children,onClose}:{title:string;children:React.ReactNode;onClose:()=>void}) {const dialog=useRef<HTMLDialogElement>(null);useEffect(()=>{dialog.current?.showModal();return()=>dialog.current?.close();},[]);return <dialog ref={dialog} className="modal" aria-label={title} onCancel={onClose}><header><h2>{title}</h2><button className="icon-button" aria-label="Close dialog" onClick={onClose}><X size={18}/></button></header><div className="modal-body">{children}</div></dialog>;}
function AddWorkspace({onAdd,onClose}:{onAdd:(name:string)=>Promise<boolean>;onClose:()=>void}) {const [name,setName]=useState('');const [pending,setPending]=useState(false);const [error,setError]=useState('');return <Modal title="Add workspace" onClose={onClose}><p>Choose a parent folder or a single repository. Give it a name that makes sense on this machine.</p><form onSubmit={event=>{event.preventDefault();setPending(true);setError('');onAdd(name).then(added=>{if(added)onClose();}).catch(error=>setError(errorMessage(error))).finally(()=>setPending(false));}}><label className="field-label" htmlFor="workspace-name">Workspace name</label><input id="workspace-name" autoFocus className="text-field" placeholder="e.g. Work, Personal, Experiments" required maxLength={120} value={name} onChange={event=>setName(event.target.value)} disabled={pending}/>{error&&<p className="form-error" role="alert">{error}</p>}<div className="modal-actions"><button type="button" className="quiet-button" onClick={onClose} disabled={pending}>Cancel</button><button className="primary-button" disabled={!name.trim()||pending}>{pending?<LoaderCircle size={16} className="spin"/>:<FolderPlus size={16}/>}Choose folder…</button></div></form></Modal>;}
