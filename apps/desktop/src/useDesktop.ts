import { useCallback, useEffect, useRef, useState } from 'react';
import type { Bridge, Inventory, ScanRequest, Settings, Status } from './types';
const initialStatus:Status={running:null,completed:0,generation:0,progress:null,invalidated:false,error:null,watch_errors:[]};
export function useDesktop(bridge:Bridge) {
  const [settings,setSettings]=useState<Settings|null>(null);
  const [inventory,setInventory]=useState<Inventory|null>(null);
  const [status,setStatus]=useState<Status>(initialStatus);
  const [error,setError]=useState<string|null>(null);
  const [starting,setStarting]=useState(false);
  const request=useRef<ScanRequest>({provider:'codex',workspace_id:null,all_markdown:false,include_personal:false});
  const generation=useRef(0);
  const settingsEpoch=useRef(0);
  const busy=useRef(false);
  const queued=useRef(false);
  const lastTicket=useRef(0);
  const startingRequest=useRef(false);
  const loaded=useRef(false);
  const lastStart=useRef(0);
  const [selection,setSelection]=useState(request.current);
  const refresh=useCallback(async (change:Partial<ScanRequest>={})=>{
    const next={...request.current,...change};request.current=next;setSelection(next);
    if(busy.current){queued.current=true;return;}
    busy.current=true;startingRequest.current=true;setStarting(true);setError(null);
    try { const ticket=await bridge.start(next);lastTicket.current=ticket;lastStart.current=Date.now();setStatus(s=>({...s,running:ticket,error:null})); }
    catch(e) { setError(String(e));busy.current=false; }
    finally {startingRequest.current=false;setStarting(false);}
  },[bridge]);
  useEffect(()=>{
    let live=true;
    bridge.settings().then(settings=>{
      if(!live)return;setSettings(settings);loaded.current=true;
      void refresh({include_personal:settings.include_personal});
    }).catch(e=>{if(live)setError(String(e));});
    return()=>{live=false;};
  },[bridge,refresh]);
  useEffect(()=>{
    let live=true;let timer:ReturnType<typeof setTimeout>;
    async function poll(){
      const epoch=settingsEpoch.current;
      try {
        const next=await bridge.status();if(!live)return;
        setStatus(next);busy.current=startingRequest.current||next.running!==null||lastTicket.current>next.completed;
        if(!queued.current&&next.generation>generation.current){
          const [data,settings]=await Promise.all([bridge.inventory(next.generation),bridge.settings()]);
          if(!live)return;
          if(epoch===settingsEpoch.current&&!queued.current){generation.current=next.generation;setInventory(data);setSettings(settings);}
        }
      }catch(e){if(live&&epoch===settingsEpoch.current&&!queued.current)setError(String(e));}
      if(live&&!busy.current&&queued.current){queued.current=false;void refresh();}
      if(live)timer=setTimeout(()=>void poll(),400);
    }
    void poll();return()=>{live=false;clearTimeout(timer);};
  },[bridge,refresh]);
  useEffect(()=>{
    const focus=()=>{if(loaded.current&&!busy.current&&Date.now()-lastStart.current>2000)void refresh();};
    window.addEventListener('focus',focus);return()=>window.removeEventListener('focus',focus);
  },[refresh]);
  const add=useCallback(async(name:string)=>{
    setError(null);const next=await bridge.chooseWorkspace(name);
    if(next){setSettings(next);await refresh();}return next!==null;
  },[bridge,refresh]);
  const remove=useCallback(async(rootId:string)=>{
    const next=await bridge.removeLocation(rootId);
    settingsEpoch.current++;setSettings(next);setInventory(null);setError(null);
    const workspaceId=request.current.workspace_id;
    await refresh({workspace_id:next.workspaces.some(workspace=>workspace.id===workspaceId)?workspaceId:null});
  },[bridge,refresh]);
  const cancel=useCallback(async()=>{
    if(status.running!==null)try{await bridge.cancel(status.running);}catch(e){setError(String(e));}
  },[bridge,status.running]);
  return {settings,inventory,status,error,setError,selection,refresh,add,remove,cancel,busy:starting||status.running!==null};
}
