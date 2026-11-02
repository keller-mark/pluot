---
"@pluot/core": patch
"@pluot/react": patch
---

Strongly type the per-layer picking and brushing results: `info` is now a layer-specific struct (e.g., `PointPickingInfo`) with TypeScript types generated via ts-rs, rather than a string-to-string map. Numeric values are now numbers in their native dtype, bitmap/bitmask channel values are returned as a `channels` array instead of `channel_<i>` keys, and `LayerBrushingResult.element_info` is removed.
