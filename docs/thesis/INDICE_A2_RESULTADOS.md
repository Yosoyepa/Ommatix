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

- **Corrida definitiva:** 15 de octubre de 2026, de 04:12 a 04:38 (hora local). Un solo proceso, `candidate_index.exe` compilado en `--release`, un hilo de medición, sin pantalla ni entrada de teclado o ratón.
- **Commit medido:** `5c28ec4` (el mismo árbol aparece como `3f5e499` tras reajustar la rama sobre `origin/main`; el código Rust y los manifiestos son idénticos, sin líneas distintas). El protocolo quedó en `4289ed8`. Después sólo cambió el código del arnés, en `28cc215` (tope de tiempo por celda, descrito en §3.1), y los commits posteriores son documentación. El código Rust del medido es el de `28cc215`.
- **Compilador:** rustc 1.99.0 (b940084d7, 2026-09-28), perfil `release` del espacio de trabajo (`lto = "thin"`, `codegen-units = 1`).
- **Máquina:** AMD Ryzen 7 5700X (8 núcleos, 16 hilos), Windows 11 Pro 10.0.26300. Es la misma máquina de `S0_RESULTADOS.md` §11.
- **Carga antes de empezar:** CPU total entre 7 y 18 %, sin procesos `cargo` ni `rustc`. Durante la corrida no se muestreó la carga. La corrida anterior (§3.8) sí se hizo con otras sesiones compilando.
- **Comando:** `./target/release/examples/candidate_index.exe --out indice_a2.json --commit 5c28ec4 --reps 9`.
- **Salida:** [`indice_a2_medicion.json`](indice_a2_medicion.json), 63 981 bytes, SHA-256 `9443f4bf18cd0e2eba4449a7b170ba8b2514bd47ea12be3544ecb57e20b6bae8`. Contiene las celdas, las comprobaciones de equivalencia, las desviaciones y los estadísticos.

## 3. Resultados y decisión

### 3.1 Desviaciones del protocolo

Las hizo el coordinador para esta corrida, y el informe las registra en `deviations`:

1. **Tope de 15 minutos por celda.** Una celda que sigue en marcha a los 15 minutos se registra como *no medida (límite de tiempo)*, con las repeticiones completadas. Sólo la celda *IdenticalBoxes, 8 192 controles* lo alcanzó: 0 de 3 repeticiones y 2 de 4 comprobaciones de equivalencia.
2. **Tres repeticiones, no nueve, en las celdas de 8 192 controles de las familias cuadráticas** (*IdenticalBoxes* y *ExtremeOverlap*). *ExtremeOverlap, 8 192* se midió con tres repeticiones; con n = 3 el p95 es el máximo de tres.
3. **El arnés cambió después del compromiso del protocolo** (commit `28cc215`): tope por celda, comprobaciones de equivalencia una a una y escritura del informe celda a celda. No cambió lo que se mide ni los estadísticos.

Límites del protocolo que se cumplió tal cual, y que conviene leer con cuidado:

- Con nueve repeticiones el **p95 es el máximo** de las nueve (rango más próximo: ⌈0,95 · 9⌉ = 9). El criterio G5 compara así máximos de nueve repeticiones: es un criterio grueso.
- El intervalo del cociente de medianas usa 2 000 remuestreos de nueve pares: es orientativo.

### 3.2 Defecto de medición: la consulta del recorrido no es válida en la corrida definitiva

En la variante `scan`, la etapa de consultas se optimizó. El visitante `|i| sink = sink.wrapping_add(i)` sobre el rango `0..n` tiene forma cerrada, y el compilador lo sustituyó por una constante: por eso la columna «consultas recorrido» de §3.4 da microsegundos donde deberían darse milisegundos (8 562 palabras por 8 192 controles en la cuadrícula). **En §3.4, la columna «consultas recorrido» y el resto que se deriva por diferencia en el recorrido corresponden a la corrida anterior a la corrección, y no son válidas.** El total de `fuse` sí es válido: su resultado se usa, y sus comprobaciones no se pueden eliminar. La decisión se apoya sólo en totales. La construcción y las consultas del índice (`rstar`) son válidas: la consulta recoge y ordena los candidatos.

