import { useEffect, useRef } from 'react';
import { Compartment, EditorState } from '@codemirror/state';
import { EditorView, lineNumbers, highlightSpecialChars, drawSelection, keymap } from '@codemirror/view';
import { defaultHighlightStyle, syntaxHighlighting, StreamLanguage } from '@codemirror/language';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import { markdown } from '@codemirror/lang-markdown';
import { json } from '@codemirror/lang-json';
import { yaml } from '@codemirror/lang-yaml';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { normalizedText, patchEditorText } from './editorText';
export default function SourceViewer({text,path,onChange,locked=false}:{text:string;path:string;onChange?:(value:string)=>void;locked?:boolean}) {
 const container=useRef<HTMLDivElement>(null),editor=useRef<EditorView|null>(null);
 const access=useRef(new Compartment());
 const raw=useRef(text),changed=useRef(onChange),external=useRef(false);
 changed.current=onChange;
 const editable=Boolean(onChange);
 useEffect(()=>{
  if(!container.current)return;
  const ext=path.split('.').at(-1)?.toLowerCase();
  const language=ext==='json'?json():ext==='toml'?StreamLanguage.define(toml):ext==='yaml'||ext==='yml'?yaml():ext==='md'||ext==='markdown'?markdown():[];
  const view=new EditorView({parent:container.current,state:EditorState.create({doc:normalizedText(raw.current),extensions:[
   access.current.of([EditorState.readOnly.of(!editable||locked),EditorView.editable.of(editable&&!locked)]),EditorView.lineWrapping,
   EditorView.contentAttributes.of({'aria-label':editable?'Draft source':'Read-only source',tabindex:'0'}),
   lineNumbers(),highlightSpecialChars(),drawSelection(),highlightSelectionMatches(),history(),
   keymap.of([...defaultKeymap,...historyKeymap,...searchKeymap]),syntaxHighlighting(defaultHighlightStyle),language,
   EditorView.updateListener.of(update=>{
    if(!update.docChanged||external.current)return;
    const changes:{from:number;to:number;text:string}[]=[];
    update.changes.iterChanges((from,to,_from,_to,insert)=>changes.push({from,to,text:insert.toString()}));
    raw.current=patchEditorText(raw.current,changes);changed.current?.(raw.current);
   }),
   EditorView.theme({'&':{height:'100%',fontSize:'15px'},'.cm-scroller':{overflow:'auto',fontFamily:'ui-monospace, SFMono-Regular, Menlo, monospace',lineHeight:'1.75'},'.cm-content':{padding:'20px 0'},'.cm-line':{padding:'0 24px 0 16px'},'.cm-gutters':{backgroundColor:'#fafafa',color:'#737681',border:'none',minWidth:'45px'},'&.cm-focused':{outline:'2px solid #a6bba1',outlineOffset:'-2px'}}),
  ]})});editor.current=view;
  return()=>{editor.current=null;view.destroy();};
 },[path,editable]);
 useEffect(()=>{editor.current?.dispatch({effects:access.current.reconfigure([EditorState.readOnly.of(!editable||locked),EditorView.editable.of(editable&&!locked)])});},[editable,locked]);
 useEffect(()=>{
  raw.current=text;const view=editor.current,normalized=normalizedText(text);
  if(view&&view.state.doc.toString()!==normalized){external.current=true;view.dispatch({changes:{from:0,to:view.state.doc.length,insert:normalized}});external.current=false;}
 },[text]);
 return <div className="source-viewer" ref={container}/>;
}
