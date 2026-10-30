//! De celdas a pixeles, como lo hace kitty: cada letra en su celda con la
//! fuente de kitty, y las líneas de los recuadros (─│╭╮…) y los bloques de las
//! barras (▁…█) dibujados a mano para que se unan sin huecos entre celdas.

use crate::icon::Icon;
use crate::look::{Look, kitty};
use cosmic_text::{Attrs, Buffer as TextBuffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent, Weight};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect as SkRect, Stroke, Transform};

type Rgba = (u8, u8, u8, f32);

pub struct Painter {
    fonts: FontSystem,
    cache: SwashCache,
    family: String,
    weight: u16,
    /// el peso de la negrita: la cara más gruesa que sigue y que la familia
    /// sí tenga (sin ella, cosmic-text se va a otra fuente)
    bold: u16,
    /// tamaño de la letra y de la celda, en pixeles lógicos
    pub px: f32,
    pub cw: f32,
    pub ch: f32,
}

/// "GeistMono Nerd Font Mono Bold" → ("GeistMono Nerd Font Mono", 700): kitty
/// junta familia y estilo en un solo nombre.
fn split_family(fonts: &FontSystem, name: &str) -> (String, u16) {
    let words: Vec<&str> = name.split_whitespace().collect();
    let has = |fam: &str| fonts.db().faces().any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(fam)));
    for cut in (1..=words.len()).rev() {
        let fam = words[..cut].join(" ");
        if has(&fam) {
            let weight = match words[cut..].join(" ").to_lowercase().replace(' ', "").as_str() {
                "thin" => 100,
                "extralight" | "ultralight" => 200,
                "light" => 300,
                "medium" => 500,
                "semibold" | "demibold" => 600,
                "bold" => 700,
                "extrabold" | "ultrabold" => 800,
                "black" | "heavy" => 900,
                _ => 400,
            };
            return (fam, weight);
        }
    }
    ("monospace".into(), 400)
}

/// La cara que sigue de `weight` en `family` (Regular → Bold); si ya es la más
/// gruesa, la misma, como kitty con `bold_font auto`.
fn bold_weight(fonts: &FontSystem, family: &str, weight: u16) -> u16 {
    fonts
        .db()
        .faces()
        .filter(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(family)))
        .map(|f| f.weight.0)
        .filter(|&w| w > weight)
        .min()
        .unwrap_or(weight)
}

fn rgba(c: Color, alpha: f32) -> Option<Rgba> {
    match c {
        Color::Rgb(r, g, b) => Some((r, g, b, alpha)),
        Color::Reset => None,
        Color::Black => Some((0, 0, 0, alpha)),
        Color::White => Some((255, 255, 255, alpha)),
        _ => Some((128, 128, 128, alpha)),
    }
}

fn paint(c: Rgba) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.0, c.1, c.2, (c.3 * 255.0).round() as u8);
    p.anti_alias = true;
    p
}

/// Mezcla un pixel (pixmap con alfa premultiplicado).
fn blend(pix: &mut Pixmap, x: i32, y: i32, c: Rgba, cov: f32) {
    if x < 0 || y < 0 || x >= pix.width() as i32 || y >= pix.height() as i32 {
        return;
    }
    let a = (c.3 * cov).clamp(0.0, 1.0);
    if a <= 0.0 {
        return;
    }
    let i = (y as usize * pix.width() as usize + x as usize) * 4;
    let d = &mut pix.data_mut()[i..i + 4];
    let mix = |src: u8, dst: u8| (src as f32 * a + dst as f32 * (1.0 - a)).round() as u8;
    d[0] = mix(c.0, d[0]);
    d[1] = mix(c.1, d[1]);
    d[2] = mix(c.2, d[2]);
    d[3] = ((a + d[3] as f32 / 255.0 * (1.0 - a)) * 255.0).round() as u8;
}

