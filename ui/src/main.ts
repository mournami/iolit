import './fonts.css';
import { mount } from 'svelte';
import App from './App.svelte';
import './style.css';
import './theme.svelte';
mount(App, { target: document.getElementById('app')! });
