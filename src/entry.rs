//! One service of a catalogue, whatever catalogue it came from.

use crate::kind::Kind;
use crate::url::split;

/// Longest text kept of a catalogue's field: descriptions run to pages, and the panel shows a
/// few lines.
pub const MAX_TEXT: usize = 600;
/// Longest address kept: a longer one is no service address.
pub const MAX_URL: usize = 4_096;

/// One service. Only `id`, `name` and `url` are required; a catalogue fills what it has.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The catalogue's own id, kept so an embedded index and the live catalogue match.
    pub id: String,
    pub name: String,
    /// The service's address: an ArcGIS layer without its query, or an OGC capabilities address.
    pub url: String,
    /// A `where` the catalogue put in an ArcGIS address ([`split`]).
    pub filter: Option<String>,
    pub kind: Kind,
    /// Its theme: a field (IDECA's CATEGORIA) or where it sits (Barranquilla's themes).
    pub category: Option<String>,
    /// A group within its theme (IDECA's SUBCATEGORIA: Arborización within Ambiente).
    pub subcategory: Option<String>,
    /// Who publishes it.
    pub entity: Option<String>,
    pub licence: Option<String>,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub metadata_url: Option<String>,
    /// The same data as other services (WMS, WFS…): the catalogue's `URL2` or `servicios`.
    pub alternates: Vec<(Kind, String)>,
    /// The catalogue hides it in its own viewer (`visible: false`).
    pub hidden: bool,
}

/// What a catalogue says of a service, before [`Entry::new`] checks it.
#[derive(Debug, Default, Clone)]
pub struct Raw<'a> {
    pub id: Option<&'a str>,
    pub name: Option<&'a str>,
    pub url: Option<&'a str>,
    /// The catalogue's label for the service (`FeatureLayer`, `SceneLayer`…).
    pub label: Option<&'a str>,
    pub category: Option<&'a str>,
    pub subcategory: Option<&'a str>,
    pub entity: Option<&'a str>,
    pub licence: Option<&'a str>,
    pub description: Option<&'a str>,
    /// Comma-separated, as catalogues give them.
    pub keywords: Option<&'a str>,
    pub metadata_url: Option<&'a str>,
    /// A `where` kept apart from the address (an embedded index stores it so); one in the
    /// address comes first.
    pub filter: Option<&'a str>,
    /// Other addresses, each with the catalogue's label.
    pub alternates: Vec<(Option<&'a str>, &'a str)>,
    pub hidden: bool,
}

impl Entry {
    /// An entry from what a catalogue says, or `None` when it lacks an id, a name or an
    /// `http(s)` address of a service RustGIS knows.
    pub fn new(raw: &Raw) -> Option<Entry> {
        let id = text(raw.id)?;
        let name = text(raw.name)?;
        let address = web_url(raw.url)?;
        let kind = Kind::from_url(&address, raw.label)?;
        let (url, filter) = match kind {
            Kind::ArcGisLayer | Kind::ArcGisService | Kind::ArcGisImage => split(&address),
            _ => (address, None),
        };
        let filter = filter.or_else(|| text(raw.filter));
        let alternates = raw
            .alternates
            .iter()
            .filter_map(|(label, u)| {
                let u = web_url(Some(u))?;
                Some((Kind::from_url(&u, *label)?, u))
            })
            .filter(|(k, u)| k.supported() && *u != url)
            .collect();
        Some(Entry {
            id,
            name,
            url,
            filter,
            kind,
            category: text(raw.category),
            subcategory: text(raw.subcategory),
            entity: text(raw.entity),
            licence: text(raw.licence),
            description: text(raw.description),
            keywords: raw.keywords.unwrap_or("").split(',').filter_map(|k| text(Some(k))).collect(),
            metadata_url: web_url(raw.metadata_url),
            alternates,
            hidden: raw.hidden,
        })
    }
}

/// A catalogue's list, fit to show: the first of entries with the same id, then the first of
/// entries with the same address and filter (catalogues list some services twice).
pub fn clean(entries: impl IntoIterator<Item = Entry>) -> Vec<Entry> {
    let mut ids = std::collections::HashSet::new();
    let mut services = std::collections::HashSet::new();
    entries
        .into_iter()
        .filter(|e| ids.insert(e.id.clone()))
        .filter(|e| services.insert((e.url.to_ascii_lowercase(), e.filter.clone())))
        .collect()
}