**Corrección (commit `b15fdf4`).** Cada posición visitada pasa por `black_box`, y así el recorrido se ejecuta. Una corrida de validación a 128 y 512 controles, con tres repeticiones y sin tope (no es la medición del protocolo; `indice_a2_validacion_black_box.json`, en el mismo commit), da la consulta del recorrido medida y coherente con el trabajo: entre 0,2 y 0,5 ns por posición visitada. Ejemplos a 512 controles (mediana, ms; consulta del recorrido / consulta del índice; construcción del índice):

| Familia | Palabras / brechas | Consulta del recorrido | Consulta del índice | Construcción del índice |
|---|---:|---:|---:|---:|
| Cuadrícula dispersa | 539 / 0 | 0,125 | 0,087 | 0,097 |
| Formularios de varias columnas | 801 / 998 | 0,312 | 0,221 | 0,092 |
| Contenedores anidados | 296 / 0 | 0,069 | 0,068 | 0,095 |
| Solapamiento extremo | 512 / 0 | 0,119 | 3,473 | 0,124 |
| Cajas idénticas | 257 / 33 024 | 4,257 | 57,054 | 0,088 |

La descomposición de la corrida definitiva a todos los tamaños no se repitió: queda marcada como medida antes de la corrección, y se repetirá con la celda de cajas idénticas de 8 192 controles (§3.10).
### 3.3 Equivalencia (G1 y G2)

- **178 comprobaciones completadas, ninguna con diferencia:** 44 celdas por 4 comprobaciones (umbrales de propiedad 0,75, 0 y 1, y propietarios con 0,75), más 2 de la celda *IdenticalBoxes, 8 192*.
- Faltan 2 de las 4 comprobaciones de esa celda: los umbrales 1,0 y los propietarios no se comprobaron a 8 192 controles. Es un hueco declarado.

### 3.4 Tabla de resultados

Costo geométrico total, es decir `fuse` completo, en milisegundos: mediana y p95 de las repeticiones. **Reducción** = 1 − mediana(índice) / mediana(recorrido); un valor positivo quiere decir que el índice es más rápido. IC 95 % por bootstrap sobre los pares de repeticiones. **Δ p95** = p95(índice) − p95(recorrido) del total, frente al **límite G5** (el mayor entre el 1 % del p95 del recorrido y 0,2 ms; sólo para n ≤ 128). La construcción y las consultas del índice son válidas; las del recorrido no (§3.2).