/// Brazos de una línea de recuadro: arriba, derecha, abajo, izquierda
/// (0 nada, 1 delgada, 2 gruesa, 3 doble).
fn arms(c: char) -> Option<[u8; 4]> {
    Some(match c {
        '─' => [0, 1, 0, 1],
        '│' => [1, 0, 1, 0],
        '┌' => [0, 1, 1, 0],
        '┐' => [0, 0, 1, 1],
        '└' => [1, 1, 0, 0],
        '┘' => [1, 0, 0, 1],
        '├' => [1, 1, 1, 0],
        '┤' => [1, 0, 1, 1],
        '┬' => [0, 1, 1, 1],
        '┴' => [1, 1, 0, 1],
        '┼' => [1, 1, 1, 1],
        '━' => [0, 2, 0, 2],
        '┃' => [2, 0, 2, 0],
        '┏' => [0, 2, 2, 0],
        '┓' => [0, 0, 2, 2],
        '┗' => [2, 2, 0, 0],
        '┛' => [2, 0, 0, 2],
        '═' => [0, 3, 0, 3],
        '║' => [3, 0, 3, 0],
        '╔' => [0, 3, 3, 0],
        '╗' => [0, 0, 3, 3],
        '╚' => [3, 3, 0, 0],
        '╝' => [3, 0, 0, 3],
        _ => return None,
    })
}

impl Painter {
    pub fn new() -> Painter {
        Painter::sized(None)
    }

    /// Con la fuente de kitty pero a `px` pixeles (la barra de empalme va más
    /// chica que la terminal); None: el tamaño de kitty.
    pub fn sized(px: Option<f32>) -> Painter {
        let mut fonts = FontSystem::new();
        let name = kitty("font_family").unwrap_or_else(|| "monospace".into());
        let (family, weight) = split_family(&fonts, &name);
        let pt: f32 = kitty("font_size").and_then(|v| v.parse().ok()).unwrap_or(11.0);
        let px = px.unwrap_or(pt * 96.0 / 72.0).round();
        // el ancho de la celda: lo que avanza una letra de la fuente
        let mut b = TextBuffer::new(&mut fonts, Metrics::new(px, px));
        let attrs = Attrs::new().family(Family::Name(&family)).weight(Weight(weight));
        let mut bb = b.borrow_with(&mut fonts);
        bb.set_text("MMMMMMMMMM", &attrs, Shaping::Advanced, None);
        let cw = bb.layout_runs().next().map_or(px * 0.6, |r| r.line_w / 10.0);
        let bold = bold_weight(&fonts, &family, weight);
        Painter { fonts, cache: SwashCache::new(), family, weight, bold, px, cw, ch: (px * 1.3).round() }
    }

    /// Tamaño en pixeles (lógicos) de `cols`×`rows` celdas.
    pub fn size(&self, cols: u16, rows: u16) -> (u32, u32) {
        ((cols as f32 * self.cw).ceil() as u32, (rows as f32 * self.ch).ceil() as u32)
    }

    /// Dibuja un aviso con su esquina en (ox, oy), a escala `s`.
    pub fn card(&mut self, pix: &mut Pixmap, buf: &Buffer, look: &Look, ox: f32, oy: f32, s: f32) {
        let (cw, ch) = (self.cw * s, self.ch * s);
        let area = buf.area;
        let alpha = look.alpha();
        let base = rgba(look.theme.base, alpha).unwrap_or((0, 0, 0, alpha));
        let lt = (cw / 9.0).round().max(1.0);

        // el fondo: hasta la mitad de las celdas del borde (donde va la línea),
        // redondeado si el borde lo es; por fuera queda transparente
        let corner = buf[(area.x, area.y)].symbol();
        let (fx, fy, fw, fh) = if corner == " " {
            (ox, oy, area.width as f32 * cw, area.height as f32 * ch)
        } else {
            let (cx, cy) = (((cw - lt) / 2.0).floor() + lt / 2.0, ((ch - lt) / 2.0).floor() + lt / 2.0);
            (ox + cx, oy + cy, (area.width as f32 - 1.0) * cw, (area.height as f32 - 1.0) * ch)
        };
        let r = if corner == "╭" { cw / 2.0 } else { 0.0 };
        if let Some(path) = rounded_rect(fx, fy, fw, fh, r) {
            pix.fill_path(&path, &paint(base), FillRule::Winding, Transform::identity(), None);
        }
        self.cells(pix, buf, look, ox, oy, s);
    }

