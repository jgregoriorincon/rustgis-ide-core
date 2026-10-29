//! The ArcGIS addresses a plugin hands RustGIS: a layer's features in the map view, and a map
//! image as a tile template.
//!
//! RustGIS's `service.add` downloads an ArcGIS layer whole and can't apply a catalogue's `where`
//! (and the web app refuses it from a command), so the plugin asks the server itself:
//! `…/query` with the `where` and the view's box, read by `layer.addUrl`, or `…/export` as a
//! `map.addTiles` template ({bbox-epsg-3857}, {width} and {height} are RustGIS's placeholders).
//! Before that, `…/query` can count a layer's features in the view and give the box that holds
//! them, so a plugin can take the map to data that is far from it; a layer's description
//! declares that box too, at no cost, when the entry has no filter.

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

/// The scales a layer's description says it draws at: `(min_scale, max_scale)`, the most zoomed
/// out (the largest scale number) and the most zoomed in, each `None` when it has no limit
/// (ArcGIS writes `0`). A server draws no map image outside them, and a layer drawn from a
/// scale on is often too big to download zoomed out.
pub fn scale_range(layer: &Value) -> (Option<f64>, Option<f64>) {
    let scale = |k: &str| layer.get(k).and_then(Value::as_f64).filter(|s| s.is_finite() && *s > 0.0);
    match (scale("minScale"), scale("maxScale")) {
        // A range turned round: none, rather than one nothing fits in.
        (Some(lo), Some(hi)) if hi > lo => (None, None),
        range => range,
    }
}

/// How far inside a layer's scale range [`fit_scale`] goes, so the server draws it whatever it
/// rounds.
const INSIDE: f64 = 0.95;

/// The scale to go to when the map's `scale` (RustGIS's, at latitude `lat`) is outside `range`
/// (from [`scale_range`]): a little inside its nearest end, as RustGIS's scale. `None` when it
/// is within, or isn't a scale.
///
/// The two count differently: RustGIS a ground scale with 0.28 mm pixels, ArcGIS a Web Mercator
/// one with 96 dpi pixels, which at Bogotá's latitude reads about 6 % more. Comparing them as
/// they are would stop the map where the server still draws nothing.
pub fn fit_scale(scale: f64, lat: f64, (min_scale, max_scale): (Option<f64>, Option<f64>)) -> Option<f64> {
    let cos = lat.clamp(-85.0, 85.0).to_radians().cos();
    if !(scale.is_finite() && scale > 0.0 && lat.is_finite()) {
        return None;
    }
    // A RustGIS scale as ArcGIS reads the same view, and back.
    let to_arcgis = 0.000_28 / (0.0254 / 96.0) / cos;
    let arcgis = scale * to_arcgis;
    let target = match (min_scale, max_scale) {
        (Some(min), _) if arcgis > min => min * INSIDE,
        (_, Some(max)) if arcgis < max => max / INSIDE,
        _ => return None,
    };
    Some(target / to_arcgis)
}

/// The address of an ArcGIS layer's features as GeoJSON in longitude/latitude: those that pass
/// its filter, in `view` when given, at most `limit`. `None` for anything but an ArcGIS layer.
pub fn features_url(e: &Entry, view: Option<View>, limit: u32) -> Option<String> {
    query(e, view, &format!("outFields=*&returnGeometry=true&outSR=4326&f=geojson&resultRecordCount={limit}"))
}

/// The address that counts an ArcGIS layer's features that pass its filter in `view`: the
/// answer is `{"count": n}` (read it with [`count_of`]).
pub fn count_url(e: &Entry, view: View) -> Option<String> {
    query(e, Some(view), "returnCountOnly=true&f=json")
}

/// The address of the box that holds an ArcGIS layer's features that pass its filter, in
/// longitude/latitude: the answer is `{"extent": {"xmin", …}}` (read it with [`extent_of`]).
pub fn extent_url(e: &Entry) -> Option<String> {
    query(e, None, "returnExtentOnly=true&outSR=4326&f=json")
}

