# pluot docs

From the root of the repo:

```sh
# nvm use 24 # if needed
wasm-pack build pluot_core --dev --target web && pnpm run start-docs
```

### Troubleshooting
May need to run:

```sh
# Reference: https://github.com/withastro/astro/issues/5711
pnpm run astro-sync
```
## Deploy

Deployment is performed automatically via GitHub Actions.

For manual deployment, run:

```sh
pnpm run deploy
```
