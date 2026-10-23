import './styles/workspace.css';
import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app');

const app = target
  ? mount(App, {
      target,
    })
  : null;

export default app;

export { default as App } from './App.svelte';
export { default as AppNavigation } from './components/AppNavigation.svelte';
export { default as WorkspaceHeader } from './components/WorkspaceHeader.svelte';
export { default as StatusRegion } from './components/StatusRegion.svelte';
export { default as SettingsPage } from './components/SettingsPage.svelte';
export { default as HelpPage } from './components/HelpPage.svelte';
export * from './types';
export * from './navigation';
export * from './theme';
