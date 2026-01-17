import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import react from '@astrojs/react';

// https://astro.build/config
export default defineConfig({
	integrations: [
		react(),
		starlight({
			title: 'Pluot',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/keller-mark/pluot' }],
			sidebar: [
				{
					label: 'Start Here',
					// Autogenerate a group of links for the 'constellations' directory.
					autogenerate: { directory: 'start' },
				},
				{
					label: 'Reference',
					autogenerate: { directory: 'reference' },
				},
				{
					label: 'Examples',
					autogenerate: { directory: 'examples' },
				},
			],
		}),
	],
});
