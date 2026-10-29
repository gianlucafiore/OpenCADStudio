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
cd ~/Escritorio/OCS/OpenCADStudio
git status -sb
git remote -v
git fetch origin
git switch perf/cached-mass-objects || git switch -c perf/cached-mass-objects --track origin/perf/cached-mass-objects
git pull --ff-only origin perf/cached-mass-objects
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

## Optimización integrada del 29/10: navegación, selección y comandos

- La caché GPU de bloques ya reutiliza las posiciones de INSERT; se agregó una
  caché **CPU por operación** que evita recalcular esas posiciones para cientos
  de segmentos de una misma definición. `reused_placement_groups` permite
  comprobar cuántos grupos reutilizaron ese vector.
- Durante PAN/ZOOM y previews interactivas de escenas masivas, el renderizador
  usa **LOD transitorio** (muestra distribuida de segmentos de bloque y omite
  superposición azul duplicada). La selección y geometría reales permanecen
  íntegras y al detener PAN/ZOOM se dibuja la geometría completa.
- MOVE/COPY/ROTATE/SCALE/MIRROR usan una muestra geométrica de hasta 2.048 wires
  y un marco de la selección completa en las operaciones gigantes. ARRAY
  también limita el total conjunto de posiciones y wires mostrados. **Es una
  previsualización parcial intencional**, no una selección parcial: commit,
  guardar, undo y transformaciones afectan el conjunto completo.
- Para comparar con previews antiguas sin muestreo se puede lanzar
  `OCS_FULL_MASS_PREVIEW=1 PERF=1 ./target/release/OpenCADStudio`.
  No es recomendable para una sesión de 50.000 objetos.

### Validación conjunta después de bajar cambios

```bash
cargo test --lib pooled_block_placements_require_identical_sources_and_offsets -- --nocapture
cargo test --lib repeated_block_placements_reuse_one_instance_stream -- --nocapture
cargo test --lib massive_wire_preview_is_bounded_without_affecting_small_edits -- --nocapture
cargo test --lib massive_array_preview_budget_accounts_for_all_copies -- --nocapture
cargo test --lib mass_array_copy_count_is_bounded -- --nocapture
cargo test --lib mass_move_preview_is_bounded_but_commit_keeps_every_handle -- --nocapture
cargo test --lib hatch_free_insert_selection_does_not_walk_the_hatch_graph -- --nocapture
cargo test --lib stress_50k_lines_select_and_transform_without_losing_entities -- --ignored --nocapture
cargo build --release
PERF=1 ./target/release/OpenCADStudio 2>&1 | tee ~/ocs-perf-cache-integral.log
```

### Verificación visual antes de considerar cerrado

1. Abrir `test plantas.dwg`, seleccionar 54.433 INSERT por caja cruzada,
   panear y hacer zoom. Durante el movimiento la geometría puede verse
   simplificada; al detenerse debe reaparecer COMPLETA y correctamente resaltada.
2. Ejecutar MOVE/COPY/ROTATE/SCALE/MIRROR con 54k INSERT; mover cursor,
   confirmar, hacer UNDO/REDO y verificar todas las copias, hatch y posición final.
3. ARRAY rectangular/polar/por recorrido con selección grande; el preview es
   representativo, **la geometría final debe contener todas las copias**.
4. Repetir con 50k líneas, bloques mixtos con hatch, OSNAP/OTRACK, y comparar
   latencia con selección vacía. Guardar, cerrar y reabrir el DWG.
5. Revisar especialmente `wire-highlight`, `nav-prepare`,
   `command-preview-compute`, `input-to-render`, `block-wire-geometry` y
   `gpu-live` para medir mejoras y detectar regresiones.

**Estado:** cambios subidos; sin compilación o pruebas de interfaz ejecutadas
en el entorno de GitHub disponible desde esta sesión. La aceptación requiere
los tests y la inspección visual anteriores.


## 29/10: corrección del lag residual en Intel Vulkan

Registro de referencia: 54.433 INSERT, 772 segmentos distintos, 42.021.504
instancias potenciales y 23 MiB de buffers GPU. PAN y ZOOM todavía tenían
esperas de ~430 ms aunque `frame-prepare` era <1 ms, porque el LOD de bloques
se activaba a través de `skip_hatch` y ese flag solo estaba habilitado si
`OCS_HATCH_LOD` estaba activo. El DWG no tenía hatches.

Cambios implementados:

- `navigation_lod` separado de `skip_hatch`; ahora se activa para Model
  durante PAN/ZOOM incluso si el documento no tiene HATCH. Se conserva la
  configuración original de supresión de rellenos.
- Presupuesto transitorio para geometría de bloques: 350.000 segmentos
  referenciados por frame. En el dibujo de referencia resulta en un stride de
  121 de 772 batches. La GPU mantiene las posiciones/segmentos residentes y
  recupera todas las geometrías al terminar de navegar.
- `wire-nav-lod` registra `active`, `preview`, `segments` y `stride`.
  Verificar que durante PAN/ZOOM **active=true** y stride>1; si no, el LOD
  sigue sin entrar en la ruta de dibujo realmente usada.
- El atlas de posiciones compartidas tiene ahora un id de memo por operación:
  evita recalcular 42M hashes y comparaciones byte a byte al preparar el
  resaltado seleccionado, así como al actualizar el dibujo después de editarlo.
- Se evita la clasificación genérica de 327k wires si todos son instancias
  de bloques.

**Validación:** no asumir resolución sin probar `cargo test --lib`, compilar
`--release`, comprobar FPS y revisar la restauración de geometría y resaltado
tras dejar de panear, hacer zoom, MOVE, copiar, rotar, escalar, hacer MIRROR y
guardar/reabrir el DWG.

Pruebas específicas adicionales:

```bash
cargo test --lib hatch_free_42m_segment_drawing_uses_navigation_budget -- --nocapture
cargo test --lib memoized_block_runs_alias_only_with_the_same_memo_and_chunk -- --nocapture
cargo test --lib repeated_block_placements_reuse_one_instance_stream -- --nocapture
```

En el log nuevo se espera observar `wire-nav-lod active=true` durante la
navegación y que `wire-highlight` y `wire-upload` dejen de tardar segundos
en las mismas operaciones. Esas mejoras son hipótesis de rendimiento
instrumentadas y todavía no medidas en la máquina del usuario.
