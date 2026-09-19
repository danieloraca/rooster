// CodeMirror uses normalized line offsets. Map them back to the original text so
// a local edit does not normalize untouched CRLF or mixed line endings.
export const normalizedText=(text:string)=>text.replace(/\r\n?/g,'\n');
function rawOffset(raw:string,position:number):number {
 let visual=0,i=0;
 while(i<raw.length&&visual<position){if(raw[i]==='\r'&&raw[i+1]==='\n')i++;i++;visual++;}
 return i;
}
export function patchEditorText(raw:string,changes:{from:number;to:number;text:string}[]):string {
 const newline=raw.includes('\r\n')&&!raw.replace(/\r\n/g,'').includes('\n')?'\r\n':'\n';
 let result=raw;
 for(const change of [...changes].reverse()) {
  const from=rawOffset(raw,change.from),to=rawOffset(raw,change.to);
  result=result.slice(0,from)+normalizedText(change.text).replace(/\n/g,newline)+result.slice(to);
 }
 return result;
}
