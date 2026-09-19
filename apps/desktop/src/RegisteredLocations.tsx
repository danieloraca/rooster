import { useEffect, useRef, useState } from 'react';
import { Folder, LoaderCircle, X } from 'lucide-react';
import { basename, displayPath } from './types';
import type { Workspace } from './types';

type Location = { workspace: Workspace; root: Workspace['roots'][number] };
interface Props {
  workspaces: Workspace[];
  workspaceId: string | null;
  busy: boolean;
  onRemove: (rootId: string) => Promise<void>;
}

export default function RegisteredLocations({ workspaces, workspaceId, busy, onRemove }: Props) {
  const [removing, setRemoving] = useState<Location | null>(null);
  const locations = workspaces
    .filter(workspace => workspaceId === null || workspace.id === workspaceId)
    .flatMap(workspace => workspace.roots.map(root => ({ workspace, root })));
  return <>
    {locations.length > 0 && <section aria-label="Registered folders" className="registered-locations">
      <h2 className="location-group-label">Registered folders</h2>
      <ul>{locations.map(({ workspace, root }) => <li className="registered-location" key={root.id}>
        <Folder size={15} aria-hidden="true"/>
        <div className="location-name" title={`${workspace.name} · ${displayPath(root.path)}`}>
          <strong>{basename(root.path)}</strong>
          <small>{workspace.name} · {displayPath(root.path)}</small>
        </div>
        <button className="icon-button remove-location" title="Remove location" disabled={busy}
          aria-label={`Remove location ${displayPath(root.path)} from ${workspace.name}`}
          onClick={() => setRemoving({ workspace, root })}><X size={14}/></button>
      </li>)}</ul>
    </section>}
    {removing && <RemoveLocation location={removing} onRemove={onRemove} onClose={() => setRemoving(null)}/>}
  </>;
}

function RemoveLocation({ location: { workspace, root }, onRemove, onClose }: {
  location: Location; onRemove: Props['onRemove']; onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  async function remove() {
    setPending(true); setError('');
    try { await onRemove(root.id); onClose(); }
    catch (error) { setError(error instanceof Error ? error.message : String(error)); setPending(false); }
  }
  return <dialog ref={dialog} className="modal" aria-label="Remove location" onCancel={event => {
    event.preventDefault(); if (!pending) onClose();
  }}>
    <header><h2>Remove location?</h2><button className="icon-button" aria-label="Close dialog" disabled={pending} onClick={onClose}><X size={18}/></button></header>
    <div className="modal-body">
      <p>Stop scanning this folder in <strong>{workspace.name}</strong>?</p>
      <p className="removal-path">{displayPath(root.path)}</p>
      <p>Your files and repositories stay where they are. Saved drafts and recovery history are kept. You can add this folder again later.</p>
      {workspace.roots.length === 1 && <p>This is the last location in {workspace.name}, so the empty workspace will also be removed.</p>}
      <p>Repositories included through another registered folder will still appear.</p>
      {error && <p className="form-error" role="alert">{error}</p>}
      <div className="modal-actions">
        <button className="quiet-button" autoFocus disabled={pending} onClick={onClose}>Cancel</button>
        <button className="primary-button" disabled={pending} onClick={() => void remove()}>
          {pending && <LoaderCircle size={16} className="spin"/>}{pending ? 'Removing…' : 'Remove location'}
        </button>
      </div>
    </div>
  </dialog>;
}
