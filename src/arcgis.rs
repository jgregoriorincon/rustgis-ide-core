//! The ArcGIS addresses a plugin hands RustGIS: a layer's features in the map view, and a map
//! image as a tile template.
//!
//! RustGIS's `service.add` downloads an ArcGIS layer whole and can't apply a catalogue's `where`
//! (and the web app refuses it from a command), so the plugin asks the server itself:
//! `…/query` with the `where` and the view's box, read by `layer.addUrl`, or `…/export` as a
//! `map.addTiles` template ({bbox-epsg-3857}, {width} and {height} are RustGIS's placeholders).

use crate::entry::Entry;
use crate::kind::Kind;
use serde_json::Value;

/// Features a server gives at once when its layer doesn't say (ArcGIS's own default).
pub const DEFAULT_MAX_RECORDS: u32 = 1_000;
/// The most asked for at once, whatever a layer says: more makes answers too big to add.
pub const MAX_RECORDS: u32 = 10_000;

/// The map view as a Web Mercator box `[west, south, east, north]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub bbox: [f64; 4],
}

/// The view from `host.view` (`{"center": [lon, lat], "scale"}`): the box a screen of about
/// 1600 × 1000 pixels at 96 dpi shows at that scale, which covers the view with a margin.
pub fn view_of(view: &Value) -> Option<View> {
    let c = view.get("center")?.as_array()?;
    let (lon, lat) = (c.first()?.as_f64()?, c.get(1)?.as_f64()?);
    let scale = view.get("scale")?.as_f64()?;
    if !(lon.is_finite() && lat.is_finite() && scale.is_finite()) || scale <= 0.0 {
        return None;
    }
    const R: f64 = 6_378_137.0;
    let x = lon.clamp(-180.0, 180.0).to_radians() * R;
    let y = (std::f64::consts::FRAC_PI_4 + lat.clamp(-85.05, 85.05).to_radians() / 2.0).tan().ln() * R;
    let m_per_px = scale * 0.0254 / 96.0;
    let (half_w, half_h) = (800.0 * m_per_px, 500.0 * m_per_px);
    Some(View { bbox: [x - half_w, y - half_h, x + half_w, y + half_h] })
}

/// The features a layer's description (`…/MapServer/3?f=json`) says it gives at once.
pub fn max_records(layer: &Value) -> u32 {
    layer
        .get("maxRecordCount")
        .and_then(Value::as_u64)
        .filter(|n| *n > 0)
        .map_or(DEFAULT_MAX_RECORDS, |n| u32::try_from(n).unwrap_or(MAX_RECORDS).min(MAX_RECORDS))
}

/// The address of an ArcGIS layer's features as GeoJSON in longitude/latitude: those that pass
/// its filter, in `view` when given, at most `limit`. `None` for anything but an ArcGIS layer.
pub fn features_url(e: &Entry, view: Option<View>, limit: u32) -> Option<String> {
    if e.kind != Kind::ArcGisLayer {
        return None;
    }
    let filter = e.filter.as_deref().unwrap_or("1=1");
    let mut u =
        format!("{}/query?where={}&outFields=*&returnGeometry=true&outSR=4326&f=geojson&resultRecordCount={limit}", e.url, encode(filter));
    if let Some(View { bbox: [w, s, east, n] }) = view {
        u.push_str(&format!(
            "&geometry={w:.0},{s:.0},{east:.0},{n:.0}&geometryType=esriGeometryEnvelope&inSR=3857&spatialRel=esriSpatialRelIntersects"
        ));
    }
    Some(u)
}

/// A `map.addTiles` template of the service's map image: a MapServer (or one of its layers,
/// with its filter) or an ImageServer. `None` for a FeatureServer, which draws no images.
pub fn image_template(e: &Entry) -> Option<String> {
    const BOX: &str = "bbox={bbox-epsg-3857}&bboxSR=3857&imageSR=3857&size={width},{height}&transparent=true&f=image";
    match e.kind {
        Kind::ArcGisImage => Some(format!("{}/exportImage?{BOX}&format=png", e.url)),
        Kind::ArcGisService if is_map_server(&e.url) => Some(format!("{}/export?{BOX}&format=png32", e.url)),
        Kind::ArcGisLayer => {
            let (service, layer) = e.url.rsplit_once('/')?;
            if !is_map_server(service) {
                return None;
            }
            let mut u = format!("{service}/export?{BOX}&format=png32&layers=show:{layer}");
            if let Some(f) = &e.filter {
                let defs = serde_json::json!({ layer: f }).to_string();
                u.push_str(&format!("&layerDefs={}", encode(&defs)));
            }
            Some(u)
        }
        _ => None,
    }
}

fn is_map_server(url: &str) -> bool {
    url.to_ascii_lowercase().ends_with("/mapserver")
}

