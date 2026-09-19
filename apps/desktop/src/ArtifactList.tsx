import { useEffect, useState } from 'react';
import { AlertTriangle, BookOpen, ChevronRight, FileCode2, FileText, LockKeyhole, Package, Sparkles } from 'lucide-react';
import { basename, displayPath, kindLabels, title } from './types';
import type { Artifact, NativePath } from './types';
import type { LibraryRow } from './library';

export default function ArtifactList({ rows, selected, filtered, onSelect }: {
  rows: LibraryRow[]; selected: string | null; filtered: boolean; onSelect: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const selectedParent = rows.find(row => row.children.some(child => child.id === selected))?.artifact.id;
  useEffect(() => {
    if (selectedParent) setExpanded(previous => previous.has(selectedParent) ? previous : new Set(previous).add(selectedParent));
  }, [selectedParent]);
  function toggle(id: string) {
    setExpanded(previous => { const next = new Set(previous); if (next.has(id)) next.delete(id); else next.add(id); return next; });
  }
  return rows.map(({ artifact, children, packageRoot }) => children.length === 0
    ? <ArtifactRow key={artifact.id} artifact={artifact} selected={selected} onSelect={onSelect}/>
    : <section className="skill-group" key={artifact.id} aria-label={`${title(artifact)} skill package`}>
      <ArtifactRow artifact={artifact} selected={selected} onSelect={onSelect}/>
      {filtered ? <div className="skill-files-label">{children.length} matching package {children.length === 1 ? 'file' : 'files'}</div>
        : <button className="skill-files-toggle" aria-label={`${expanded.has(artifact.id) ? 'Collapse' : 'Expand'} files for ${title(artifact)}`}
          aria-expanded={expanded.has(artifact.id)} onClick={() => toggle(artifact.id)}>
          <ChevronRight size={13} className={expanded.has(artifact.id) ? 'expanded' : ''}/>
          {children.length} supporting {children.length === 1 ? 'file' : 'files'}
        </button>}
      {(filtered || expanded.has(artifact.id)) && <div className="skill-children">{children.map(child =>
        <ArtifactRow key={child.id} artifact={child} selected={selected} onSelect={onSelect} packageRoot={packageRoot}/>
      )}</div>}
    </section>);
}

function ArtifactRow({ artifact, selected, onSelect, packageRoot }: {
  artifact: Artifact; selected: string | null; onSelect: (id: string) => void; packageRoot?: NativePath | null;
}) {
  return <button className={`artifact-row ${packageRoot ? 'package-child' : ''} ${selected === artifact.id ? 'selected' : ''}`}
    onClick={() => onSelect(artifact.id)} aria-pressed={selected === artifact.id}>
    <ArtifactIcon artifact={artifact}/>
    <span className="row-content">
      <strong>{packageRoot ? relativeLabel(artifact, packageRoot) : title(artifact)}</strong>
      <span>{kindLabels[artifact.kind]}{!packageRoot && <><span className="separator">·</span>{artifact.provenance === 'repository' ? basename(artifact.owner.root) : artifact.provenance.replaceAll('_', ' ')}</>}</span>
      <small title={displayPath(artifact.path)}>{relativeLabel(artifact)}</small>
    </span>
    {artifact.validation !== 'valid' ? <AlertTriangle size={14} className="warning-icon" aria-label={artifact.validation}/>
      : artifact.read_only_reason ? <LockKeyhole size={13} className="muted" aria-label="Read-only source"/> : null}
  </button>;
}
export function ArtifactIcon({ artifact }: { artifact: Artifact }) {
  const Icon = artifact.kind === 'skill' ? Sparkles : ['agent', 'legacy_agent'].includes(artifact.kind) ? FileCode2
    : artifact.kind === 'instruction' ? BookOpen : artifact.kind === 'supporting_file' ? Package : FileText;
  return <span className={`artifact-icon kind-${artifact.kind}`}><Icon size={18}/></span>;
}
export function relativeLabel(artifact: Artifact, base = artifact.owner.root): string {
  const path = displayPath(artifact.path), root = displayPath(base);
  return path.startsWith(root + '/') ? path.slice(root.length + 1) : path;
}