| Familia | n | Controles / palabras / corredores | Repeticiones | Recorrido (mediana / p95) | Índice (mediana / p95) | Reducción | IC 95 % | Construcción índice | Consultas recorrido / índice | Resto recorrido / índice | Δ p95 | Límite G5 | G4 | G5 | Equivalencia |
|---|---:|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---:|---|---|---|
| SparseGrid (objetivo) | 32 | 32 / 41 / 0 | 9 de 9 | 0.017 / 0.018 | 0.022 / 0.023 | -30.2 % | [-32.4, -27.5] | 0.004 | 0.000 / 0.004 | 0.017 / 0.014 | +0.005 | 0.200 | — | ok | 0 de 4 (de 4) |
| SparseGrid (objetivo) | 128 | 128 / 128 / 0 | 9 de 9 | 0.096 / 0.117 | 0.094 / 0.105 | 1.5 % | [-0.9, 5.5] | 0.017 | 0.000 / 0.014 | 0.096 / 0.064 | -0.012 | 0.200 | — | ok | 0 de 4 (de 4) |
| SparseGrid (objetivo) | 512 | 512 / 539 / 0 | 9 de 9 | 1.057 / 1.107 | 0.646 / 0.732 | 38.9 % | [34.3, 40.3] | 0.096 | 0.000 / 0.083 | 1.057 / 0.467 | -0.374 | 0.200 | cumple | — | 0 de 4 (de 4) |
| SparseGrid (objetivo) | 2048 | 2048 / 2126 / 0 | 9 de 9 | 13.881 / 14.152 | 5.146 / 5.328 | 62.9 % | [62.3, 63.8] | 0.421 | 0.001 / 0.381 | 13.880 / 4.352 | -8.824 | 0.200 | cumple | — | 0 de 4 (de 4) |
| SparseGrid (objetivo) | 8192 | 8192 / 8562 / 0 | 9 de 9 | 252.578 / 294.331 | 66.042 / 84.804 | 73.9 % | [68.0, 76.2] | 1.680 | 0.004 / 1.726 | 252.574 / 62.730 | -209.527 | 2.943 | cumple | — | 0 de 4 (de 4) |
| MultiColumnForm (objetivo) | 32 | 32 / 52 / 63 | 9 de 9 | 0.030 / 0.030 | 0.031 / 0.032 | -3.7 % | [-4.8, -2.3] | 0.003 | 0.000 / 0.006 | 0.029 / 0.021 | +0.002 | 0.200 | — | ok | 0 de 4 (de 4) |
| MultiColumnForm (objetivo) | 128 | 128 / 198 / 246 | 9 de 9 | 0.331 / 0.361 | 0.274 / 0.294 | 17.5 % | [15.9, 18.1] | 0.014 | 0.000 / 0.042 | 0.331 / 0.216 | -0.067 | 0.200 | — | ok | 0 de 4 (de 4) |
| MultiColumnForm (objetivo) | 512 | 512 / 801 / 998 | 9 de 9 | 4.816 / 5.016 | 3.454 / 4.040 | 28.3 % | [27.0, 32.5] | 0.083 | 0.001 / 0.218 | 4.816 / 3.159 | -0.976 | 0.200 | cumple | — | 0 de 4 (de 4) |
| MultiColumnForm (objetivo) | 2048 | 2048 / 3059 / 3867 | 9 de 9 | 113.309 / 145.071 | 63.479 / 79.962 | 44.0 % | [24.7, 59.1] | 0.379 | 0.002 / 1.088 | 113.304 / 61.331 | -65.109 | 1.451 | cumple | — | 0 de 4 (de 4) |
| MultiColumnForm (objetivo) | 8192 | 8192 / 12200 / 15424 | 9 de 9 | 1773.774 / 2113.258 | 1235.019 / 1413.449 | 30.4 % | [19.5, 41.8] | 2.125 | 0.009 / 7.386 | 1773.765 / 1228.112 | -699.808 | 21.133 | cumple | — | 0 de 4 (de 4) |
| ManyTextsPerControl | 32 | 2 / 34 / 0 | 9 de 9 | 0.010 / 0.011 | 0.014 / 0.014 | -32.7 % | [-33.0, -27.8] | 0.000 | 0.000 / 0.003 | 0.010 / 0.010 | +0.003 | 0.200 | — | ok | 0 de 4 (de 4) |
| ManyTextsPerControl | 128 | 8 / 136 / 0 | 9 de 9 | 0.044 / 0.044 | 0.057 / 0.058 | -31.4 % | [-33.2, -28.9] | 0.001 | 0.000 / 0.016 | 0.043 / 0.040 | +0.013 | 0.200 | — | ok | 0 de 4 (de 4) |
| ManyTextsPerControl | 512 | 32 / 544 / 0 | 9 de 9 | 0.238 / 0.241 | 0.254 / 0.274 | -6.6 % | [-10.6, -5.7] | 0.005 | 0.001 / 0.073 | 0.238 / 0.176 | +0.033 | 0.200 | — | — | 0 de 4 (de 4) |
| ManyTextsPerControl | 2048 | 128 / 2176 / 0 | 9 de 9 | 1.978 / 2.050 | 1.335 / 1.352 | 32.5 % | [31.4, 35.2] | 0.025 | 0.002 / 0.337 | 1.976 / 0.976 | -0.698 | 0.200 | — | — | 0 de 4 (de 4) |
| ManyTextsPerControl | 8192 | 512 / 8704 / 0 | 9 de 9 | 24.539 / 25.032 | 9.916 / 10.274 | 59.6 % | [58.7, 59.7] | 0.123 | 0.008 / 1.487 | 24.530 / 8.314 | -14.758 | 0.250 | — | — | 0 de 4 (de 4) |
| NestedContainers | 32 | 35 / 20 / 0 | 9 de 9 | 0.014 / 0.014 | 0.021 / 0.021 | -52.9 % | [-54.0, -51.1] | 0.006 | 0.000 / 0.004 | 0.014 / 0.012 | +0.007 | 0.200 | — | ok | 0 de 4 (de 4) |
| NestedContainers | 128 | 133 / 76 / 0 | 9 de 9 | 0.094 / 0.095 | 0.100 / 0.103 | -5.8 % | [-8.5, -2.2] | 0.026 | 0.000 / 0.017 | 0.094 / 0.056 | +0.008 | 0.200 | — | ok | 0 de 4 (de 4) |
| NestedContainers | 512 | 518 / 296 / 0 | 9 de 9 | 0.939 / 0.958 | 0.598 / 0.629 | 36.3 % | [34.5, 40.0] | 0.115 | 0.000 / 0.091 | 0.938 / 0.389 | -0.329 | 0.200 | — | — | 0 de 4 (de 4) |
| NestedContainers | 2048 | 2051 / 1172 / 0 | 9 de 9 | 12.605 / 12.704 | 4.892 / 4.988 | 61.2 % | [60.5, 61.9] | 0.508 | 0.001 / 0.402 | 12.603 / 3.952 | -7.716 | 0.200 | — | — | 0 de 4 (de 4) |
| NestedContainers | 8192 | 8197 / 4684 / 0 | 9 de 9 | 107.453 / 175.926 | 24.320 / 27.787 | 77.4 % | [77.2, 82.1] | 1.500 | 0.002 / 1.469 | 107.451 / 21.355 | -148.139 | 1.759 | — | — | 0 de 4 (de 4) |
| ExtremeOverlap | 32 | 32 / 32 / 0 | 9 de 9 | 0.020 / 0.020 | 0.037 / 0.038 | -87.4 % | [-89.8, -84.5] | 0.003 | 0.000 / 0.015 | 0.020 / 0.019 | +0.018 | 0.200 | — | ok | 0 de 4 (de 4) |
| ExtremeOverlap | 128 | 128 / 128 / 0 | 9 de 9 | 0.282 / 0.310 | 0.517 / 0.531 | -83.3 % | [-91.2, -78.5] | 0.017 | 0.000 / 0.223 | 0.282 / 0.277 | +0.221 | 0.200 | — | revisar | 0 de 4 (de 4) |
| ExtremeOverlap | 512 | 512 / 512 / 0 | 9 de 9 | 5.188 / 5.262 | 8.403 / 8.475 | -62.0 % | [-62.3, -58.5] | 0.090 | 0.000 / 3.381 | 5.187 / 4.888 | +3.213 | 0.200 | — | — | 0 de 4 (de 4) |
| ExtremeOverlap | 2048 | 2048 / 2048 / 0 | 9 de 9 | 101.430 / 115.263 | 153.496 / 193.221 | -51.3 % | [-59.2, -43.5] | 0.394 | 0.001 / 57.712 | 101.428 / 93.705 | +77.959 | 1.153 | — | — | 0 de 4 (de 4) |
| ExtremeOverlap | 8192 | 8192 / 8192 / 0 | 3 de 3 | 1993.629 / 2349.325 | 3009.943 / 3090.106 | -51.0 % | [-51.0, -31.5] | 2.514 | 0.007 / 1231.741 | 1993.625 / 1651.369 | +740.782 | 23.493 | — | — | 0 de 4 (de 4) |
| IdenticalBoxes | 32 | 32 / 17 / 144 | 9 de 9 | 0.019 / 0.020 | 0.037 / 0.038 | -92.2 % | [-92.8, -88.6] | 0.003 | 0.000 / 0.021 | 0.019 / 0.013 | +0.018 | 0.200 | — | ok | 0 de 4 (de 4) |
| IdenticalBoxes | 128 | 128 / 65 / 2112 | 9 de 9 | 0.618 / 0.688 | 1.255 / 1.273 | -102.9 % | [-103.3, -101.8] | 0.013 | 0.001 / 0.940 | 0.618 / 0.303 | +0.585 | 0.200 | — | revisar | 0 de 4 (de 4) |
| IdenticalBoxes | 512 | 512 / 257 / 33024 | 9 de 9 | 38.566 / 67.492 | 74.872 / 133.501 | -94.1 % | [-154.2, -39.8] | 0.084 | 0.009 / 59.019 | 38.557 / 17.430 | +66.010 | 0.675 | — | — | 0 de 4 (de 4) |
| IdenticalBoxes | 2048 | 2048 / 1025 / 525312 | 9 de 9 | 2473.579 / 2869.550 | 5777.789 / 6269.019 | -133.6 % | [-161.2, -113.4] | 0.334 | 0.114 / 4372.445 | 2473.465 / 1519.353 | +3399.470 | 28.695 | — | — | 0 de 4 (de 4) |
| IdenticalBoxes | 8192 | 8192 / 4097 / 8392704 | 0 de 3 (no medida: límite de tiempo) | — | — | — | — | — | — | — | — | — | — | 0 de 2 (de 4) |
| SharedLabels | 32 | 32 / 64 / 182 | 9 de 9 | 0.043 / 0.075 | 0.044 / 0.048 | -2.1 % | [-4.4, -1.1] | 0.003 | 0.000 / 0.017 | 0.043 / 0.025 | -0.027 | 0.200 | — | ok | 0 de 4 (de 4) |
| SharedLabels | 128 | 128 / 256 / 758 | 9 de 9 | 0.525 / 0.685 | 0.388 / 0.527 | 26.2 % | [20.0, 31.2] | 0.013 | 0.000 / 0.100 | 0.524 / 0.275 | -0.158 | 0.200 | — | ok | 0 de 4 (de 4) |
| SharedLabels | 512 | 512 / 1024 / 3062 | 9 de 9 | 8.302 / 13.665 | 4.835 / 6.079 | 41.8 % | [33.2, 45.7] | 0.078 | 0.001 / 0.595 | 8.300 / 4.165 | -7.586 | 0.200 | — | — | 0 de 4 (de 4) |
| SharedLabels | 2048 | 2048 / 4096 / 12278 | 9 de 9 | 121.946 / 126.521 | 66.697 / 77.368 | 45.3 % | [43.2, 45.7] | 0.281 | 0.004 / 3.244 | 121.942 / 63.197 | -49.153 | 1.265 | — | — | 0 de 4 (de 4) |
| SharedLabels | 8192 | 8192 / 16384 / 49142 | 9 de 9 | 2137.015 / 2614.602 | 1271.477 / 1758.387 | 40.5 % | [37.2, 51.8] | 1.368 | 0.018 / 17.568 | 2136.988 / 1242.378 | -856.216 | 26.146 | — | — | 0 de 4 (de 4) |
| SupportChains | 32 | 32 / 50 / 9 | 9 de 9 | 0.019 / 0.021 | 0.022 / 0.023 | -15.8 % | [-17.7, -12.7] | 0.003 | 0.000 / 0.004 | 0.019 / 0.015 | +0.002 | 0.200 | — | ok | 0 de 4 (de 4) |
| SupportChains | 128 | 128 / 195 / 32 | 9 de 9 | 0.153 / 0.154 | 0.126 / 0.127 | 17.9 % | [17.4, 18.5] | 0.013 | 0.000 / 0.023 | 0.153 / 0.090 | -0.028 | 0.200 | — | ok | 0 de 4 (de 4) |
| SupportChains | 512 | 512 / 774 / 131 | 9 de 9 | 1.867 / 2.024 | 1.058 / 1.186 | 43.3 % | [41.4, 43.8] | 0.060 | 0.000 / 0.128 | 1.866 / 0.866 | -0.838 | 0.200 | — | — | 0 de 4 (de 4) |
| SupportChains | 2048 | 2048 / 3051 / 493 | 9 de 9 | 32.476 / 42.543 | 12.098 / 22.447 | 62.7 % | [40.8, 69.7] | 0.301 | 0.002 / 0.634 | 32.474 / 11.170 | -20.096 | 0.425 | — | — | 0 de 4 (de 4) |
| SupportChains | 8192 | 8192 / 12287 / 2036 | 9 de 9 | 441.924 / 508.195 | 183.119 / 192.902 | 58.6 % | [57.2, 61.2] | 1.174 | 0.006 / 3.195 | 441.918 / 178.785 | -315.293 | 5.082 | — | — | 0 de 4 (de 4) |
| FractionalCoordinates | 32 | 32 / 46 / 70 | 9 de 9 | 0.027 / 0.028 | 0.028 / 0.029 | -4.4 % | [-7.2, -3.7] | 0.003 | 0.000 / 0.006 | 0.027 / 0.019 | +0.002 | 0.200 | — | ok | 0 de 4 (de 4) |
| FractionalCoordinates | 128 | 128 / 170 / 249 | 9 de 9 | 0.325 / 0.397 | 0.259 / 0.274 | 20.3 % | [19.3, 23.5] | 0.014 | 0.000 / 0.036 | 0.324 / 0.208 | -0.123 | 0.200 | — | ok | 0 de 4 (de 4) |
| FractionalCoordinates | 512 | 512 / 686 / 1031 | 9 de 9 | 4.994 / 5.054 | 3.357 / 3.620 | 32.8 % | [29.8, 33.0] | 0.072 | 0.001 / 0.200 | 4.993 / 3.087 | -1.435 | 0.200 | — | — | 0 de 4 (de 4) |
| FractionalCoordinates | 2048 | 2048 / 2678 / 3935 | 9 de 9 | 80.849 / 84.164 | 52.485 / 53.755 | 35.1 % | [34.5, 35.9] | 0.296 | 0.002 / 0.962 | 80.846 / 51.236 | -30.409 | 0.842 | — | — | 0 de 4 (de 4) |
| FractionalCoordinates | 8192 | 8192 / 10649 / 15560 | 9 de 9 | 1448.926 / 1629.264 | 970.253 / 1037.060 | 33.0 % | [31.9, 36.0] | 1.194 | 0.008 / 5.205 | 1448.918 / 963.719 | -592.204 | 16.293 | — | — | 0 de 4 (de 4) |

