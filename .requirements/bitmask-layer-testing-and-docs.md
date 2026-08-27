# Bitmask Testing & Documentation Fidelity

- Tests should exercise mask shapes drawn from the real documentation example, not only synthetic test data, including multi-channel variants and different array shapes.
- Include at least one non-simply-connected mask shape (e.g. a ring with a hole) to properly exercise outline rendering.
- Tests should default to an identity camera/view as the general-behavior baseline; zoomed-in/zoomed-out camera behavior should be covered by separate, dedicated tests rather than mixed into general tests.
- Documentation examples should demonstrate multiple stroke-width unit modes, and provide a UI control to toggle between filled and outlined rendering.
