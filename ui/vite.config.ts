import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath, URL } from 'node:url';
export default defineConfig({ root: fileURLToPath(new URL('.', import.meta.url)),plugins:[svelte()],clearScreen:false,server:{host:'127.0.0.1',port:1420,strictPort:true},build:{target:'es2022',outDir:'dist',emptyOutDir:true} });