### 3.5 Lectura

- **Familias objetivo (cuadrícula dispersa y formularios de varias columnas).** Con 512 controles o más, el índice es más rápido en las dos: la cuadrícula gana 39 %, 63 % y 74 % (512, 2 048 y 8 192 controles, intervalos por encima de cero) y los formularios 28 %, 44 % y 30 %. A 32 controles el índice es más lento (cuadrícula −30 %, formularios −4 %), con diferencias absolutas de cinco microsegundos o menos. A 128 controles la cuadrícula está en paridad y los formularios ganan un 18 %.
- **Dónde gana.** En las familias no cuadráticas, con 512 controles o más, la reducción va del 33 % al 77 % (contenedores anidados llega al 77 % con 8 192). La única excepción de ese tamaño es *ManyTextsPerControl*, con −7 % a 512 controles.
- **Dónde pierde.** En *ExtremeOverlap*, donde todos los controles tocan todas las ventanas, el índice es más lento a todos los tamaños: −51 % a 8 192 controles y −87 % a 32. En *IdenticalBoxes* lo es en todos los medidos: −94 % a 512 y −134 % a 2 048. Allí la consulta domina el tiempo del índice: 4,4 s de los 5,8 s a 2 048 controles, porque cada ventana devuelve todos los candidatos y hay que ordenarlos. El recorrido, una pasada sin estructura, es más rápido.
- **Carga pequeña.** Con 32 y 128 controles las diferencias absolutas son de décimas de milisegundo, salvo en las dos familias de solapamiento: en *ExtremeOverlap*, 0,221 ms de Δ p95 frente a un límite de 0,200; en *IdenticalBoxes*, 0,585 ms.
- **Varianza.** La reducción de una misma celda cambia entre 2 y 35 puntos de una corrida a otra (§3.8). Por eso la conclusión se apoya en la dirección del efecto y en varias celdas, no en una cifra suelta.

