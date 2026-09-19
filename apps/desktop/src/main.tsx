import React from 'react';
import ReactDOM from 'react-dom/client';
import { isTauri } from '@tauri-apps/api/core';
import App from './App';
import { nativeBridge } from './bridge';
import './styles.css';
let bridge = nativeBridge;
if (import.meta.env.DEV && new URLSearchParams(location.search).get('fixture') === '1') {
  const { fixtureBridge } = await import('./test/fixture');
  bridge = await fixtureBridge();
}
const available = isTauri() || (import.meta.env.DEV && new URLSearchParams(location.search).get('fixture') === '1');
ReactDOM.createRoot(document.getElementById('root')!).render(<React.StrictMode>{available
  ? <App bridge={bridge}/>
  : <main className="desktop-required"><div className="brand-mark">R</div><h1>Open Rooster on your desktop</h1><p>Your local files are available through the Tauri application.</p><code>npm run desktop</code></main>
}</React.StrictMode>);