    /// Las celdas de `buf` (texto, fondos propios de cada celda y lo que se
    /// dibuja a mano) con su esquina en (ox, oy), sin pintar un fondo general:
    /// el de la barra lo pone quien llama (islas, barra sólida…).
    pub fn cells(&mut self, pix: &mut Pixmap, buf: &Buffer, look: &Look, ox: f32, oy: f32, s: f32) {
        let (cw, ch) = (self.cw * s, self.ch * s);
        let area = buf.area;
        let lt = (cw / 9.0).round().max(1.0);
        for row in 0..area.height {
            let y = oy + row as f32 * ch;
            // primero fondos distintos al del aviso y lo que se dibuja a mano
            let mut text: Vec<(usize, String, Rgba, bool)> = Vec::new();
            for col in 0..area.width {
                let cell = &buf[(area.x + col, area.y + row)];
                let x = ox + col as f32 * cw;
                if cell.bg != look.theme.base {
                    if let Some(bg) = rgba(cell.bg, 1.0) {
                        pix.fill_rect(SkRect::from_xywh(x, y, cw, ch).unwrap(), &paint(bg), Transform::identity(), None);
                    }
                }
                let sym = cell.symbol();
                let fg = rgba(cell.fg, 1.0).unwrap_or_else(|| rgba(look.theme.fg, 1.0).unwrap_or((255, 255, 255, 1.0)));
                let mut chars = sym.chars();
                if let (Some(c), None) = (chars.next(), chars.next()) {
                    if self.by_hand(pix, c, x, y, cw, ch, lt, fg) {
                        continue;
                    }
                }
                if !sym.trim().is_empty() {
                    text.push((col as usize, sym.to_string(), fg, cell.modifier.contains(Modifier::BOLD)));
                }
                // subrayado: una línea abajo de la celda, del grueso de los recuadros
                if cell.modifier.contains(Modifier::UNDERLINED)
                    && let Some(r) = SkRect::from_xywh(x, (y + ch - 2.0 * lt).round(), cw, lt)
                {
                    pix.fill_rect(r, &paint(fg), Transform::identity(), None);
                }
            }
            self.text(pix, &text, ox, y, cw, ch, s);
        }
    }

    /// El ícono de un aviso, siempre en el mismo cuadro: un cuadrado del alto
    /// de su caja, redondeado si el borde del look lo es. Una foto lo llena
    /// recortada (sin deformar) con un filo del color del borde; un ícono del
    /// tema va entero al centro sobre un fondito del color del aviso, y los de
    /// una sola tinta se pintan de ese color (el del look, o rojo si es urgente).
    #[allow(clippy::too_many_arguments)]
    pub fn icon(&self, pix: &mut Pixmap, icon: &Icon, cell: ratatui::layout::Rect, look: &Look, tint: Color, rounded: bool, ox: f32, oy: f32, s: f32) {
        let (cw, ch) = (self.cw * s, self.ch * s);
        let (bw, bh) = (cell.width as f32 * cw, cell.height as f32 * ch);
        let q = bw.min(bh).floor();
        let x = (ox + cell.x as f32 * cw).round();
        let y = (oy + cell.y as f32 * ch + (bh - q) / 2.0).round();
        let r = if rounded { (q * 0.22).round() } else { 0.0 };
        let lt = (cw / 9.0).round().max(1.0);
        let Some(tile) = rounded_rect(x, y, q, q, r) else { return };
        let src = &icon.pix;
        let quality = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
        if icon.photo {
            let k = (q / src.width() as f32).max(q / src.height() as f32);
            let dx = x + (q - src.width() as f32 * k) / 2.0;
            let dy = y + (q - src.height() as f32 * k) / 2.0;
            let Some(mut mask) = tiny_skia::Mask::new(pix.width(), pix.height()) else { return };
            mask.fill_path(&tile, FillRule::Winding, true, Transform::identity());
            pix.draw_pixmap(0, 0, (**src).as_ref(), &quality, Transform::from_row(k, 0.0, 0.0, k, dx, dy), Some(&mask));
            if let (Some(edge), Some(c)) = (rounded_rect(x + lt / 2.0, y + lt / 2.0, q - lt, q - lt, (r - lt / 2.0).max(0.0)), rgba(look.theme.border, 1.0)) {
                let stroke = Stroke { width: lt, ..Default::default() };
                pix.stroke_path(&edge, &paint(c), &stroke, Transform::identity(), None);
            }
            return;
        }
        if let Some(bg) = rgba(tint, 0.16) {
            pix.fill_path(&tile, &paint(bg), FillRule::Winding, Transform::identity(), None);
        }
        let inner = q * if icon.symbolic { 0.56 } else { 0.72 };
        let k = (inner / src.width() as f32).min(inner / src.height() as f32);
        let dx = x + (q - src.width() as f32 * k) / 2.0;
        let dy = y + (q - src.height() as f32 * k) / 2.0;
        let tinted;
        let src = match (icon.symbolic, rgba(tint, 1.0)) {
            (true, Some((r, g, b, _))) => {
                let mut p = (**src).clone();
                for px in p.data_mut().chunks_exact_mut(4) {
                    let a = px[3] as u16;
                    px[0] = (r as u16 * a / 255) as u8;
                    px[1] = (g as u16 * a / 255) as u8;
                    px[2] = (b as u16 * a / 255) as u8;
                }
                tinted = p;
                &tinted
            }
            _ => &**src,
        };
        pix.draw_pixmap(0, 0, src.as_ref(), &quality, Transform::from_row(k, 0.0, 0.0, k, dx, dy), None);
    }