### 3.6 Gates

- **G1 y G2 (equivalencia):** cumplidos en todas las comprobaciones completadas (178, cero diferencias). La celda *IdenticalBoxes, 8 192* tiene sólo 2 de 4.
- **G3 (consistencia):** IDX-09 e IDX-10 pasan en el motor (`src/application/src/engine/tests/candidates.rs`); el índice no se guarda entre observaciones.
- **G4 (rendimiento neto):** **cumplido** en las dos familias objetivo con 512 controles o más, con intervalos por encima de cero.
- **G5 (no regresión en carga pequeña):** **no cumplido.** En dos celdas de 128 controles el Δ p95 supera el límite: *ExtremeOverlap* (+0,221 ms) e *IdenticalBoxes* (+0,585 ms). Las demás celdas de 32 y 128 controles están dentro.
- **G6 (publicación):** esta página, con sus límites, sus desviaciones y el JSON.

### 3.7 Decisión

**El índice no se adopta como valor por defecto.** `scan` sigue siendo la referencia y el valor por defecto. `rstar` queda como experimento desactivado (`[engine] candidate_search = "rstar"`).

Razones, con los criterios de §1.7 y §1.8:

1. **G4 se cumple y G5 no.** Adoptar exige ambos. La combinación no está cubierta literalmente por §1.8, así que se aplica la lectura conservadora: no se adopta.
2. **El índice pierde en las cargas de solapamiento y de cajas idénticas, a todos los tamaños**, con pérdidas de hasta −134 %. Una adopción por defecto necesitaría un selector por tamaño y selectividad, y ese selector no está calibrado: los gates no fijan un umbral universal sin datos.
3. **Cuando gana, la ganancia es en cargas que no son las de Ommatix.** Las pantallas medidas tienen unos 75 elementos. En el núcleo de `S0_RESULTADOS.md` §11, con 300 controles y 600 fragmentos, la fusión cuesta unos 1,2 ms y el total del núcleo en caliente unos 3 ms. Ahí, una mejora del 40 % en la fusión son décimas de milisegundo por observación. Esta medición no demuestra una mejora de la observación completa ni de la latencia de la aplicación.

