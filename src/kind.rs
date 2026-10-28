//! What a service is, read from its address.
//!
//! Catalogues label their services, but the labels can't be trusted: Barranquilla calls a
//! `…/WMSServer?…service=WMS` address a `WFSLayer` and a `WMTSCapabilities.xml` a
//! `FeatureLayer`. The address decides; the label only marks what RustGIS can't show
//! ([`Kind::from_url`]).

/// A service's kind, as RustGIS adds it to the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One layer of an ArcGIS MapServer or FeatureServer (`…/MapServer/3`).
    ArcGisLayer,
    /// A whole ArcGIS MapServer or FeatureServer (`…/MapServer`).
    ArcGisService,
    /// An ArcGIS ImageServer.
    ArcGisImage,
    Wms,
    Wfs,
    Wmts,
    /// A GeoJSON document.
    GeoJson,
    /// Something RustGIS doesn't show; the reason, for the panel.
    Unsupported(&'static str),
}

impl Kind {
    /// The kind of the service at `url`, with the catalogue's `label` for what the address
    /// doesn't tell (3D scenes, tables). `None` when the address names no service RustGIS knows.
    pub fn from_url(url: &str, label: Option<&str>) -> Option<Kind> {
        let label = label.unwrap_or("").to_ascii_lowercase();
        if label.contains("scene") {
            return Some(Kind::Unsupported("3D scene layers"));
        }
        if label.contains("table") {
            return Some(Kind::Unsupported("tables without geometry"));
        }
        let lower = url.to_ascii_lowercase();
        let (path, query) = lower.split_once('?').unwrap_or((&lower, ""));
        let path = path.trim_end_matches('/');
        let service = query_value(query, "service");
        // OGC first: an ArcGIS server publishes them under …/MapServer/WMSServer too.
        if path.ends_with("wmtscapabilities.xml") || path.contains("/wmts") || service == Some("wmts") {
            return Some(Kind::Wmts);
        }
        if path.ends_with("/wfsserver") || service == Some("wfs") {
            return Some(Kind::Wfs);
        }
        if path.ends_with("/wmsserver") || service == Some("wms") {
            return Some(Kind::Wms);
        }
        if path.ends_with(".geojson") || (query_value(query, "f") == Some("geojson") && !path.contains("/rest/services/")) {
            return Some(Kind::GeoJson);
        }
        if path.ends_with("/sceneserver") {
            return Some(Kind::Unsupported("3D scene layers"));
        }
        if path.ends_with("/imageserver") {
            return Some(Kind::ArcGisImage);
        }
        let segments: Vec<&str> = path.rsplit('/').take(3).collect();
        match segments.as_slice() {
            // …/MapServer/3/query, …/MapServer/3, …/MapServer
            ["query", n, "mapserver" | "featureserver", ..] | [n, "mapserver" | "featureserver", ..] if is_number(n) => {
                Some(Kind::ArcGisLayer)
            }
            ["mapserver" | "featureserver", ..] => Some(Kind::ArcGisService),
            _ => None,
        }
    }

    /// Whether RustGIS can add it to the map.
    pub fn supported(self) -> bool {
        !matches!(self, Kind::Unsupported(_))
    }

    /// The label that brings back a kind the address alone doesn't tell, for an embedded index
    /// ([`Kind::from_url`] reads it).
    pub fn label(self) -> Option<&'static str> {
        match self {
            Kind::Unsupported(reason) if reason.contains("scene") => Some("SceneLayer"),
            Kind::Unsupported(_) => Some("FeatureTable"),
            _ => None,
        }
    }
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// The value of `key` in a lower-cased query string.
fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| pair.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
}

#[cfg(test)]
mod tests {
    use super::Kind::{self, *};

    fn kind(url: &str) -> Option<Kind> {
        Kind::from_url(url, None)
    }

