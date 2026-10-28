//! Los íconos de los avisos: una ruta (png o svg), un nombre del tema de
//! íconos ("media-record", del tema de GTK, Adwaita o hicolor) o la imagen que
//! manda la app por D-Bus (image-data, que bus.rs guarda como png). PNG, JPEG
//! (aunque se llame .png: se ve por dentro) y SVG. Se cargan
//! una vez; los "symbolic" (de una sola tinta) se pintan con el color del look.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use tiny_skia::Pixmap;

/// Lo grande que se rasteriza un svg (luego se escala al cuadro del aviso).
const SVG_PX: f32 = 128.0;

#[derive(Clone)]
pub struct Icon {
    pub pix: Rc<Pixmap>,
    /// de una sola tinta: pintarlo del color del look
    pub symbolic: bool,
    /// una foto (la imagen de la app, una captura, una portada): llena su
    /// cuadro recortada; los íconos del tema caben enteros dentro de él
    pub photo: bool,
}

pub struct Icons {
    cache: HashMap<String, Option<Icon>>,
    themes: Vec<PathBuf>,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// Las carpetas de temas a buscar, en orden: el de GTK (y los que hereda),
/// luego Adwaita y hicolor, en las de tu usuario y en las del sistema.
fn themes() -> Vec<PathBuf> {
    let gtk = ["gtk-3.0/settings.ini", "gtk-4.0/settings.ini"]
        .iter()
        .filter_map(|f| std::fs::read_to_string(home().join(".config").join(f)).ok())
        .find_map(|t| t.lines().find_map(|l| l.trim().strip_prefix("gtk-icon-theme-name=").map(|v| v.trim().to_string())));
    let bases = [home().join(".local/share/icons"), home().join(".icons"), PathBuf::from("/usr/share/icons")];
    // el de GTK y Adwaita, con lo que heredan (Adwaita → AdwaitaLegacy, hicolor)
    let mut names: Vec<String> = gtk.into_iter().chain(["Adwaita".to_string()]).collect();
    let mut i = 0;
    while i < names.len() {
        // lo que hereda el tema (index.theme → Inherits=)
        let inherits = bases
            .iter()
            .filter_map(|b| std::fs::read_to_string(b.join(&names[i]).join("index.theme")).ok())
            .find_map(|t| t.lines().find_map(|l| l.strip_prefix("Inherits=").map(String::from)));
        for n in inherits.iter().flat_map(|v| v.split(',')).map(str::trim) {
            if !n.is_empty() && !names.iter().any(|x| x == n) {
                names.push(n.to_string());
            }
        }
        i += 1;
    }
    if !names.iter().any(|x| x == "hicolor") {
        names.push("hicolor".into());
    }
    names.iter().flat_map(|n| bases.iter().map(move |b| b.join(n))).filter(|p| p.is_dir()).collect()
}

/// De nombre a archivo: ruta directa, o búsqueda en los temas.
fn resolve(name: &str, themes: &[PathBuf]) -> Option<PathBuf> {
    let name = name.strip_prefix("file://").unwrap_or(name);
    if let Some(rest) = name.strip_prefix("~/") {
        return Some(home().join(rest)).filter(|p| p.is_file());
    }
    if name.starts_with('/') {
        return Some(PathBuf::from(name)).filter(|p| p.is_file());
    }
    const SIZES: [&str; 9] = ["scalable", "symbolic", "48x48", "64x64", "128x128", "256x256", "32x32", "512x512", "24x24"];
    const KINDS: [&str; 10] = ["apps", "status", "devices", "actions", "places", "mimetypes", "categories", "emblems", "panel", "legacy"];
    for theme in themes {
        for size in SIZES {
            for kind in KINDS {
                // dos acomodos: tamaño/tipo (lo normal) y tipo/tamaño
                for dir in [theme.join(size).join(kind), theme.join(kind).join(size)] {
                    for ext in ["svg", "png"] {
                        let p = dir.join(format!("{name}.{ext}"));
                        if p.is_file() {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }
    ["svg", "png"].iter().map(|e| PathBuf::from(format!("/usr/share/pixmaps/{name}.{e}"))).find(|p| p.is_file())
}

fn load(path: &PathBuf) -> Option<Pixmap> {
    let data = std::fs::read(path).ok()?;
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) || data.starts_with(b"<") {
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let size = tree.size();
        let k = SVG_PX / size.width().max(size.height());
        let mut pix = Pixmap::new((size.width() * k).ceil() as u32, (size.height() * k).ceil() as u32)?;
        resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pix.as_mut());
        return Some(pix);
    }
    let pix = if data.starts_with(&[0xFF, 0xD8]) { jpeg(&data)? } else { Pixmap::decode_png(&data).ok()? };
    Some(shrink(pix))
}

fn jpeg(data: &[u8]) -> Option<Pixmap> {
    use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut d = zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(data), opts);
    let rgba = d.decode().ok()?; // sin transparencia: ya está "premultiplicado"
    let info = d.info()?;
    Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(info.width as u32, info.height as u32)?)
}

/// Las fotos grandes, a mitades hasta que queden cerca del tamaño de un svg:
/// se ven mejor que de un solo salto y no ocupan memoria de más.
fn shrink(mut pix: Pixmap) -> Pixmap {
    while pix.width().max(pix.height()) as f32 > SVG_PX * 2.0 {
        let Some(mut half) = Pixmap::new(pix.width().div_ceil(2), pix.height().div_ceil(2)) else { break };
        let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
        half.draw_pixmap(0, 0, pix.as_ref(), &paint, tiny_skia::Transform::from_scale(0.5, 0.5), None);
        pix = half;
    }
    pix
}

impl Icons {
    pub fn new() -> Icons {
        Icons { cache: HashMap::new(), themes: themes() }
    }

    pub fn get(&mut self, name: &str) -> Option<Icon> {
        if name.trim().is_empty() {
            return None;
        }
        if let Some(hit) = self.cache.get(name) {
            return hit.clone();
        }
        // como GTK: si no está el de color, el de una sola tinta
        let found = resolve(name, &self.themes).or_else(|| (!name.contains('/')).then(|| resolve(&format!("{name}-symbolic"), &self.themes)).flatten());
        let icon = found.and_then(|p| {
            let symbolic = name.ends_with("-symbolic") || p.components().any(|c| c.as_os_str() == "symbolic");
            let svg = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
            let photo = !symbolic && !svg && name.contains('/');
            load(&p).map(|pix| Icon { pix: Rc::new(pix), symbolic, photo })
        });
        // una ruta que todavía no existe (una miniatura a medio escribir) se
        // vuelve a buscar; cada imagen de app es otra: que no crezca sin fin
        if icon.is_some() || !name.contains('/') {
            if self.cache.len() > 300 {
                self.cache.clear();
            }
            self.cache.insert(name.to_string(), icon.clone());
        }
        icon
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Con los temas de este sistema (Adwaita y su AdwaitaLegacy): por nombre,
    /// el de una sola tinta y una ruta que no existe.
    #[test]
    #[ignore = "depende de los íconos instalados"]
    fn finds_theme_icons() {
        let mut icons = Icons::new();
        assert!(icons.get("media-record").is_some());
        assert!(icons.get("audio-volume-high-symbolic").is_some_and(|i| i.symbolic));
        assert!(icons.get("/no/existe.png").is_none());
        // un jpeg con nombre de png, y grande: se lee y se achica
        let err = home().join(".config/swaync/images/error.png");
        if err.exists() {
            let i = icons.get(err.to_str().unwrap()).expect("jpeg");
            assert!(i.pix.width() <= 256);
        }
        assert!(icons.get("").is_none());
    }
}
