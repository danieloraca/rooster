import { useMemo } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
const previewStyles = `:root{color-scheme:light}body{max-width:760px;margin:0 auto;padding:34px 40px 70px;color:#35373b;font:16px/1.75 -apple-system,BlinkMacSystemFont,Segoe UI,sans-serif;overflow-wrap:anywhere}h1,h2,h3{color:#202126;line-height:1.3;letter-spacing:-.02em}h1{font-size:28px;margin:0 0 24px}h2{font-size:21px;margin:32px 0 12px}h3{font-size:17px}p{margin:14px 0}pre{padding:17px 20px;background:#f5f5f7;border:1px solid #e9e9ec;border-radius:7px;overflow:auto}code{font:15px/1.65 ui-monospace,SFMono-Regular,Menlo,monospace}p code,li code{background:#f1f1f3;padding:2px 5px;border-radius:3px}blockquote{border-left:3px solid #c0c1c8;margin:20px 0;padding:0 20px;color:#696b73}table{border-collapse:collapse;width:100%}th,td{border-bottom:1px solid #e6e6ea;padding:9px 12px;text-align:left}th{background:#f7f7f9}hr{border:0;border-top:1px solid #e6e6ea;margin:28px 0}.reference{color:#9e4434;text-decoration:underline;text-underline-offset:3px}.media{display:block;background:#f7f7f9;color:#74757c;border:1px dashed #d7d8dd;padding:14px 18px;border-radius:5px}input{pointer-events:none}`;
export function previewDocument(source:string):string {
  const body = source.replace(/^\uFEFF?---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/, '');
  const html = renderToStaticMarkup(<Markdown remarkPlugins={[remarkGfm]} skipHtml
    components={{a:({children})=><span className="reference">{children}</span>, img:({alt})=><span className="media">Image: {alt || 'Untitled'} — not loaded</span>}}>{body}</Markdown>);
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src 'none'; connect-src 'none'; script-src 'none'; form-action 'none'; base-uri 'none'"><style>${previewStyles}</style></head><body>${html}</body></html>`;
}
export default function Preview({text}:{text:string}) {
  const document=useMemo(()=>previewDocument(text),[text]);
  return <iframe className="markdown-preview" title="Markdown preview" sandbox="" referrerPolicy="no-referrer" srcDoc={document}/>;
}