/// `…/query` of an ArcGIS layer with its filter, in `view` when given, and `rest`.
fn query(e: &Entry, view: Option<View>, rest: &str) -> Option<String> {
    if e.kind != Kind::ArcGisLayer {
        return None;
    }
    let filter = e.filter.as_deref().unwrap_or("1=1");
    let mut u = format!("{}/query?where={}&{rest}", e.url, encode(filter));
    if let Some(View { bbox: [w, s, east, n] }) = view {
        u.push_str(&format!(
            "&geometry={w:.0},{s:.0},{east:.0},{n:.0}&geometryType=esriGeometryEnvelope&inSR=3857&spatialRel=esriSpatialRelIntersects"
        ));
    }
    Some(u)
}

/// The count in a [`count_url`] answer; `None` when the server said something else (an error).
pub fn count_of(answer: &Value) -> Option<u64> {
    answer.get("count")?.as_u64()
}

/// The least width and height of a box to zoom to, in degrees (about 200 m): a layer of one
/// point, or of points on a line, has an empty box.
pub const MIN_SPAN: f64 = 0.002;

/// The box `[west, south, east, north]` in an [`extent_url`] answer, at least [`MIN_SPAN`] wide
/// and high, as `map.zoomToExtent` takes it. `None` when there is none: a layer without
/// features (whose box servers give as `NaN`s or not at all), an error, or a box that isn't in
/// longitude/latitude.
pub fn extent_of(answer: &Value) -> Option<[f64; 4]> {
    let [w, s, east, north] = corners(answer.get("extent")?)?;
    lonlat_box(w, s, east, north)
}

