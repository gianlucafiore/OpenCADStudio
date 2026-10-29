# Rendimiento masivo: cachés y selección (rama permanente)

Rama: `perf/cached-mass-objects`. Base: `HakanSeven12/OpenCADStudio@dfe3d7f` (2026-10-29).

## Alcance

Optimizar edición de 5.000, 20.000 y 50.000 entidades sin una segunda vía de rasterización. Trabajar sobre la caché de teselación residente, índice espacial, caché de bloques, superposiciones hatch y control de invalidaciones. Mantener exactitud de geometría, orden de dibujo, referencias a bloques, snaps, OTRACK, undo/redo y guardado.

**Importante:** tener tests de 50.000 handles no demuestra que 50.000 geometrías se muevan a 60 FPS. La fluidez solo puede verificarse ejecutando la app en hardware real, y la versión actual todavía genera geometría de preview en CPU por movimiento del cursor.

## Cambios iniciales

- **Selección y borrado:** compresión de orden de selección una sola vez después de borrar un lote, evitando una secuencia cuadrática de `Vec::retain`.
- **Selección por ventana, QSELECT, automatización, relanzamiento, copia y pegado:** llamadas de selección/desselección en lote en vez de una invalidación de estado por entidad.
- **Caché de selección ordenada:** instantánea invalidada cuando cambia el conjunto seleccionado, no al sobrevolar.
- **Caché de firma de hatch/INSERT seleccionados:** no buscar entre miles de líneas en cada cuadro para decidir si hay rellenos seleccionados.
- **GRIPOBJLIMIT:** comprobar el umbral antes de resolver todas las entidades seleccionadas.
- **Fuente de previsualización:** reutilizar la teselación residente si coincide exactamente su guard de visualización. Fallback a teselación individual para elementos editados, papeles y contextos distintos.

## Preparación para probar en Linux

```bash
cd ~/Escritorio/OCS/OpenCADStudio-HatchTest
git status -sb
git remote -v
git fetch origin
git switch perf/cached-mass-objects || git switch -c perf/cached-mass-objects --track origin/perf/cached-mass-objects
git pull --ff-only origin perf/cached-mass-objects
cargo fmt --all -- --check
cargo check
cargo test --lib large_selection_order_tracks_bulk_add_remove_without_duplicate_rebuild -- --nocapture
cargo test --lib selection_order_snapshot_invalidates_on_changes_but_not_hover -- --nocapture
cargo test --lib selected_hatch_signature_cache_tracks_hatch_membership -- --nocapture
cargo test --lib edit_preview_reuses_resident_compatible_wire_geometry -- --nocapture
cargo build --release
PERF=1 ./target/release/OpenCADStudio 2>&1 | tee /tmp/ocs-perf-cache-50k.log
```

> Si el repositorio local lleva otro nombre, entrar al directorio correcto antes de ejecutar. Si existen modificaciones locales sin confirmar, no hacer `reset --hard`, `clean -fd` ni un `pull` que las destruya.

## Matriz de pruebas manuales

| Escenario | Tamaño | Operaciones | Verificar |
| --- | --- | --- | --- |
| Líneas sueltas | 5k, 20k, 50k | Ventana, quitar selección, MOVE, COPY, ROTATE, SCALE, MIRROR, ARRAY | Primer click, arrastre continuo, precisión, ESC, undo |
| Polilíneas + arcos | 5k, 20k, 50k | Selección cruzada, MOVE, ROTATE, SCALE, MIRROR | Puntos de snap y polar/OTRACK sin congelar |
| INSERT sin hatch | 5k, 20k, 50k | MOVE, COPY, MIRROR, ARRAY | Referencias y geometría correcta, no expandir a millones de instancias innecesarias |
| INSERT con hatch | 5k, 20k, 50k | Todas las transformaciones | Fill y contorno sincronizados, hatch no desaparece |
| Hatch independientes | 5k, 20k, 50k | Seleccionar, mover, rotar, escalar, espejar | Orden de dibujo, límites, patrón y carga GPU |
| Mezcla heterogénea | 50k | Selección, deselección, edición, zoom con OSNAP, guardar y reabrir | No bloquear ni corromper el archivo |

Medir por operación: (1) tiempo hasta primera previsualización, (2) latencia cursor→render P50/P95, (3) intervalos máximos congelados, (4) uso RAM/VRAM, (5) tiempos de confirmación y undo. Comparar aceleración GPU y fallback, y repetir con OSNAP y OTRACK habilitados.

## Criterios de aceptación

1. Ninguna regresión de selección exacta, hatch, block references, undo/redo, snaps, polar, OTRACK o archivos.
2. Sin bucles por elemento que recomputen el conjunto completo o invaliden la misma caché miles de veces.
3. Cachés reutilizadas únicamente con identificador de contexto y generación válidos.
4. Medición con archivo representativo y perfiles CPU/GPU. No declarar la meta de 50.000 objetos conseguida hasta pasar la matriz.
5. No reintroducir preview rasterizada. La siguiente etapa de rendimiento real exige estudiar transformaciones GPU/instancias de geometría de previsualización sin tessellate/clone/upload en cada pixel del cursor.
