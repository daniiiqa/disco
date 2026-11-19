# disco

*El disco*: el comal de arado donde se cocina la discada. La base que comparten
las apps de [discada](https://github.com/daniiiqa/dotfiles) para verse como el
look del rice: los colores, el estilo y la fuente del look, y cómo dibujar como
la terminal.

Lo usan [rola](https://github.com/daniiiqa/rola),
[vato](https://github.com/daniiiqa/vato), [taco](https://github.com/daniiiqa/taco),
[arre](https://github.com/daniiiqa/arre),
[empalme](https://github.com/daniiiqa/empalme),
[jale](https://github.com/daniiiqa/jale),
[machaca](https://github.com/daniiiqa/machaca),
[nota](https://github.com/daniiiqa/nota),
[trompo](https://github.com/daniiiqa/trompo),
[lena](https://github.com/daniiiqa/lena) y las demás.

## Qué trae

| módulo | qué hace | feature |
|---|---|---|
| `look` | el look de ahora: colores, estilo y fuente; se recarga solo cuando cambia | siempre |
| `color` | `#rrggbb`, mezclas, HLS, luminancia y contraste WCAG (`lift` sube un color hasta el contraste que le pidas) | siempre |
| `paint` | dibujar celdas de ratatui en pixeles con la fuente de kitty, como una terminal: recuadros y barras unidos sin huecos entre celdas | `paint` |
| `icon` | íconos: una ruta (png, jpeg o svg) o un nombre del tema de íconos; los «symbolic» se pintan del color del look | `paint` |

## De dónde sale el look

- **Colores**: del theme de kitty que tenga puesto el look (el último `include`
  de `~/.config/kitty/kitty.conf`). Es la paleta propia del look o la que
  wallust saca del wallpaper; la escribe lena.
- **Estilo**: de `~/.config/<app>/look.json` o, si la app no tiene el suyo, del
  `look.json` de rola. Sale del `[style]` del `look.toml` de cada look:
  bordes (`rounded`, `plain`, `double`, `thick`, `dashed`, `none`), cómo se
  marca lo elegido (`fill`, `bar`, `inverse`), barras de progreso (`line`,
  `knob`, `blocks`, `dots`, `ascii`), mayúsculas, íconos o solo texto, y tinte
  (`color`, `phosphor`, `ink`).
- **Fuente y opacidad**: de kitty.

## Cómo se usa

```toml
[dependencies]
disco = { git = "https://github.com/daniiiqa/disco" }
# las apps que dibujan en pixeles (barras, menús, paneles):
# disco = { git = "https://github.com/daniiiqa/disco", features = ["paint"] }
```

```rust
use disco::look::Look;

let mut look = Look::load("miapp"); // lee ~/.config/miapp/look.json, o el de rola

// en cada vuelta del loop: si cambió el look, true y se vuelve a dibujar
if look.refresh() { /* redibujar */ }

let block = look.block(true);            // un recuadro con los bordes del look
let (estilo, marca) = look.sel(true);    // cómo se marca lo elegido
let titulo = look.tx("ajustes");         // con las mayúsculas del look
let (lleno, vacio) = look.bar(0.6, 20);  // una barra de progreso del look
```

```rust
use disco::color::{parse, contrast, lift};

let fondo = parse("#1e1e2e").unwrap();
let texto = parse("#585b70").unwrap();
let legible = lift(texto, fondo, 4.5, 0.1); // lo sube hasta 4.5:1 (WCAG AA)
assert!(contrast(legible, fondo) >= 4.5);
```

## Licencia

MIT
