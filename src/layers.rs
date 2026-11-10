//! The map's layers a plugin added, told from the user's own.
//!
//! A plugin replaces the layers it added (Add Features again, Update to View), and must leave alone
//! a layer the user added: the same service through Add From Service, under the same name, has the
//! same features query as its source. So the features queries a plugin adds carry a parameter of
//! its own, [`mark`], which ArcGIS servers ignore, and RustGIS keeps the address the plugin passed
//! as the layer's source: a layer is the plugin's only when its source has the mark.
//!
//! Checked on IDECA's Lote and on Barranquilla's Barrios: the same count, and the same `f=pbf`
//! answer, with and without the mark.

use crate::arcgis::{self, View};
use crate::entry::Entry;

/// The parameter `plugin` (its id in `plugin.json`: letters, digits and hyphens) adds to its
/// features queries: `rustgis-plugin=ide-bogota`.
pub fn mark(plugin: &str) -> String {
    format!("rustgis-plugin={plugin}")
}

/// `e`'s features query ([`arcgis::features_url`]) with `plugin`'s [`mark`]: the address a plugin
/// passes to `layer.addUrl`.
pub fn features_url(e: &Entry, view: Option<View>, plugin: &str) -> Option<String> {
    arcgis::features_url(e, view).map(|url| format!("{url}&{}", mark(plugin)))
}

/// Whether a layer named `name` may be `e`'s: its name, or its name and ` (n)`, as RustGIS names a
/// layer added while one of its name is on the map. The name alone doesn't make it `e`'s: see
/// [`added_by`].
pub fn named(e: &Entry, name: &str) -> bool {
    match name.strip_prefix(e.name.as_str()) {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix(" (")
            .and_then(|r| r.strip_suffix(')'))
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
        None => false,
    }
}

/// Whether a layer whose source is `source` (as `layer.copyPath` gives it) is one `plugin` added
/// of `e`: `e`'s features query (up to `/query?`, so layer 65's query isn't layer 652's), with
/// `plugin`'s [`mark`]. The same query added by hand, or by a plugin before it marked its queries,
/// isn't the plugin's.
pub fn added_by(e: &Entry, source: &str, plugin: &str) -> bool {
    let mark = mark(plugin);
    source
        .strip_prefix(e.url.as_str())
        .and_then(|rest| rest.strip_prefix("/query?"))
        .is_some_and(|params| params.split('&').any(|p| p == mark))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Raw;

    fn entry(name: &str, url: &str) -> Entry {
        Entry::new(&Raw { id: Some("1"), name: Some(name), url: Some(url), ..Raw::default() }).expect("a valid entry")
    }

    const LOTE: &str = "https://serviciosgis.catastrobogota.gov.co/arcgis/rest/services/catastro/lote/MapServer/0";
    const BARRIOS: &str = "https://miciudad.barranquilla.gov.co/gis/rest/services/ordenamiento/unidadesadministrativas/MapServer/1";

    #[test]
    fn a_layer_is_the_plugins_by_its_marked_source() {
        let lote = entry("Lote", LOTE);
        let view = View { bbox: [-8_250_000.0, 520_000.0, -8_230_000.0, 530_000.0] };
        for v in [None, Some(view)] {
            let url = features_url(&lote, v, "ide-bogota").expect("a layer");
            assert!(url.ends_with("&rustgis-plugin=ide-bogota"), "{url}");
            assert!(added_by(&lote, &url, "ide-bogota"), "{url}");
            // Another plugin's layer of the same service isn't this one's.
            assert!(!added_by(&lote, &url, "ide-barranquilla"), "{url}");
            // The same query unmarked: added by hand, or by an older plugin.
            let unmarked = arcgis::features_url(&lote, v).expect("a layer");
            assert!(!added_by(&lote, &unmarked, "ide-bogota"), "{unmarked}");
        }
        // A parameter that only starts as the mark.
        let url = features_url(&lote, None, "ide-bogota").expect("a layer");
        assert!(!added_by(&lote, &format!("{url}x"), "ide-bogota"));
        assert!(!added_by(&lote, &url.replace("ide-bogota", "ide-bogota-x"), "ide-bogota"));
        // Layer 65's query isn't layer 652's.
        let lote_65 = entry("Lote", &LOTE.replace("MapServer/0", "MapServer/65"));
        let url_652 = features_url(&entry("Lote", &LOTE.replace("MapServer/0", "MapServer/652")), None, "ide-bogota").expect("a layer");
        assert!(!added_by(&lote_65, &url_652, "ide-bogota"));
        // A layer from a file, or without a source.
        assert!(!added_by(&lote, "/home/user/lote.geojson", "ide-bogota"));
        assert!(!added_by(&lote, "", "ide-bogota"));
        // Not a layer: nothing to add, nothing to own.
        assert_eq!(features_url(&entry("Catastro", "https://x/rest/services/a/MapServer"), None, "ide-bogota"), None);
    }

    #[test]
    fn a_filtered_layer_of_barranquilla() {
        // As geobaq writes a filter: the `=` and the space encoded twice.
        let barrios = entry("Barrios", &format!("{BARRIOS}/query?where=nombre%253D'El%2520Prado'"));
        let url = features_url(&barrios, None, "ide-barranquilla").expect("a layer");
        assert!(url.starts_with(&format!("{BARRIOS}/query?where=nombre%3D%27El")), "{url}");
        assert!(added_by(&barrios, &url, "ide-barranquilla"), "{url}");
    }

    #[test]
    fn a_name_and_rustgis_copies_of_it() {
        let lote = entry("Lote", LOTE);
        for name in ["Lote", "Lote (2)", "Lote (13)"] {
            assert!(named(&lote, name), "{name}");
        }
        for name in ["Lotes", "Lote (a)", "Lote ()", "Lote (2) copy", "Lote(2)", "lote", ""] {
            assert!(!named(&lote, name), "{name}");
        }
    }
}