Lo que se publica: el índice reduce el costo geométrico de cargas grandes y selectivas (cuadrículas y formularios desde 512 controles), empeora las cargas de solapamiento y las de cajas idénticas, y no compensa como valor por defecto. Queda como experimento desactivado, con los mismos resultados que el recorrido.

### 3.8 Corrida anterior (no es la medición del protocolo)

Antes de la corrida definitiva se hizo otra del mismo código (commit `d3837fa`, sin tope), con otras sesiones compilando en la misma máquina. Se detuvo a mano durante la celda *IdenticalBoxes, 8 192*, tras más de una hora en ella, sin JSON. Sus cifras no forman parte de la medición del protocolo, pero muestran la varianza: *formularios, 512*: 15 % con intervalo [−4, 38], frente a 28 % en la corrida definitiva; *ExtremeOverlap, 512*: −97 % frente a −62 %; *IdenticalBoxes, 2 048*: −114 % frente a −134 %. La celda de formularios con 512 controles está en el límite del criterio G4: en la corrida anterior no lo cumplía. Se lee como resultado no firme.

### 3.9 Límites

- Las escenas son sintéticas: estudian comportamiento y escala, no la distribución de aplicaciones reales.
- Se mide el núcleo de fusión, no la observación completa: no incluye captura, detección, OCR, accesibilidad ni commit.
- Una máquina y una corrida definitiva. La carga durante la corrida no se muestreó.
- Nueve repeticiones (tres en las celdas de 8 192 controles de las familias cuadráticas): los p95 son máximos y los intervalos son orientativos.
- La descomposición del recorrido no es válida (§3.2).

### 3.10 Trabajo pendiente

- Repetir la descomposición de la corrida definitiva a todos los tamaños, ya con el visitante corregido (`b15fdf4`); hoy sólo se valida a 128 y 512 controles.
- Medir la observación completa (`Engine::observe_screen`) antes de afirmar nada sobre la latencia de Ommatix.
- Calibrar un selector por tamaño y selectividad, si alguna vez se quiere adoptar el índice por defecto.
- Repetir la celda *IdenticalBoxes, 8 192* sin tope, para completar sus cuatro comprobaciones de equivalencia.
