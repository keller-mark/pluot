# Snapshot Testing Conventions

- Snapshot tests should write actual output to a "dirty" snapshot location and compare against a separately maintained "blessed" (approved) location, consistent with existing snapshot tests in the project.
- Expected shader output should be checked against blessed snapshot files rather than inlined as strings in test source.
- Blessed snapshot filenames should be prefixed by the feature/layer they belong to.
- When a behavior change is expected to affect many existing snapshots, update the test setup, regenerate outputs, and bless the new snapshots afterward rather than manually recomputing each expected value.