/// Trimmed text, clipped to [`MAX_TEXT`] characters; `None` when empty.
fn text(s: Option<&str>) -> Option<String> {
    let s = s?.trim();
    (!s.is_empty()).then(|| s.chars().take(MAX_TEXT).collect())
}

/// An `http(s)` address, trimmed; `None` for anything else.
fn web_url(s: Option<&str>) -> Option<String> {
    let s = s?.trim();
    let lower = s.get(..8).unwrap_or(s).to_ascii_lowercase();
    let web = lower.starts_with("https://") || lower.starts_with("http://");
    (web && s.len() <= MAX_URL && !s.contains(char::is_whitespace)).then(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw<'a>(id: &'a str, url: &'a str) -> Raw<'a> {
        Raw { id: Some(id), name: Some("Lote"), url: Some(url), ..Raw::default() }
    }

    const LOTE: &str = "https://serviciosgis.catastrobogota.gov.co/arcgis/rest/services/catastro/lote/MapServer/0";

    #[test]
    fn an_entry_needs_an_id_a_name_and_a_service_address() {
        assert!(Entry::new(&raw("1", LOTE)).is_some());
        assert!(Entry::new(&raw(" ", LOTE)).is_none());
        assert!(Entry::new(&Raw { name: None, ..raw("1", LOTE) }).is_none());
        assert!(Entry::new(&raw("1", "")).is_none());
        assert!(Entry::new(&raw("1", "ftp://x/MapServer/0")).is_none());
        assert!(Entry::new(&raw("1", "javascript:alert(1)//MapServer/0")).is_none());
        assert!(Entry::new(&raw("1", "https://datosabiertos.bogota.gov.co/dataset/x")).is_none());
    }

    #[test]
    fn the_filter_leaves_the_address() {
        let url = "https://hermes2.invias.gov.co/server/rest/services/MapaCarreteras/Peaje/FeatureServer/0/query?where=a%253D'B'";
        let e = Entry::new(&raw("3-0-6", url)).expect("a valid entry");
        assert_eq!(e.url, "https://hermes2.invias.gov.co/server/rest/services/MapaCarreteras/Peaje/FeatureServer/0");
        assert_eq!(e.filter.as_deref(), Some("a='B'"));
        assert_eq!(e.kind, Kind::ArcGisLayer);
    }

    #[test]
    fn fields_are_trimmed_clipped_and_split() {
        let long = "x".repeat(MAX_TEXT + 50);
        let e = Entry::new(&Raw {
            description: Some(&long),
            keywords: Some(" Catastro, Lote ,, Predio "),
            licence: Some(""),
            metadata_url: Some("{A8AB6E7A-55FB-40DA-85E1-0AA6B4FE6CE3}"),
            ..raw("1", LOTE)
        })
        .expect("a valid entry");
        assert_eq!(e.description.map(|d| d.chars().count()), Some(MAX_TEXT));
        assert_eq!(e.keywords, ["Catastro", "Lote", "Predio"]);
        assert_eq!(e.licence, None);
        assert_eq!(e.metadata_url, None);
    }

    #[test]
    fn alternates_keep_what_rustgis_shows() {
        let wms =
            "https://miciudad.barranquilla.gov.co/gis/services/catastro/manzanas/MapServer/WMSServer?request=GetCapabilities&service=WMS";
        let e = Entry::new(&Raw {
            alternates: vec![(Some("WFSLayer"), wms), (Some("WMSLayer"), ""), (Some("SceneLayer"), wms), (None, LOTE)],
            ..raw("1", LOTE)
        })
        .expect("a valid entry");
        assert_eq!(e.alternates, [(Kind::Wms, wms.to_string())]);
    }

    #[test]
    fn clean_drops_repeated_ids_and_services() {
        let a = Entry::new(&raw("15-3-3", LOTE)).expect("a valid entry");
        let same_service = Entry { id: "15-0-1".into(), ..a.clone() };
        let same_id = Entry { url: "https://x/MapServer/1".into(), ..a.clone() };
        let other = Entry { id: "2".into(), url: "https://x/MapServer/2".into(), ..a.clone() };
        assert_eq!(clean([a.clone(), same_service, same_id, other.clone()]), [a, other]);
    }
}
