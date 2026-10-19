# Entrada real y control del operador

[English](live-input.md) · [Configuración](configuration.es.md)

> [!CAUTION]
> La entrada real está apagada por defecto. Estas opciones son del operador; el texto de
> pantalla y el agente nunca conceden permisos.

Todo falla de forma segura por defecto: sin archivo de configuración, la plantilla niega la
actuación, oculta los valores y no carga ningún modelo; cada acción es un dry run salvo que
el agente envíe `dry_run: false`; y ninguna opción de la línea de órdenes relaja `[safety]`.
La entrada real exige **todo** lo siguiente:

1. `--input sendinput` con `--source dxgi`, en un binario de Windows con `sendinput`;
2. `[safety] actuation_enabled = true` y una lista `[safety] allowed_applications` no vacía
   de nombres de ejecutable en minúsculas (`"notepad.exe"`), ambas en la configuración del
   operador;
3. `dry_run: false` del agente, sobre un objetivo del snapshot vigente.

La línea de órdenes rechaza un adaptador de entrada real antes de abrir ningún adaptador si
no se cumplen 1 o 2. En cada acción, el motor comprueba después la aplicación en primer plano
que atestigua el actuador contra `allowed_applications`, vuelve a capturar la pantalla y
rechaza (`STALE_SNAPSHOT`) si ya no coincide con el snapshot, lee la parada del operador y
vuelve a atestiguar la aplicación en primer plano (rechaza con `POLICY_DENIED` si la parada
está activa o la aplicación cambió), y sólo entonces entrega. El actuador `SendInput`
rechaza además un proceso en primer plano con un nivel de integridad mayor (UIPI
descartaría la entrada en silencio), un punto fuera de las ventanas de ese proceso y teclas
modificadoras o botones del ratón pulsados; en `type`, tras el clic que enfoca el campo,
comprueba que el foco de teclado está en la ventana bajo el punto objetivo (o en una que la
contiene) del mismo proceso en primer plano y, si no, no envía ninguna tecla. Con
`[models] accessibility = "uia"` el motor entrega `type` en dos fases
([ADR-0018](../adr/0018-two-phase-type-with-element-level-focus-confirmation.md)): envía
el clic, consulta con su reloj hasta que UI Automation informa el foco de teclado en el
elemento objetivo y sólo entonces envía las teclas; sin confirmación dentro del plazo de
verificación no hay teclas. Ese rechazo se informa como `indeterminate` y nunca se
reintenta: en `type` el clic puede haberse entregado ya. `--input native` (o `[engine] mode = "native"`) se
sigue aceptando con `--source screen` y `actuation_enabled = true`, pero `enigo` no puede
atestiguar la aplicación en primer plano, así que sus acciones reales son siempre
`POLICY_DENIED`.

**Parada del operador.** Mientras la entrada real es posible, `serve` escribe en stderr una
línea `ACTIVE CONTROL` con las aplicaciones permitidas y cómo detenerla. Con `[safety]
stop_file = "STOP"` (relativo al archivo de configuración), crear ese archivo detiene la
entrada real: el motor lo lee antes de cada acción y otra vez justo antes de entregar,
rechaza las acciones nuevas con `POLICY_DENIED` (`next_step: operator_review`) sin enviar
ningún evento y escribe una línea en stderr al activarse o retirarse la parada; borrar el
archivo la retira. Las acciones ya entregadas conservan el estado informado (repetir su
`request_id` sigue diciendo `input_delivered`); no se deshace nada. Ninguna herramienta MCP
puede activar ni retirar la parada. Terminar el proceso (Ctrl+C) también detiene; sin
`stop_file`, es la única parada local.

**Purga del operador** ([ADR-0028](../adr/0028-operator-purge-file.md), aprobada; apagada por
defecto). Con `[safety] purge_file = "PURGE"` (relativo al archivo de configuración), crear ese
archivo hace que el motor borre su estado y su caché en su siguiente petición, sin terminar la
sesión; ningún identificador se emite otra vez después. El motor atiende cada instancia del archivo
una sola vez (una fecha de modificación o un tamaño nuevos son otra instancia), escribe una línea
en stderr y nunca borra el archivo: para volver a pedirla, el operador lo retira, deja pasar una
petición y lo vuelve a crear. Ninguna herramienta MCP puede pedir la purga, y el agente sólo ve el
`STALE_SNAPSHOT` habitual en su siguiente acción sobre el snapshot anterior.

**Alcance del operador por ventanas** ([ADR-0020](../adr/0020-operator-scope-on-windows.md),
aprobada; apagado por defecto). `allowed_applications` nombra un proceso, y `explorer.exe`
es también dueño del escritorio y de la barra de tareas. `[safety] allowed_window_classes =
{ "explorer.exe" = ["CabinetWClass"] }` limita una aplicación a las clases de ventana de la
lista, y `[safety] window_pin_file` nombra un archivo, que escribe el operador o el arnés que
ejecuta, con el identificador de la única ventana permitida. El motor comprueba la ventana
en primer plano y la ventana bajo el punto de la acción (clase e identificador, como los
atestigua el actuador, nunca un título) en la puerta y otra vez justo antes de entregar; lo
que no se pueda atestiguar se rechaza con `POLICY_DENIED`. `applicationframehost.exe` nunca se
permite, y `serve` avisa si `explorer.exe` está permitido sin ninguna de las dos.



## Entrega y efecto

`simulated` no afirma entrega; `input_delivered` no afirma efecto; `indeterminate` nunca
inicia un reintento automático. La verificación actual comprueba `visual_change` dentro de
`verify_timeout_ms`, con sondeos cada `verify_poll_ms`; los demás efectos no comprobables
conservan la incertidumbre. Toda entrega agota el snapshot; una acción nueva necesita una
nueva observación. Fuentes: [ADR-0007](../adr/0007-fail-closed-actuation-and-honest-simulation.md),
[arquitectura](../architecture/overview.es.md) y [contrato](../thesis/07_CONTRATOS_MCP_Y_ERRORES.md).
