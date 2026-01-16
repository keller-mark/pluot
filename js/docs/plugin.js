import react from '@astrojs/react';
export default {
    name: 'plugin-using-react',
    hooks: {
        'config:setup'({ addIntegration, astroConfig }) {
            const isReactLoaded = astroConfig.integrations.find(({ name }) => name === '@astrojs/react');
            // Only add the React integration if it's not already loaded.
            if (!isReactLoaded) {
                addIntegration(react());
            }
        },
    },
};
