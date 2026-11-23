import './fonts.css';
import { mount } from 'svelte';
import App from './App.svelte';
import 'overlayscrollbars/overlayscrollbars.css';
import './style.css';
import './theme.svelte';
mount(App, { target: document.getElementById('app')! });
