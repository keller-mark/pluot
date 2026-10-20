---
name: pluot-layer-parameter-principles
description: Use when adding fields/properties/members to layer parameter structs.
---

## Option types

Unless a property absolutely must be specified explicitly, prefer Option types in which `None` is the default value of the property, such that `None` is a sentinel value which is interpreted to mean "I want the default behavior for this property".
