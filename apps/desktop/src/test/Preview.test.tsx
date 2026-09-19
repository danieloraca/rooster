import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import Preview, { previewDocument } from '../Preview';
describe('untrusted Markdown',()=>{
 it('preserves readable content while removing executable and remote elements',()=>{
  const source=`---\nname: hostile\n---\n# Safe heading\n<script>parent.__TAURI_INTERNALS__.invoke('choose_workspace')</script>\n<img src="https://example.invalid/track" onerror="alert(1)">\n\n![Remote](https://example.invalid/image)\n[Execute](javascript:alert(1))\n[Visit](https://example.invalid/)`;
  const doc=new DOMParser().parseFromString(previewDocument(source),'text/html');
  expect(doc.body.textContent).toContain('Safe heading');
  expect(doc.body.textContent).toContain('Image: Remote — not loaded');
  expect(doc.body.textContent).not.toContain('name: hostile');
  expect(doc.body.querySelector('script,img,a,iframe,object')).toBeNull();
  expect(doc.querySelector('meta[http-equiv]')?.getAttribute('content')).toContain("connect-src 'none'");
 });
 it('keeps rendered content in a sandbox with no scripts or parent-origin privileges',()=>{
  render(<Preview text="# Hello"/>);const frame=screen.getByTitle('Markdown preview');
  expect(frame).toHaveAttribute('sandbox','');expect(frame).toHaveAttribute('referrerpolicy','no-referrer');
 });
});
