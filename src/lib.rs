//! disco: la base del platillo. Lo que comparten las apps de discada (rola, vato, arre, taco y
//! empalme) para verse como el look del rice.
//!
//! - `look`: colores, estilo y fuente del look (siempre)
//! - `color`: `#rrggbb`, mezclas y contraste WCAG (siempre; lo usa lena)
//! - `paint`, `icon`: dibujar celdas de ratatui en pixeles con la fuente de
//!   kitty, como una terminal, e íconos (con la feature `paint`; las apps de
//!   terminal no la necesitan)

pub mod color;
pub mod look;

#[cfg(feature = "paint")]
pub mod icon;
#[cfg(feature = "paint")]
pub mod paint;
