//! An ArcGIS address split into its layer and the `where` a catalogue put in it.
//!
//! Barranquilla's catalogue points at national services narrowed to the city with a query:
//! `…/MapServer/1/query?where=municipio_1%253D'BARRANQUILLA'`. Passed on as it is, the layer
//! either doesn't load or brings the whole country. [`split`] gives the layer's address and the
//! `where` decoded, encoded twice as it often is (`%253D` → `%3D` → `=`).

/// The decoding passes a `where` gets: catalogues encode it once or twice, and a third pass
/// would turn a `LIKE 'a%20b'` pattern into something else.
const DECODE_PASSES: usize = 2;

/// An ArcGIS address's layer (no `/query`, no query string) and its `where`, when it has one
/// that selects something (`1=1` selects everything, so it's no filter).
pub fn split(url: &str) -> (String, Option<String>) {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let path = path.trim_end_matches('/');
    let base = path.strip_suffix("/query").unwrap_or(path).to_string();
    let filter = query
        .split('&')
        .find_map(|pair| pair.split_once('=').filter(|(k, _)| k.eq_ignore_ascii_case("where")).map(|(_, v)| v))
        .map(|w| (0..DECODE_PASSES).fold(w.to_string(), |w, _| decode(&w)))
        .map(|w| w.trim().to_string())
        .filter(|w| !w.is_empty() && w != "1=1");
    (base, filter)
}

/// One pass of URL decoding: `%XX` sequences and `+` as a space. A `%` that starts no valid
/// sequence stays, and so do bytes that don't make UTF-8.
pub fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        let hex = |at: usize| bytes.get(at).and_then(|c| (*c as char).to_digit(16));
        match (b, hex(i + 1), hex(i + 2)) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            (b'+', ..) => {
                out.push(b' ');
                i += 1;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::{decode, split};

    #[test]
    fn a_where_encoded_twice() {
        let url = "https://hermes2.invias.gov.co/server/rest/services/ValorizacionPredial/ValorizacionPredialV2/MapServer/1/query?where=municipio_1%253D'BARRANQUILLA'";
        assert_eq!(
            split(url),
            (
                "https://hermes2.invias.gov.co/server/rest/services/ValorizacionPredial/ValorizacionPredialV2/MapServer/1".to_string(),
                Some("municipio_1='BARRANQUILLA'".to_string())
            )
        );
    }

    #[test]
    fn a_where_among_other_parameters() {
        let url = "https://geo.upme.gov.co/server/rest/services/Capas_Hidrocarburos/nodos_oferta_demanda/FeatureServer/2/query?where=municipio%3D%27Barranquilla%27&fullText=&outFields=&f=html";
        let (base, filter) = split(url);
        assert_eq!(base, "https://geo.upme.gov.co/server/rest/services/Capas_Hidrocarburos/nodos_oferta_demanda/FeatureServer/2");
        assert_eq!(filter.as_deref(), Some("municipio='Barranquilla'"));
    }

    #[test]
    fn a_like_pattern_keeps_its_percent() {
        let url = "https://services6.arcgis.com/x/arcgis/rest/services/PuentesODAGOL/FeatureServer/0/query?where=carretera+LIKE+%27Barranquilla%25%27&";
        assert_eq!(split(url).1.as_deref(), Some("carretera LIKE 'Barranquilla%'"));
        let url =
            "https://x/rest/services/a/MapServer/4/query?where=MUNICIPIOS+IN+('BARRANQUILLA'%252C+'BARRANQUILLA%252C+PUERTO+COLOMBIA')";
        assert_eq!(split(url).1.as_deref(), Some("MUNICIPIOS IN ('BARRANQUILLA', 'BARRANQUILLA, PUERTO COLOMBIA')"));
    }

    #[test]
    fn no_filter() {
        let url = "https://serviciosgis.catastrobogota.gov.co/arcgis/rest/services/catastro/lote/MapServer/0";
        assert_eq!(split(url), (url.to_string(), None));
        assert_eq!(split("https://x/MapServer/0/query?where=1%3D1").1, None);
        assert_eq!(split("https://x/MapServer/0/query?where=&f=json").1, None);
        assert_eq!(split("https://x/MapServer/0/"), ("https://x/MapServer/0".to_string(), None));
    }

    #[test]
    fn hostile_text_decodes_without_panicking() {
        assert_eq!(decode("%"), "%");
        assert_eq!(decode("%4"), "%4");
        assert_eq!(decode("%zz%41"), "%zzA");
        assert_eq!(decode("%FF"), "%FF");
        assert_eq!(decode(""), "");
        assert_eq!(split(""), (String::new(), None));
        assert_eq!(split("?where=%25%25%25"), (String::new(), Some("%%%".to_string())));
    }
}
