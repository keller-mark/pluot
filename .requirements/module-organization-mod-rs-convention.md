# Keep Module Root Files Minimal

- When a module's root file grows large, move its logic into a submodule file and keep the root file limited to just re-exporting/declaring submodules, matching the existing convention elsewhere in the project.