/// The box a layer's description (`…/MapServer/3?f=json`) declares, as [`extent_of`] gives it:
/// it comes with the description, while [`extent_url`]'s query can take a server most of a
/// minute on a big layer (Catastro's 934,099 lots). `None` for an entry with a filter (the
/// description's box is the whole layer's), or a box in another system than longitude/latitude
/// (EPSG:4326) or Web Mercator (EPSG:3857 and its old codes).
pub fn declared_extent(e: &Entry, layer: &Value) -> Option<[f64; 4]> {
    if e.filter.is_some() {
        return None;
    }
    let extent = layer.get("extent")?;
    let sr = extent.get("spatialReference")?;
    let code = |k: &str| sr.get(k).and_then(Value::as_u64);
    let [w, s, east, north] = corners(extent)?;
    match code("latestWkid").or_else(|| code("wkid"))? {
        4326 => lonlat_box(w, s, east, north),
        3857 | 102100 | 102113 | 900913 => {
            const R: f64 = 6_378_137.0;
            let lon = |x: f64| (x / R).to_degrees();
            let lat = |y: f64| (2.0 * (y / R).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
            lonlat_box(lon(w), lat(s), lon(east), lat(north))
        }
        _ => None,
    }
}

/// An ArcGIS extent's `[xmin, ymin, xmax, ymax]`, when all four are numbers.
fn corners(e: &Value) -> Option<[f64; 4]> {
    let n = |k: &str| e.get(k).and_then(Value::as_f64).filter(|v| v.is_finite());
    Some([n("xmin")?, n("ymin")?, n("xmax")?, n("ymax")?])
}

/// A box in longitude/latitude, widened to [`MIN_SPAN`]; `None` when it isn't one.
fn lonlat_box(w: f64, s: f64, east: f64, north: f64) -> Option<[f64; 4]> {
    let lonlat = |x: f64, lim: f64| (-lim..=lim).contains(&x);
    if !(lonlat(w, 180.0) && lonlat(east, 180.0) && lonlat(s, 90.0) && lonlat(north, 90.0)) || w > east || s > north {
        return None;
    }
    let widen = |lo: f64, hi: f64, lim: f64| {
        let pad = ((MIN_SPAN - (hi - lo)) / 2.0).max(0.0);
        ((lo - pad).max(-lim), (hi + pad).min(lim))
    };
    let ((w, east), (s, north)) = (widen(w, east, 180.0), widen(s, north, 90.0));
    Some([w, s, east, north])
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
    fn counting_in_the_view_and_the_box_of_the_data() {
        let peajes =
            entry("https://hermes2.invias.gov.co/server/rest/services/MapaCarreteras/Peaje/FeatureServer/0/query?where=a%253D'B'", None);
        let view = View { bbox: [-8_250_000.4, 520_000.0, -8_230_000.0, 530_000.6] };
        let base = "https://hermes2.invias.gov.co/server/rest/services/MapaCarreteras/Peaje/FeatureServer/0/query?where=a%3D%27B%27";
        assert_eq!(
            count_url(&peajes, view).as_deref(),
            Some(format!("{base}&returnCountOnly=true&f=json&geometry=-8250000,520000,-8230000,530001&geometryType=esriGeometryEnvelope&inSR=3857&spatialRel=esriSpatialRelIntersects").as_str())
        );
        assert_eq!(extent_url(&peajes).as_deref(), Some(format!("{base}&returnExtentOnly=true&outSR=4326&f=json").as_str()));
        let service = entry("https://x/rest/services/a/MapServer", None);
        assert_eq!((count_url(&service, view), extent_url(&service)), (None, None));

        assert_eq!(count_of(&json!({"count": 40})), Some(40));
        for junk in [json!({"error": {"code": 400}}), json!({"count": -1}), json!({"count": "3"}), json!(null)] {
            assert_eq!(count_of(&junk), None, "{junk}");
        }
    }

    #[test]
    fn the_scales_a_layer_draws_at() {
        // IDECA's Lote draws from 1:10,000 in; Manzana Estratificación between 1:1,200,000 and 1:500.
        let lote = scale_range(&json!({"minScale": 10000, "maxScale": 0}));
        assert_eq!(lote, (Some(10_000.0), None));
        assert_eq!(scale_range(&json!({"minScale": 1_200_000.0, "maxScale": 500})), (Some(1_200_000.0), Some(500.0)));
        for none in
            [json!({"minScale": 0, "maxScale": 0}), json!({}), json!({"minScale": "10000"}), json!({"minScale": 500, "maxScale": 1000})]
        {
            assert_eq!(scale_range(&none), (None, None), "{none}");
        }
        // A RustGIS scale as the server reads it at Bogotá's latitude, so tests speak ArcGIS's.
        const LAT: f64 = 4.6;
        let arcgis = |s: f64| s * 0.000_28 / (0.0254 / 96.0) / LAT.to_radians().cos();
        let fit = |s: f64, range| fit_scale(s, LAT, range).map(arcgis);
        let near = |got: Option<f64>, want: f64| got.is_some_and(|g| (g - want).abs() < 1e-6 * want);
        assert!(near(fit(50_000.0, lote), 9_500.0), "zoomed out: in to a little inside where it draws");
        assert_eq!(fit(8_000.0, lote), None);
        // 1:9,600 in RustGIS is about 1:10,200 to the server, which wouldn't draw Lote.
        assert!(near(fit(9_600.0, lote), 9_500.0));
        let both = (Some(1_200_000.0), Some(500.0));
        assert!(near(fit(100.0, both), 500.0 / 0.95), "zoomed in too far: out");
        assert!(near(fit(5_000_000.0, both), 1_140_000.0));
        assert_eq!(fit(100_000.0, both), None);
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(fit_scale(bad, LAT, both), None);
        }
        assert_eq!(fit_scale(50_000.0, f64::NAN, both), None);
        assert_eq!(fit_scale(1e9, LAT, (None, None)), None);
    }

    #[test]
    fn the_box_to_zoom_to() {
        let b = |xmin: f64, ymin: f64, xmax: f64, ymax: f64| json!({"extent": {"xmin": xmin, "ymin": ymin, "xmax": xmax, "ymax": ymax, "spatialReference": {"wkid": 4326}}});
        assert_eq!(extent_of(&b(-74.2, 4.5, -74.0, 4.8)), Some([-74.2, 4.5, -74.0, 4.8]));
        // One point: widened to the least span around it.
        let [w, s, e, n] = extent_of(&b(-74.1, 4.6, -74.1, 4.6)).expect("a point's box");
        assert!((e - w - MIN_SPAN).abs() < 1e-12 && (n - s - MIN_SPAN).abs() < 1e-12 && w < -74.1 && e > -74.1, "{w} {s} {e} {n}");
        // At the world's edge it stays within it.
        let [w, _, e, _] = extent_of(&b(180.0, 0.0, 180.0, 1.0)).expect("a box");
        assert!(e <= 180.0 && e > w, "{w} {e}");
        // No features: ArcGIS answers NaN (as a string or not at all); an error; a projected box.
        for junk in [
            json!({"extent": {"xmin": "NaN", "ymin": "NaN", "xmax": "NaN", "ymax": "NaN"}}),
            json!({"extent": {}}),
            json!({"error": {"code": 400}}),
            b(-8_250_000.0, 520_000.0, -8_230_000.0, 530_000.0),
            b(-74.0, 4.5, -74.2, 4.8),
            json!(null),
        ] {
            assert_eq!(extent_of(&junk), None, "{junk}");
        }
    }

    #[test]
    fn the_box_a_layer_declares() {
        let layer = |xmin: f64, ymin: f64, xmax: f64, ymax: f64, sr: Value| {
            json!({"extent": {"xmin": xmin, "ymin": ymin, "xmax": xmax, "ymax": ymax, "spatialReference": sr}})
        };
        let lote = entry(LOTE, None);
        // Catastro's lots, as their description gives them.
        let declared = layer(-74.450724163, 3.73067253300002, -73.989018105, 4.83695168200001, json!({"wkid": 4326, "latestWkid": 4326}));
        assert_eq!(declared_extent(&lote, &declared), Some([-74.450724163, 3.73067253300002, -73.989018105, 4.83695168200001]));
        // Web Mercator, by its code or its old one.
        for sr in [json!({"wkid": 3857}), json!({"wkid": 102100, "latestWkid": 3857}), json!({"wkid": 102100})] {
            let [w, s, e, n] = declared_extent(&lote, &layer(-8_250_000.0, 520_000.0, -8_230_000.0, 530_000.0, sr.clone())).expect("a box");
            assert!((w + 74.1110).abs() < 1e-3 && (e + 73.9313).abs() < 1e-3, "{sr}: {w} {e}");
            assert!((s - 4.6661).abs() < 1e-3 && (n - 4.7556).abs() < 1e-3, "{sr}: {s} {n}");
        }
        // A filtered entry's data isn't the whole layer's; another system; no box; no features.
        let filtered =
            entry("https://hermes2.invias.gov.co/server/rest/services/MapaCarreteras/Peaje/FeatureServer/0/query?where=a%253D'B'", None);
        assert_eq!(declared_extent(&filtered, &declared), None);
        for junk in [
            layer(4_800_000.0, 1_990_000.0, 4_900_000.0, 2_100_000.0, json!({"wkid": 9377})),
            layer(-74.2, 4.5, -74.0, 4.8, json!({})),
            json!({"extent": {"xmin": "NaN", "ymin": "NaN", "xmax": "NaN", "ymax": "NaN", "spatialReference": {"wkid": 4326}}}),
            json!({"name": "Lote"}),
            json!(null),
        ] {
            assert_eq!(declared_extent(&lote, &junk), None, "{junk}");
        }
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