    /// Las letras de un renglón, cada una en su celda.
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, pix: &mut Pixmap, cells: &[(usize, String, Rgba, bool)], ox: f32, y: f32, cw: f32, ch: f32, s: f32) {
        if cells.is_empty() {
            return;
        }
        let (weight, bold) = (self.weight, self.bold);
        let mut glyphs = Vec::new();
        {
            let family = self.family.clone();
            let mut b = TextBuffer::new(&mut self.fonts, Metrics::new(self.px * s, ch));
            let mut bb = b.borrow_with(&mut self.fonts);
            let spans: Vec<(&str, Attrs)> = cells
                .iter()
                .map(|(_, sym, _, is_bold)| {
                    (sym.as_str(), Attrs::new().family(Family::Name(&family)).weight(Weight(if *is_bold { bold } else { weight })))
                })
                .collect();
            let default = Attrs::new().family(Family::Name(&family));
            bb.set_rich_text(spans, &default, Shaping::Advanced, None);
            // de byte a celda: cada pedazo empieza donde terminó el anterior
            let mut starts = Vec::with_capacity(cells.len());
            let mut at = 0;
            for (col, sym, fg, _) in cells {
                starts.push((at, *col, *fg));
                at += sym.len();
            }
            for run in bb.layout_runs() {
                for g in run.glyphs {
                    let Some(&(_, col, fg)) = starts.iter().rev().find(|(b, ..)| *b <= g.start) else { continue };
                    let p = g.physical((ox + col as f32 * cw - g.x, y + run.line_y), 1.0);
                    glyphs.push((p, fg));
                }
            }
        }
        for (p, fg) in glyphs {
            let Some(img) = self.cache.get_image(&mut self.fonts, p.cache_key).as_ref() else { continue };
            let (left, top) = (img.placement.left, img.placement.top);
            let (w, h) = (img.placement.width as i32, img.placement.height as i32);
            for iy in 0..h {
                for ix in 0..w {
                    let (px, py) = (p.x + left + ix, p.y - top + iy);
                    let i = (iy * w + ix) as usize;
                    match img.content {
                        SwashContent::Mask => blend(pix, px, py, fg, img.data[i] as f32 / 255.0),
                        SwashContent::SubpixelMask => {
                            let d = &img.data[i * 4..i * 4 + 3];
                            blend(pix, px, py, fg, (d[0] as f32 + d[1] as f32 + d[2] as f32) / 765.0)
                        }
                        SwashContent::Color => {
                            let d = &img.data[i * 4..i * 4 + 4];
                            blend(pix, px, py, (d[0], d[1], d[2], 1.0), d[3] as f32 / 255.0)
                        }
                    }
                }
            }
        }
    }

    /// Líneas de recuadro, bloques y sombras: a mano, a la medida de la celda.
    #[allow(clippy::too_many_arguments)]
    fn by_hand(&self, pix: &mut Pixmap, c: char, x: f32, y: f32, w: f32, h: f32, lt: f32, fg: Rgba) -> bool {
        let p = paint(fg);
        let mut rect = |x: f32, y: f32, w: f32, h: f32| {
            if let Some(r) = SkRect::from_xywh(x, y, w.max(0.0), h.max(0.0)) {
                pix.fill_rect(r, &p, Transform::identity(), None);
            }
        };
        let code = c as u32;
        // ▁▂▃▄▅▆▇█: de abajo hacia arriba en octavos
        if (0x2581..=0x2588).contains(&code) {
            let k = (code - 0x2580) as f32 / 8.0;
            rect(x, y + h * (1.0 - k), w, h * k);
            return true;
        }
        match c {
            '▀' => rect(x, y, w, h / 2.0),
            '▌' => rect(x, y, (w / 2.0).round(), h),
            '▐' => rect(x + (w / 2.0).round(), y, w - (w / 2.0).round(), h),
            '░' | '▒' | '▓' => {
                let a = match c {
                    '░' => 0.25,
                    '▒' => 0.5,
                    _ => 0.75,
                };
                let p = paint((fg.0, fg.1, fg.2, fg.3 * a));
                pix.fill_rect(SkRect::from_xywh(x, y, w, h).unwrap(), &p, Transform::identity(), None);
            }
            '┄' | '┈' | '┆' | '┊' => {
                let n = if matches!(c, '┄' | '┆') { 3.0 } else { 4.0 };
                let horizontal = matches!(c, '┄' | '┈');
                let len = if horizontal { w } else { h };
                let seg = len / n;
                for i in 0..n as usize {
                    let start = i as f32 * seg + seg * 0.2;
                    if horizontal {
                        rect(x + start, y + ((h - lt) / 2.0).floor(), seg * 0.6, lt);
                    } else {
                        rect(x + ((w - lt) / 2.0).floor(), y + start, lt, seg * 0.6);
                    }
                }
            }
            '╭' | '╮' | '╰' | '╯' => {
                let (hx, vy) = match c {
                    '╭' => (1.0, 1.0),
                    '╮' => (-1.0, 1.0),
                    '╰' => (1.0, -1.0),
                    _ => (-1.0, -1.0),
                };
                let cx = x + ((w - lt) / 2.0).floor() + lt / 2.0;
                let cy = y + ((h - lt) / 2.0).floor() + lt / 2.0;
                let r = w / 2.0;
                let mut pb = PathBuilder::new();
                pb.move_to(if hx > 0.0 { x + w } else { x }, cy);
                pb.line_to(cx + hx * r, cy);
                pb.quad_to(cx, cy, cx, cy + vy * r);
                pb.line_to(cx, if vy > 0.0 { y + h } else { y });
                if let Some(path) = pb.finish() {
                    let stroke = Stroke { width: lt, ..Stroke::default() };
                    pix.stroke_path(&path, &p, &stroke, Transform::identity(), None);
                }
            }
            _ => {
                let Some([up, right, down, left]) = arms(c) else { return false };
                let ht = (lt * 2.0).max(2.0);
                let gap = lt + 1.0;
                let thick = |k: u8| if k == 2 { ht } else { lt };
                let mid = |len: f32, t: f32| ((len - t) / 2.0).floor();
                if [up, right, down, left].contains(&3) {
                    let (cx, cy) = (x + mid(w, lt), y + mid(h, lt));
                    let (horiz, vert) = (left == 3 || right == 3, up == 3 || down == 3);
                    if horiz && !vert {
                        rect(x, cy - gap, w, lt);
                        rect(x, cy + gap, w, lt);
                    } else if vert && !horiz {
                        rect(cx - gap, y, lt, h);
                        rect(cx + gap, y, lt, h);
                    } else {
                        // esquina doble: una línea por fuera (k = -1) y otra por dentro
                        let hx = if right == 3 { 1.0 } else { -1.0 };
                        let vy = if down == 3 { 1.0 } else { -1.0 };
                        for k in [-1.0f32, 1.0] {
                            let (mx, my) = (cx + k * hx * gap, cy + k * vy * gap);
                            if hx > 0.0 {
                                rect(mx, my, x + w - mx, lt);
                            } else {
                                rect(x, my, mx + lt - x, lt);
                            }
                            if vy > 0.0 {
                                rect(mx, my, lt, y + h - my);
                            } else {
                                rect(mx, y, lt, my + lt - y);
                            }
                        }
                    }
                } else {
                    // cada brazo, del centro a la orilla
                    let t_h = thick(left.max(right));
                    let t_v = thick(up.max(down));
                    let (vx, hy) = (x + mid(w, t_v), y + mid(h, t_h));
                    if up > 0 {
                        rect(vx, y, t_v, hy - y + t_h);
                    }
                    if down > 0 {
                        rect(vx, hy, t_v, y + h - hy);
                    }
                    if left > 0 {
                        rect(x, hy, vx - x + t_v, t_h);
                    }
                    if right > 0 {
                        rect(vx, hy, x + w - vx, t_h);
                    }
                }
            }
        }
        true
    }
}

/// Un rectángulo redondeado relleno del color `c` con opacidad `alpha` (las
/// islas de empalme, el fondo de su barra).
#[allow(clippy::too_many_arguments)]
pub fn rounded(pix: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color, alpha: f32) {
    if let (Some(path), Some(col)) = (rounded_rect(x, y, w, h, r.min(w / 2.0).min(h / 2.0)), rgba(c, alpha)) {
        pix.fill_path(&path, &paint(col), FillRule::Winding, Transform::identity(), None);
    }
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    if r <= 0.0 {
        return SkRect::from_xywh(x, y, w, h).map(PathBuilder::from_rect);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}
