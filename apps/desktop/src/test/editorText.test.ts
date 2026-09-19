import {expect,it} from 'vitest';
import {normalizedText,patchEditorText} from '../editorText';
it('maps normalized editor positions to raw mixed line endings and Unicode without changing unrelated bytes',()=>{
 const original='\ufefffirst\r\nsecond\n😀 last\r\n';
 const normalized=normalizedText(original),from=normalized.indexOf('second');
 expect(patchEditorText(original,[{from,to:from+6,text:'edited\nadded'}])).toBe('\ufefffirst\r\nedited\nadded\n😀 last\r\n');
 const emoji=normalized.indexOf('😀');
 expect(patchEditorText(original,[{from:emoji,to:emoji+2,text:'🌱'}])).toBe('\ufefffirst\r\nsecond\n🌱 last\r\n');
});
it('keeps uniform CRLF when inserting a new editor line',()=>{
 expect(patchEditorText('a\r\nb\r\n',[{from:1,to:1,text:'\nc'}])).toBe('a\r\nc\r\nb\r\n');
});
