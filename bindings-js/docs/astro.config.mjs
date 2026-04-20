import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import react from '@astrojs/react';

// https://astro.build/config
export default defineConfig({
	site: "https://pluot.dev",
	base: "/",
	trailingSlash: "always",
	integrations: [
		react(),
		starlight({
      title: 'Pluot',
      favicon: '/favicon.png',
      editLink: {
        baseUrl: 'https://github.com/keller-mark/pluot/edit/main/bindings-js/docs/',
      },
      social: [
        { icon: 'document', label: 'Docs.rs', href: 'https://docs.rs/pluot/' },
        { icon: 'github', label: 'GitHub', href: 'https://github.com/keller-mark/pluot' }
      ],
			sidebar: [
				{
					label: 'Overview',
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
			customCss: [
				// Relative path to your custom CSS file
				'./src/styles/custom.css',
			],
			components: {
				// Override the default `ThemeSelect` component.
        ThemeSelect: './src/components/ThemeSelect.astro',
				Pagination: './src/components/Pagination.astro'
      },
		}),
	],
	markdown: {
		// Opt-out of "smart quotes". We do not want our normal quotes to be changed.
		smartypants: false,
	}
});
