# Dynamic blocks: integration architecture

This feature builds on OCS's existing parametric system. Do **not** add an independent parameter expression evaluator or constraint solver.

## Verified existing infrastructure

- `src/scene/named_parameters.rs`: `ParameterTable`, `DrivingValue`, formula parsing and dependency-cycle detection.
- `src/scene/parametric_constraints.rs`: persistent `ParametricScope` for model space and block definitions, with stable entity/subentity references.
- `src/scene/parametric_solve.rs`: geometry constraint solving.
- `src/scene/dwg_native_constraints.rs`: native DWG associative constraint objects; extend this adapter, rather than exporting opaque native-only behavior.
- `src/modules/parametric/`: UI and commands for constraints, dimensions and named parameters.
- `src/app/document.rs`: `current_parametric_scope()` already detects an active BEDIT block definition.
- `src/modules/draw/modify/block_edit/`: existing BEDIT editing sessions.
- `src/app/visibility.rs`: existing imported AutoCAD dynamic-block visibility grip and state switching through anonymous block members.
- `src/entities/insert.rs`: INSERT grips and properties; do not replace the underlying codec entity.
- `src/io/mod.rs`: DWG/DXF through `opencadcodec`.

## What is missing or needs verification

1. Block-specific **instance** parameter values: overriding one INSERT must not edit the master definition or another instance.
2. A behavior/action layer binding existing parameters to selected geometry, including stretch, translation, rotate, flip, array, and visibility.
3. A friendly BEDIT visual authoring workflow with direct selection/grips and small popup editors.
4. Persistent action serialization and lossless DWG/DXF roundtrip; must verify what the pinned `opencadcodec` version exposes for dynamic-block parameters/actions.
5. Instance-safe cache invalidation, undo/redo, and anonymous block identity.
6. Roundtrip fixtures authored in AutoCAD, with validation **in AutoCAD**, not simply geometrically identical static exports.

## Design constraints

- Keep a single parameter/constraint implementation; reuse `ParameterTable` and the existing geometry solver.
- Preserve `CadDocument` and INSERT identities; don't mutate shared block geometry to simulate a single instance variation.
- Store behaviors separately from drawing-global parameters until block-local namespace and persistence semantics are established.
- Changes need migration or graceful fallback for old drawings.
- No assertion of full AutoCAD compatibility without verified DWG/DXF roundtrip tests.
- Keep first edits isolated on `feature/dynamic-blocks-core`, not `main`.

## Initial implementation order

1. Unit tests characterizing BEDIT-scoped constraints and existing visibility behavior.
2. Establish instance identity, per-instance override storage and non-destructive geometry evaluation hooks, calling the existing parameter/constraint APIs.
3. Add stretch/rotate/visibility action bindings and regression tests for two inserts of the same block.
4. Connect to existing BEDIT and INSERT properties/grips.
5. Add DWG/DXF dynamic object adapters, then external AutoCAD tests.

## Acceptance scenarios

- Two instances of a door: changing width or opening on one leaves the other unchanged.
- Editing the block definition updates applicable inserts without losing their overrides.
- Undo/redo and reopen retain distinct instance state.
- Visibility states imported from AutoCAD remain available.
- Save/reopen in OCS retains dynamic behavior.
- Save/reopen in AutoCAD exposes genuine dynamic grips/actions before claiming bidirectional compatibility.