/// Percent-encodes everything but the unreserved characters, so a `where` or a JSON value
/// travels as one query value (and its braces aren't taken for template placeholders).
pub fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Raw;
    use serde_json::json;

    fn entry(url: &str, label: Option<&str>) -> Entry {
        Entry::new(&Raw { id: Some("1"), name: Some("Capa"), url: Some(url), label, ..Raw::default() }).expect("a valid entry")
    }

    const LOTE: &str = "https://serviciosgis.catastrobogota.gov.co/arcgis/rest/services/catastro/lote/MapServer/0";

    #[test]
    fn features_in_the_view_with_the_filter() {
        let peajes =
            entry("https://hermes2.invias.gov.co/server/rest/services/MapaCarreteras/Peaje/FeatureServer/0/query?where=a%253D'B'", None);
        let view = View { bbox: [-8_250_000.4, 520_000.0, -8_230_000.0, 530_000.6] };
        assert_eq!(
            features_url(&peajes, Some(view), 2000).as_deref(),
            Some(
                "https://hermes2.invias.gov.co/server/rest/services/MapaCarreteras/Peaje/FeatureServer/0/query?where=a%3D%27B%27&outFields=*&returnGeometry=true&outSR=4326&f=geojson&resultRecordCount=2000&geometry=-8250000,520000,-8230000,530001&geometryType=esriGeometryEnvelope&inSR=3857&spatialRel=esriSpatialRelIntersects"
            )
        );
        let lote = features_url(&entry(LOTE, None), None, 1000).expect("a layer");
        assert!(lote.starts_with(&format!("{LOTE}/query?where=1%3D1&")), "{lote}");
        assert_eq!(features_url(&entry("https://x/rest/services/a/MapServer", None), None, 10), None);
    }

    #[test]
    fn map_images() {
        let lote = image_template(&entry(LOTE, None)).expect("a MapServer layer");
        assert_eq!(
            lote,
            "https://serviciosgis.catastrobogota.gov.co/arcgis/rest/services/catastro/lote/MapServer/export?bbox={bbox-epsg-3857}&bboxSR=3857&imageSR=3857&size={width},{height}&transparent=true&f=image&format=png32&layers=show:0"
        );
        let filtered =
            entry("https://geo.anm.gov.co/webgis/rest/services/ANM/ServiciosANM/MapServer/2/query?where=CODIGO%253D'505315'", None);
        let t = image_template(&filtered).expect("a MapServer layer");
        assert!(t.ends_with("&layers=show:2&layerDefs=%7B%222%22%3A%22CODIGO%3D%27505315%27%22%7D"), "{t}");
        let image =
            entry("https://serviciosgis.catastrobogota.gov.co/image/rest/services/imagenesfunciones/ortho2010funcion/ImageServer", None);
        assert!(image_template(&image).is_some_and(|t| t.contains("/ImageServer/exportImage?bbox={bbox-epsg-3857}")));
        let service = entry("https://miciudad.barranquilla.gov.co/gis/rest/services/catastro/manzanas/MapServer", None);
        assert!(image_template(&service).is_some_and(|t| t.contains("/MapServer/export?")));
        assert_eq!(image_template(&entry("https://x/rest/services/a/FeatureServer/0", None)), None);
        assert_eq!(image_template(&entry("https://x/rest/services/a/FeatureServer", None)), None);
    }

    #[test]
    fn the_view_box() {
        let v = view_of(&json!({"center": [-74.08, 4.65], "scale": 50_000.0})).expect("a view");
        let [w, s, e, n] = v.bbox;
        // 50,000 × 0.0254 / 96 ≈ 13.2 m a pixel: 1600 × 1000 pixels ≈ 21 × 13 km.
        assert!((e - w - 21_166.7).abs() < 1.0 && (n - s - 13_229.2).abs() < 1.0, "{:?}", v.bbox);
        assert!(w < -8_246_000.0 && e > -8_246_000.0 && s < 518_000.0 && n > 518_000.0, "{:?}", v.bbox);
        assert_eq!(view_of(&json!({"center": [-74.0, 4.6], "scale": 0})), None);
        assert_eq!(view_of(&json!({"center": [-74.0], "scale": 1000})), None);
        assert_eq!(view_of(&json!(null)), None);
    }

    #[test]
    fn records_a_server_gives() {
        assert_eq!(max_records(&json!({"maxRecordCount": 2000})), 2000);
        assert_eq!(max_records(&json!({"maxRecordCount": 0})), DEFAULT_MAX_RECORDS);
        assert_eq!(max_records(&json!({"maxRecordCount": 99_999_999_999u64})), MAX_RECORDS);
        assert_eq!(max_records(&json!({"maxRecordCount": "2000"})), DEFAULT_MAX_RECORDS);
        assert_eq!(max_records(&json!(null)), DEFAULT_MAX_RECORDS);
    }

    #[test]
    fn encoding() {
        assert_eq!(encode("a='B' AND c<>2"), "a%3D%27B%27%20AND%20c%3C%3E2");
        assert_eq!(encode("Ñ"), "%C3%91");
        assert_eq!(encode(""), "");
    }
}
