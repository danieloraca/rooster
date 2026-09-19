import { useEffect, useRef } from 'react';
import { X } from 'lucide-react';
export default function Dialog({title,children,onClose,wide=false,busy=false}:{title:string;children:React.ReactNode;onClose:()=>void;wide?:boolean;busy?:boolean}) {
 const dialog=useRef<HTMLDialogElement>(null);
 useEffect(()=>{const node=dialog.current;node?.showModal();return()=>node?.close();},[]);
 return <dialog ref={dialog} className={`modal ${wide?'editing-modal':''}`} aria-label={title} onCancel={e=>{e.preventDefault();if(!busy)onClose();}}>
  <header><h2>{title}</h2><button className="icon-button" aria-label="Close dialog" disabled={busy} onClick={onClose}><X size={18}/></button></header><div className="modal-body">{children}</div>
 </dialog>;
}
