# Índice A2 (R*-tree): protocolo, medición y decisión

**ADR-0027 · INC-05 del plan del próximo corte · experimento desactivado por defecto**

Esta página fija el protocolo **antes** de medir (sección 1) y registra después la medición y la decisión (sección 3). El índice enumera los candidatos de propietarios y de obstáculos del corredor del Algoritmo 2 (capítulo 04 §4.3). `scan` es la referencia y el valor por defecto; `[engine] candidate_search = "rstar"` activa el índice. Ninguna cifra de esta página es de la aplicación de prueba ni de una pantalla: son mediciones del núcleo de fusión sobre escenas sintéticas generadas.

## 1. Protocolo fijado antes de medir

### 1.1 Qué se compara

El experimento E-INDEX de `04_Protocolo_y_Gates.md` (del plan del próximo corte): la variante I0 es el recorrido completo (`scan`) y la variante I1 es el R*-tree (`rstar`), con las mismas entradas geométricas y la misma semántica. No se mide la incrementalidad (A1), ni la indexación de palabras, ni la observación completa.

### 1.2 Escenas

Generadas por `src/domain/tests/support/families.rs`, un generador determinista (SplitMix64, sin dependencias) compartido por las pruebas diferenciales del dominio y por la medición, de modo que ambas miden las mismas escenas. Familias, según la propuesta del protocolo:

| Familia | Qué contiene |
|---|---|
| Cuadrícula dispersa (`SparseGrid`) | Botones en una cuadrícula con huecos; la mayoría con una palabra dentro, algunas palabras en los huecos |
| Formularios de varias columnas (`MultiColumnForm`) | Campos en tres columnas, cada uno con su etiqueta a la izquierda y a veces una encima |
| Muchos textos por control (`ManyTextsPerControl`) | Botones grandes con dieciséis palabras cada uno y una palabra que cruza su borde |
| Contenedores anidados (`NestedContainers`) | Panel, tres botones dentro y un icono dentro de cada botón |
| Solapamiento extremo (`ExtremeOverlap`) | Controles de tamaños aleatorios alrededor de un mismo punto |
| Cajas idénticas (`IdenticalBoxes`) | Cuatro cajas distintas repetidas, con palabras que coinciden exactamente con ellas |
| Etiquetas compartidas (`SharedLabels`) | Campos en una columna, cada uno con dos etiquetas casi igual de plausibles |
| Cadenas de soportes (`SupportChains`) | Controles que se solapan con su vecino en una fila |
| Coordenadas fraccionarias (`FractionalCoordinates`) | El formulario a escala 1,25 y con desplazamientos fraccionarios |

Tamaños en controles: **32, 128, 512, 2048 y 8192** (el número de palabras es del mismo orden). Semilla de cada celda: `0x2026_1015` XOR el tamaño.

### 1.3 Familias objetivo (fijadas aquí)

Las **familias objetivo** del criterio G4 son la **cuadrícula dispersa** y los **formularios de varias columnas**: consultas selectivas (cada ventana alcanza pocos controles) y tamaño alto. Las demás familias son de control o de estrés (solapamiento, cajas idénticas, etiquetas compartidas): en ellas se espera poca o ninguna ventaja, y se publican igual.

### 1.4 Qué se mide

Por repetición, con la misma escena para las dos variantes:

- **Costo geométrico total:** `fuse` completo. En la variante indexada incluye la construcción del índice.
- **Construcción:** `ControlSearch::new` con el índice, por separado.
- **Consultas:** solo la enumeración de candidatos: la ventana de propietario de cada palabra y la ventana de corredor de cada control con etiqueta y cada palabra libre cuyo coste de etiqueta está en rango.
- **Resto:** total − construcción − consultas, derivado dentro de cada repetición. Es el refinamiento exacto, el orden y el resultado. No se suman percentiles de etapas.

### 1.5 Diseño de la medición

- Compilación `--release` del espacio de trabajo (`lto = "thin"`, `codegen-units = 1`), un solo proceso, sin pantalla, sin ventanas y sin entrada de teclado o ratón.
- Dos repeticiones de calentamiento por variante, no registradas. **Nueve repeticiones registradas por celda** (n = 9).
- El orden de las dos variantes en cada repetición alterna, para que ninguna vaya siempre en segundo lugar.
- Medianas (media de los dos valores centrales cuando el número es par) y p95 por rango más próximo.
- Intervalo al 95 % del cociente de medianas `mediana(índice) / mediana(recorrido)`, por bootstrap de 2 000 remuestreos de las **repeticiones pareadas** (la unidad independiente es la repetición: un par de mediciones sobre la misma escena, no una observación), con semilla fija.
- Comando: `cargo run --release -p ommatix-acceptance --example candidate_index -- --out <json> --commit <sha> --reps 9`.

### 1.6 Equivalencia (G1 y G2)

En cada celda, las dos variantes deben dar la **fusión completa idéntica** (propietarios, etiquetas, huérfanos, orden de cada lista) con umbral de propiedad 0,75 (la referencia), 0 y 1, y margen de etiqueta 0,15; y los propietarios idénticos con umbral 0,75. Cualquier diferencia bloquea la adopción, sin importar el tamaño ni la familia.

### 1.7 Gates

Los umbrales son los de `04_Protocolo_y_Gates.md`, aprobados por el responsable con la ADR-0027.

- **G1 Completitud y G2 Semántica:** cero diferencias en todas las celdas.
- **G3 Consistencia:** un índice no sobrevive a la fusión que lo construye; se comprueba con IDX-09 e IDX-10.
- **G4 Rendimiento neto:** en las familias objetivo y con n ≥ 512, la reducción de la mediana del costo geométrico total es de al menos el **20 %**, y su intervalo al 95 % queda por encima de cero.
- **G5 No regresión práctica:** con n = 32 y 128, el p95 del costo total del índice no supera al del recorrido en más de **max(1 %, 0,2 ms)**. Las cifras son del núcleo de fusión: si el núcleo cumple, la observación también, porque su límite es igual o mayor.
- **G6 Publicación:** esta página, con sus límites y sus comandos.

### 1.8 Decisión prevista

- Si G1 o G2 fallan en alguna celda, el índice no se adopta: se investiga el contraejemplo y no se edita el oráculo para que coincida.
- Si G4 se cumple en las dos familias objetivo y G5 no se incumple, el índice queda como experimento desactivado. Adoptarlo por defecto requiere una decisión posterior, con medición de la observación completa; esta ADR no la toma.
- Si G4 no se cumple, el índice queda desactivado y el resultado negativo se publica tal cual.

### 1.9 Límites declarados de antemano

- Las escenas son sintéticas: estudian comportamiento y escala, no la distribución de aplicaciones reales.
- Se mide el núcleo de fusión, no la observación: no incluye captura, detección, OCR, accesibilidad ni confirmación.
- Una sola máquina (la de S0, §11), sin medición de energía ni de temperatura.

## 2. Medio de la medición

Pendiente: se completa con la salida de la medición (máquina, compilador, commit y hora).

## 3. Resultados y decisión

Pendiente: se completa tras la medición, con su commit y su JSON.
