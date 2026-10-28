//! The list as a plugin embeds it, and how it compares with the live catalogue.
//!
//! A plugin carries an index of its catalogue, made by its refresh script, and the panel walks
//! that index. A plugin keeps nothing between calls, so reading the catalogue live would read
//! all of it on every click, which a command's fuel can't pay for (IDECA's is 665 KB, IGAC's
//! 25 MB). The live catalogue is read on demand only to tell whether the index is out of date
//! ([`freshness`]). The index is read with the same checks as the catalogue ([`Entry::new`]):
//! it is generated, but from data that isn't ours.

use crate::entry::{Entry, Raw, clean};
use serde_json::{Map, Value, json};

/// A live list shorter than this share of the index is taken as a broken answer (an error page
/// that parsed, a truncated document), not as services withdrawn.
pub const MIN_LIVE_SHARE: f64 = 0.5;

/// How the index stands against the live catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// The same services.
    UpToDate,
    /// Services the catalogue added and withdrew since the index was made.
    Changed { added: usize, removed: usize },
    /// The live answer had too few services to be the catalogue.
    Incomplete,
}

/// The index document: `{"date": "2026-10-24", "entries": [{"id", "name", "url", …}]}`, with
/// only the fields an entry has.
pub fn write(date: &str, entries: &[Entry]) -> Value {
    let entries: Vec<Value> = entries.iter().map(entry_json).collect();
    json!({"date": date, "entries": entries})
}

/// The index's date and entries. A document that isn't an index gives no entries.
pub fn read(doc: &Value) -> (String, Vec<Entry>) {
    let date = doc.get("date").and_then(Value::as_str).unwrap_or("").to_string();
    let list = doc.get("entries").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    (date, clean(list.iter().filter_map(entry_from)))
}

/// How `index` stands against the `live` catalogue, read with the same reader, by id.
pub fn freshness(index: &[Entry], live: &[Entry]) -> Freshness {
    if live.is_empty() || (live.len() as f64) < index.len() as f64 * MIN_LIVE_SHARE {
        return Freshness::Incomplete;
    }
    let ids = |list: &[Entry]| list.iter().map(|e| e.id.clone()).collect::<std::collections::HashSet<_>>();
    let (old, new) = (ids(index), ids(live));
    match (new.difference(&old).count(), old.difference(&new).count()) {
        (0, 0) => Freshness::UpToDate,
        (added, removed) => Freshness::Changed { added, removed },
    }
}

fn entry_json(e: &Entry) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), e.id.clone().into());
    m.insert("name".into(), e.name.clone().into());
    m.insert("url".into(), e.url.clone().into());
    let optional = [
        ("filter", &e.filter),
        ("category", &e.category),
        ("subcategory", &e.subcategory),
        ("entity", &e.entity),
        ("licence", &e.licence),
        ("description", &e.description),
        ("metadata", &e.metadata_url),
    ];
    for (key, value) in optional {
        if let Some(v) = value {
            m.insert(key.into(), v.clone().into());
        }
    }
    if let Some(label) = e.kind.label() {
        m.insert("label".into(), label.into());
    }
    if !e.keywords.is_empty() {
        m.insert("keywords".into(), e.keywords.join(", ").into());
    }
    if !e.alternates.is_empty() {
        m.insert("alternates".into(), e.alternates.iter().map(|(_, u)| Value::from(u.as_str())).collect());
    }
    if e.hidden {
        m.insert("hidden".into(), true.into());
    }
    Value::Object(m)
}

fn entry_from(v: &Value) -> Option<Entry> {
    let s = |key: &str| v.get(key).and_then(Value::as_str);
    let alternates = v.get("alternates").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    Entry::new(&Raw {
        id: s("id"),
        name: s("name"),
        url: s("url"),
        label: s("label"),
        filter: s("filter"),
        category: s("category"),
        subcategory: s("subcategory"),
        entity: s("entity"),
        licence: s("licence"),
        description: s("description"),
        keywords: s("keywords"),
        metadata_url: s("metadata"),
        alternates: alternates.iter().filter_map(Value::as_str).map(|u| (None, u)).collect(),
        hidden: v.get("hidden").and_then(Value::as_bool).unwrap_or(false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kind;

    fn entries() -> Vec<Entry> {
        let baq = Raw {
            id: Some("1-0-1"),
            name: Some("Valorización por vía"),
            url: Some(
                "https://hermes2.invias.gov.co/server/rest/services/ValorizacionPredial/ValorizacionPredialV2/MapServer/1/query?where=municipio_1%253D'BARRANQUILLA'",
            ),
            label: Some("FeatureLayer"),
            category: Some("Catastro"),
            subcategory: Some("Valorización"),
            licence: Some("CC BY - SA 4.0"),
            keywords: Some("Valorización predial, INVIAS"),
            hidden: true,
            ..Raw::default()
        };
        let scene = Raw {
            id: Some("1-98"),
            name: Some("Construcciones 3D"),
            url: Some("https://miciudad.barranquilla.gov.co/server/rest/services/Construcciones/SceneServer"),
            label: Some("SceneLayer"),
            ..Raw::default()
        };
        let table = Raw {
            id: Some("1-21"),
            name: Some("Predios"),
            url: Some("https://miciudad.barranquilla.gov.co/server/rest/services/catastro/datosabiertos/MapServer/500"),
            label: Some("FeatureTable"),
            alternates: vec![(
                Some("WMSLayer"),
                "https://miciudad.barranquilla.gov.co/server/services/catastro/datosabiertos/MapServer/WMSServer?request=GetCapabilities&service=WMS",
            )],
            ..Raw::default()
        };
        [baq, scene, table].iter().filter_map(Entry::new).collect()
    }

    #[test]
    fn an_index_reads_back_what_it_wrote() {
        let written = entries();
        assert_eq!(written.len(), 3);
        let doc = write("2026-10-24", &written);
        assert_eq!(read(&doc), ("2026-10-24".to_string(), written));
        // The filter is kept apart from the address.
        assert_eq!(doc["entries"][0]["filter"], "municipio_1='BARRANQUILLA'");
        assert_eq!(read(&doc).1[2].kind, Kind::Unsupported("tables without geometry"));
    }

    #[test]
    fn a_broken_index_gives_no_entries() {
        assert_eq!(read(&json!(null)), (String::new(), vec![]));
        assert_eq!(read(&json!({"entries": "x"})).1, vec![]);
        let junk = json!({"date": 3, "entries": [1, null, {}, {"id": "a", "name": "b", "url": "file:///etc/passwd"}]});
        assert_eq!(read(&junk), (String::new(), vec![]));
    }

    #[test]
    fn the_index_against_the_live_catalogue() {
        let index = entries();
        assert_eq!(freshness(&index, &index), Freshness::UpToDate);
        let mut live = index.clone();
        live.reverse();
        assert_eq!(freshness(&index, &live), Freshness::UpToDate);
        live.pop();
        live.push(Entry { id: "new".into(), ..index[0].clone() });
        live.push(Entry { id: "newer".into(), ..index[0].clone() });
        assert_eq!(freshness(&index, &live), Freshness::Changed { added: 2, removed: 1 });
        // Fewer than half the index: a broken answer, not services withdrawn.
        assert_eq!(freshness(&index, &index[..1]), Freshness::Incomplete);
        assert_eq!(freshness(&index, &[]), Freshness::Incomplete);
        assert_eq!(freshness(&[], &index), Freshness::Changed { added: 3, removed: 0 });
    }
}
