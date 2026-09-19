// Only imported by the Vite development entry point. No fixture bridge ships in release builds.
import type { Bridge, Inspection, Inventory, Provider, Scope, Settings, Status } from '../types';
interface Fixture {settings:Settings;inventory:Inventory;inspections:Record<string,Inspection>;scopes:Record<string,Scope>}
export async function fixtureBridge():Promise<Bridge>{
  const response=await fetch('/__rooster_fixture.json');
  if(!response.ok)throw new Error('Start the dev server with ROOSTER_BROWSER_FIXTURE pointing to an exported disposable fixture.');
  const loaded:Fixture|{providers:Partial<Record<Provider,Fixture>>}=await response.json();
  const fixtures='providers' in loaded?loaded.providers:{[loaded.inventory.provider??'codex']:loaded};
  let data=fixtures.codex??fixtures.claude!;let completed=0;
  const status:Status={running:null,completed:0,generation:0,progress:null,invalidated:false,error:null,watch_errors:[]};
  return {
    edit:async(action)=>{if(action==='state')return {owners:[],drafts:[],history:[],data_path:'Read-only browser fixture'} as never;throw new Error("Editing is available only in the native app.");},chooseDraftSource:async()=>null,
    settings:async()=>data.settings,chooseWorkspace:async()=>null,
    removeLocation:async()=>{throw new Error('Location removal is available only in the native app.');},
    start:async(request)=>{const next=fixtures[request.provider];if(!next)throw new Error('Provider is absent from this read-only fixture.');data=next;completed++;status.completed=completed;status.generation=completed;return completed;},
    status:async()=>({...status}),cancel:async()=>{},inventory:async()=>({...data.inventory,generation:completed}),
    inspect:async(_,id)=>{if(!data.inspections[id])throw new Error('Unknown fixture ID');return data.inspections[id];},
    assess:async(_,id)=>{if(!data.scopes[id])throw new Error('Unknown fixture context');return data.scopes[id];},
    search:async(_,query)=>Object.values(data.inspections).filter(view=>[view.artifact.name,JSON.stringify(view.artifact.path),view.text].some(text=>text?.toLowerCase().includes(query.toLowerCase()))).map(view=>view.artifact.id),
  };
}
