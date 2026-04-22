import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { eventsStore } from './lib/events.svelte';

// Control API の WebSocket を起動時に 1 本張る。
// 以降、各コンポーネントは eventsStore.recent / eventsStore.connection を参照するだけで良い。
// 失敗しても自動再接続が走るので、VAC 未起動でも GUI 自体は動く。
eventsStore.connect();

const app = mount(App, {
 target: document.getElementById('app')!,
});

export default app;
