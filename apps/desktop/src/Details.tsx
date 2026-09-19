import { FileText, Link2, LockKeyhole, ShieldQuestion } from 'lucide-react';
import { basename, displayPath, kindLabels, title } from './types';
import type { Artifact, Inspection, Scope } from './types';
export default function Details({view,scope,onSelect}:{view:Inspection;scope:Scope|null;onSelect:(id:string)=>void}) {
 const artifact=view.artifact;
 const availability=scope?.skills.concat(scope.agents).find(item=>item.artifact_id===artifact.id);
 const instruction=scope?.instructions.find(item=>item.artifact_id===artifact.id);
 return <div className="details-view">
   <section><h3>About this file</h3><dl className="facts">
     <dt>Provider</dt><dd>{artifact.provider??'Reference document'}</dd>
     <dt>Type</dt><dd>{kindLabels[artifact.kind]}</dd>
     <dt>Source</dt><dd>{artifact.provenance.replaceAll('_',' ')}</dd>
     <dt>Owner</dt><dd>{basename(artifact.owner.root)}{artifact.owner.branch?` · ${artifact.owner.branch}`:''}</dd>
     <dt>Validation</dt><dd>{artifact.validation}</dd>
     <dt>Location</dt><dd className="path">{displayPath(artifact.path)}</dd>
     {displayPath(artifact.path)!==displayPath(artifact.physical_path)&&<><dt>Physical target</dt><dd className="path">{displayPath(artifact.physical_path)}</dd></>}
     <dt>Directory scope</dt><dd className="path">{displayPath(artifact.scope_directory)}</dd>
     {view.snapshot&&<><dt>Snapshot</dt><dd>{view.snapshot.identity.size.toLocaleString()} bytes · {view.snapshot.newline} · {view.snapshot.utf8?'UTF-8':'Binary'}</dd><dt>SHA-256</dt><dd className="path mono">{view.snapshot.sha256}</dd></>}
   </dl></section>
   <section className="notice"><LockKeyhole size={16}/><div><strong>{view.edit_reason?'Read-only source':'Source editing'}</strong><p>{view.edit_reason??'Edit a private draft, then review every affected file before applying. Native files stay authoritative.'}</p></div></section>
   <section><h3><ShieldQuestion size={16}/> Directory applicability</h3>
     {scope?<><p className="context-path">{displayPath(scope.context)}</p><p>{availability?.reason??instruction?.reason??'This file is not in the selected instruction chain. Supporting files have no independent activation.'}</p>{availability&&<span className="status-label">{availability.state.replaceAll('_',' ')}</span>}{instruction&&<span className="status-label">{instruction.bytes_included} bytes selected</span>}
       <details><summary>What remains unknown</summary><ul>{scope.unknowns.map(unknown=><li key={unknown}>{unknown}</li>)}</ul></details></>
       :<p className="muted">Choose a repository and directory to estimate applicability. Disk presence does not establish live-session loading.</p>}
   </section>
   {view.package&&<section><h3><FileText size={16}/> Skill package <span className="count">{view.package.member_ids.length}</span></h3>
     <p className="path muted">{displayPath(view.package.root)}</p><Related artifacts={view.related.filter(a=>a.package_id===view.package?.id)} onSelect={onSelect}/></section>}
   <section><h3><Link2 size={16}/> References <span className="count">{artifact.references.length}</span></h3>
     {artifact.references.length===0?<p className="muted">No document links found.</p>:<ul className="reference-list">{artifact.references.map((reference,index)=><li key={index}>
       {reference.target_id?<button onClick={()=>onSelect(reference.target_id!)}>{reference.destination}</button>:<span>{reference.destination}</span>}
       <small>{reference.status.replaceAll('_',' ')} · line {reference.line}{!reference.destination.startsWith('@')&&reference.destination.includes('#')?' · fragment unchecked':''}</small></li>)}</ul>}
   </section>
   {view.diagnostics.length>0&&<section><h3>Diagnostics</h3><ul className="diagnostic-list">{view.diagnostics.map((diagnostic,i)=><li key={i} className={diagnostic.severity}><strong>{diagnostic.code.replaceAll('_',' ')}</strong><p>{diagnostic.message}</p></li>)}</ul></section>}
   {artifact.unknown_fields.length>0&&<section><h3>Uninterpreted fields</h3><p>{artifact.unknown_fields.join(', ')}</p></section>}
   {view.metadata!==null&&<details><summary>Native metadata</summary><pre>{JSON.stringify(view.metadata,null,2)}</pre></details>}
 </div>;
}
function Related({artifacts,onSelect}:{artifacts:Artifact[];onSelect:(id:string)=>void}) {return <ul className="related-list">{artifacts.map(artifact=><li key={artifact.id}><button onClick={()=>onSelect(artifact.id)}><FileText size={15}/><span>{title(artifact)}<small>{kindLabels[artifact.kind]}</small></span></button></li>)}</ul>;}