    #[test]
    fn arcgis_layers_and_services() {
        assert_eq!(kind("https://serviciosgis.catastrobogota.gov.co/arcgis/rest/services/catastro/lote/MapServer/0"), Some(ArcGisLayer));
        assert_eq!(
            kind("https://miciudad.barranquilla.gov.co/server/rest/services/catastro/datosabiertos/FeatureServer/315"),
            Some(ArcGisLayer)
        );
        assert_eq!(kind("https://miciudad.barranquilla.gov.co/gis/rest/services/catastro/manzanas/MapServer"), Some(ArcGisService));
        assert_eq!(
            kind("https://geo.upme.gov.co/server/rest/services/Capas_Hidrocarburos/estaciones_servicio/FeatureServer"),
            Some(ArcGisService)
        );
        assert_eq!(
            kind("https://serviciosgis.catastrobogota.gov.co/image/rest/services/imagenesfunciones/ortho2010funcion/ImageServer"),
            Some(ArcGisImage)
        );
    }

    #[test]
    fn a_query_is_its_layer() {
        let url = "https://hermes2.invias.gov.co/server/rest/services/ValorizacionPredial/ValorizacionPredialV2/MapServer/1/query?where=municipio_1%253D'BARRANQUILLA'";
        assert_eq!(kind(url), Some(ArcGisLayer));
        let url = "https://geo.upme.gov.co/server/rest/services/Capas_Hidrocarburos/nodos_oferta_demanda/FeatureServer/2/query?where=municipio%3D%27Barranquilla%27&outFields=&f=html";
        assert_eq!(kind(url), Some(ArcGisLayer));
    }

    #[test]
    fn the_address_wins_over_a_wrong_label() {
        // Barranquilla's "WFSLayer" pointing at a WMS, "FeatureLayer" at a WMTS.
        let wms = "https://miciudad.barranquilla.gov.co/gis/services/ordenamiento/unidadesadministrativas/MapServer/WMSServer?request=GetCapabilities&service=WMS";
        assert_eq!(Kind::from_url(wms, Some("WFSLayer")), Some(Wms));
        let wmts = "https://miciudad.barranquilla.gov.co/image/rest/services/orto/orto8_MAS_08001urbano_2016/ImageServer/WMTS/1.0.0/WMTSCapabilities.xml";
        assert_eq!(Kind::from_url(wmts, Some("FeatureLayer")), Some(Wmts));
        let wfs = "https://serviciosgis.catastrobogota.gov.co/arcgis/services/desarrolloeconomico/productividadrural/MapServer/WFSServer?request=GetCapabilities&service=WFS";
        assert_eq!(kind(wfs), Some(Wfs));
    }

    #[test]
    fn ogc_on_other_servers() {
        assert_eq!(
            kind("https://gisdev.ambientebogota.gov.co/mapserv?map=totalnocturnoord&SERVICE=WMS&VERSION=1.3.0&REQUEST=GetCapabilities"),
            Some(Wms)
        );
        assert_eq!(kind("https://app.zonadeparqueopago.gov.co/api/v1/public/tramos.geojson"), Some(GeoJson));
    }

    #[test]
    fn what_rustgis_does_not_show() {
        assert_eq!(
            kind("https://miciudad.barranquilla.gov.co/server/rest/services/Construcciones/SceneServer"),
            Some(Unsupported("3D scene layers"))
        );
        let table = "https://miciudad.barranquilla.gov.co/server/rest/services/catastro/datosabiertos/MapServer/500";
        assert_eq!(Kind::from_url(table, Some("FeatureTable")), Some(Unsupported("tables without geometry")));
        assert!(!Unsupported("x").supported() && ArcGisLayer.supported());
    }

    #[test]
    fn unknown_addresses() {
        assert_eq!(kind(""), None);
        assert_eq!(kind("https://datosabiertos.bogota.gov.co/dataset/dinamica-empresarial-bogota"), None);
        assert_eq!(kind("not a url ?&=&="), None);
    }
}
