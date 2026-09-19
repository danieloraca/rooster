import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {afterEach,beforeAll,expect,it,vi} from 'vitest';
import Editing from '../Editing';
import type {Bridge,Draft,Inspection,Inventory} from '../types';
vi.mock('../SourceViewer',()=>({default:({text,onChange}:{text:string;onChange:(s:string)=>void})=><textarea aria-label="Draft source" value={text} onChange={e=>onChange(e.target.value)}/> }));
afterEach(cleanup);
beforeAll(()=>{HTMLDialogElement.prototype.showModal=function(){this.open=true;};HTMLDialogElement.prototype.close=function(){this.open=false;};});
it('retains and reopens the saved draft when review reports a source conflict',async()=>{
 const path='/fixture/AGENTS.md';
 const artifact={id:'a',kind:'instruction',path,physical_path:path,owner:{id:'o',root:'/fixture',checkout_id:'o',head:null,branch:'main'},provenance:'repository',read_only_reason:null,references:[]};
 const inspection={artifact,text:'original',edit_reason:null} as unknown as Inspection;
 const inventory={generation:1,artifacts:[artifact],repositories:{checkouts:[{id:'o',path:'/fixture'}]}} as unknown as Inventory;
 let draft:Draft={provider:'codex',id:'draft-one',revision:1,target:{operation:'edit',artifact_id:'a'},path,original:'original',text:'original',updated_at:1};
 let opened=false;
 const edit=vi.fn(async(action:string,args:Record<string,unknown>)=>{
  if(action==='state')return {owners:[{id:'o',root:'/fixture',checkout:true}],drafts:opened?[draft]:[],history:[],data_path:'/private/recovery'};
  if(action==='open'){opened=true;return draft;}
  if(action==='read')return draft;
  if(action==='save'){draft={...draft,text:String(args.text),revision:draft.revision+1};return draft;}
  if(action==='prepare_draft')throw new Error('Draft base changed. Compare the current source.');
  throw new Error('Unexpected '+action);
 });
 const bridge={edit,chooseDraftSource:async()=>null} as unknown as Bridge;
 render(<Editing bridge={bridge} inventory={inventory} inspection={inspection} ownerId="o" onChanged={()=>{}}/>);
 fireEvent.click(screen.getByRole('button',{name:'Edit'}));
 const input=await screen.findByRole('textbox',{name:'Draft source'});fireEvent.change(input,{target:{value:'my preserved draft'}});
 await waitFor(()=>expect(draft.text).toBe('my preserved draft'));
 fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('Draft base changed');expect(input).toHaveValue('my preserved draft');
 fireEvent.click(screen.getByRole('button',{name:'Keep draft & close'}));
 await waitFor(()=>expect(screen.queryByRole('textbox',{name:'Draft source'})).not.toBeInTheDocument());
 fireEvent.click(screen.getByRole('button',{name:/Drafts/}));fireEvent.click(await screen.findByRole('button',{name:/AGENTS.md/}));
 expect(await screen.findByRole('textbox',{name:'Draft source'})).toHaveValue('my preserved draft');
 expect(edit.mock.calls.some(c=>c[0]==='apply')).toBe(false);
});
it('offers Claude native types and sends the chosen kind and path through the shared bridge',async()=>{
 const edit=vi.fn(async(action:string)=>{if(action==='state')return {owners:[{id:'o',root:'/fixture',checkout:true}],drafts:[],history:[],data_path:'/private/recovery'};if(action==='open')throw new Error('fixture stops at draft request');throw new Error(action);});
 const inventory={provider:'claude',generation:2,artifacts:[],repositories:{checkouts:[{id:'o',path:'/fixture'}]}} as unknown as Inventory;
 render(<Editing bridge={{edit} as unknown as Bridge} inventory={inventory} inspection={null} ownerId="o" onChanged={()=>{}}/>);
 await waitFor(()=>expect(edit).toHaveBeenCalledWith('state',{generation:2}));
 fireEvent.click(screen.getByRole('button',{name:'New'}));
 expect(screen.getByRole('option',{name:'Claude agent (.md)'})).toBeInTheDocument();
 expect(screen.queryByRole('option',{name:/Codex agent/})).not.toBeInTheDocument();
 fireEvent.change(screen.getByRole('combobox',{name:'File type'}),{target:{value:'rule'}});
 fireEvent.change(screen.getByRole('textbox',{name:'Relative destination path'}),{target:{value:'.claude/rules/api.md'}});
 fireEvent.click(screen.getByRole('button',{name:'Start draft'}));
 await waitFor(()=>expect(edit).toHaveBeenCalledWith('open',{generation:2,target:{operation:'create',owner_id:'o',path:'.claude/rules/api.md',kind:'rule'}}));
});
