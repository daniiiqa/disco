//! Colores: `#rrggbb`, mezclas y contraste (WCAG). La misma matemática para
//! lena (que arma los colores de cada look) y las apps que la necesiten.
//!
//! Un color es `[r, g, b]` en 0..=255 como `f64`: así las mezclas no se
//! redondean a medio camino, solo al escribirlas con [`hex`].

pub type Rgb = [f64; 3];

/// `#rrggbb` o `rrggbb` (con espacios alrededor) → color.
pub fn parse(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).unwrap() as f64;
    Some([n(0), n(2), n(4)])
}

/// `rrggbb` en minúsculas, o `None` si no es un color (como lo escribe un look.toml).
pub fn norm(s: &str) -> Option<String> {
    parse(s).map(|c| hex(c)[1..].to_string())
}

fn byte(v: f64) -> u8 {
    // redondeo al par (como Python): 0.5 → 0, 1.5 → 2
    v.clamp(0.0, 255.0).round_ties_even() as u8
}

/// `#rrggbb` en minúsculas.
pub fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", byte(c[0]), byte(c[1]), byte(c[2]))
}

/// `t` de `a` sobre `b` (t = 1 → a, t = 0 → b).
pub fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [0, 1, 2].map(|i| a[i] * t + b[i] * (1.0 - t))
}

fn lum01(c: [f64; 3]) -> f64 {
    let ch = |v: f64| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * ch(c[0]) + 0.7152 * ch(c[1]) + 0.0722 * ch(c[2])
}

fn to01(c: Rgb) -> [f64; 3] {
    c.map(|v| v / 255.0)
}

/// Luminancia relativa (WCAG), 0 negro … 1 blanco.
pub fn luminance(c: Rgb) -> f64 {
    lum01(to01(c))
}

fn ratio01(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (la, lb) = (lum01(a), lum01(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Contraste WCAG entre dos colores (1 … 21; texto normal pide 4.5).
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    ratio01(to01(a), to01(b))
}

/// El texto que mejor se lee sobre `color`: `light` o `dark`.
pub fn on(color: Rgb, light: Rgb, dark: Rgb) -> Rgb {
    if contrast(color, light) >= contrast(color, dark) { light } else { dark }
}

/// RGB (0..1) → HLS, igual que `colorsys` de Python.
pub fn rgb_to_hls([r, g, b]: [f64; 3]) -> (f64, f64, f64) {
    let (maxc, minc) = (r.max(g).max(b), r.min(g).min(b));
    let l = (maxc + minc) / 2.0;
    if minc == maxc {
        return (0.0, l, 0.0);
    }
    let range = maxc - minc;
    let s = if l <= 0.5 { range / (maxc + minc) } else { range / (2.0 - maxc - minc) };
    let (rc, gc, bc) = ((maxc - r) / range, (maxc - g) / range, (maxc - b) / range);
    let h = if r == maxc {
        bc - gc
    } else if g == maxc {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    ((h / 6.0).rem_euclid(1.0), l, s)
}

/// HLS → RGB (0..1), igual que `colorsys` de Python.
pub fn hls_to_rgb(h: f64, l: f64, s: f64) -> [f64; 3] {
    if s == 0.0 {
        return [l, l, l];
    }
    let m2 = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let m1 = 2.0 * l - m2;
    let v = |hue: f64| {
        let hue = hue.rem_euclid(1.0);
        if hue < 1.0 / 6.0 {
            m1 + (m2 - m1) * hue * 6.0
        } else if hue < 0.5 {
            m2
        } else if hue < 2.0 / 3.0 {
            m1 + (m2 - m1) * (2.0 / 3.0 - hue) * 6.0
        } else {
            m1
        }
    };
    [v(h + 1.0 / 3.0), v(h), v(h - 1.0 / 3.0)]
}

/// `color` con al menos `target` de contraste contra `bg`, conservando su
/// tono: lo aclara (fondo oscuro) u oscurece (fondo claro) de 1 en 1 % de
/// luminosidad. Si ya se lee, regresa igual; `margin` se suma a la meta
/// cuando hay que moverlo (por si después se redondea).
pub fn lift(color: Rgb, bg: Rgb, target: f64, margin: f64) -> Rgb {
    let (c, bg) = (to01(color), to01(bg));
    if ratio01(c, bg) >= target {
        return color;
    }
    let target = target + margin;
    let (h, mut l, s) = rgb_to_hls(c);
    let step = if lum01(bg) < 0.5 { 0.01 } else { -0.01 };
    while (0.0..=1.0).contains(&l) {
        l += step;
        let c = hls_to_rgb(h, l.clamp(0.0, 1.0), s);
        if ratio01(c, bg) >= target {
            return c.map(|v| v.clamp(0.0, 1.0) * 255.0);
        }
    }
    if step > 0.0 { [255.0; 3] } else { [0.0; 3] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_ida_y_vuelta() {
        assert_eq!(hex(parse("#1A2b3C").unwrap()), "#1a2b3c");
        assert_eq!(norm(" #ABCDEF "), Some("abcdef".into()));
        assert_eq!(parse("#12345"), None);
        assert_eq!(parse("background"), None);
    }

    #[test]
    fn contraste() {
        let (w, b) = ([255.0; 3], [0.0; 3]);
        assert!((contrast(w, b) - 21.0).abs() < 1e-9);
        assert_eq!(on(b, w, b), w);
    }

    #[test]
    fn redondeo_al_par() {
        assert_eq!(hex([0.5, 1.5, 2.5]), "#000202");
    }

    #[test]
    fn hls_como_python() {
        // colorsys.rgb_to_hls(0.2, 0.4, 0.6) = (0.5833…, 0.4, 0.5)
        let (h, l, s) = rgb_to_hls([0.2, 0.4, 0.6]);
        assert!((h - 0.5833333333333334).abs() < 1e-12 && (l - 0.4).abs() < 1e-12 && (s - 0.5).abs() < 1e-12);
        let c = hls_to_rgb(h, l, s);
        assert!((c[0] - 0.2).abs() < 1e-12 && (c[2] - 0.6).abs() < 1e-12);
    }

    #[test]
    fn lift_sube_hasta_leerse() {
        let bg = parse("#101010").unwrap();
        let c = lift(parse("#202020").unwrap(), bg, 4.5, 0.25);
        assert!(contrast(c, bg) >= 4.5);
        let ya = parse("#ffffff").unwrap();
        assert_eq!(lift(ya, bg, 4.5, 0.25), ya);
    }
}
