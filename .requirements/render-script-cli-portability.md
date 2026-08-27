# Portable CLI Render Scripts

- Generated render scripts must invoke the command-line renderer as an installed package (e.g. from crates.io), not a local build from within the project checkout.
- Scripts must not depend on an environment variable pointing at the CLI, and must not assume they run from inside the project's own repository.
- Include a comment in the generated script explaining how to install the CLI.
