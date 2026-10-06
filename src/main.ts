import { mount } from 'svelte';
import './style.css';
import App from './App.svelte';
import { applyTheme } from './lib/theme';

applyTheme('black');
mount(App, { target: document.getElementById('app')! });
