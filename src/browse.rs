//! A catalogue as the panel walks it: categories, the subcategories of one, then the services
//! of one, each group with the number of services it holds.

use crate::entry::Entry;

/// A group of the walk: its name and how many services it holds.
pub type Group = (String, usize);

/// The catalogue's categories, sorted; `other` names the services without one.
pub fn categories(entries: &[Entry], other: &str) -> Vec<Group> {
    count(entries.iter().map(|e| e.category.as_deref().unwrap_or(other)))
}

/// The subcategories of `category`, sorted; `other` names its services without one.
pub fn subcategories(entries: &[Entry], category: &str, other: &str) -> Vec<Group> {
    count(entries.iter().filter(|e| e.category.as_deref().unwrap_or(other) == category).map(|e| e.subcategory.as_deref().unwrap_or(other)))
}

/// The services of `category` and `subcategory`, by name.
pub fn services<'a>(entries: &'a [Entry], category: &str, subcategory: &str, other: &str) -> Vec<&'a Entry> {
    let mut v: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.category.as_deref().unwrap_or(other) == category && e.subcategory.as_deref().unwrap_or(other) == subcategory)
        .collect();
    v.sort_by_key(|e| sort_key(&e.name));
    v
}

/// A group's slider label: its name, clipped to `max` characters with its count.
pub fn label((name, n): &Group, max: usize) -> String {
    let count = format!(" ({n})");
    format!("{}{count}", clip(name, max.saturating_sub(count.chars().count())))
}

/// The names, sorted as people read them, each with how often it came.
fn count<'a>(names: impl Iterator<Item = &'a str>) -> Vec<Group> {
    let mut names: Vec<&str> = names.collect();
    names.sort_by_key(|s| sort_key(s));
    let mut groups: Vec<Group> = Vec::new();
    for name in names {
        match groups.last_mut() {
            Some((last, n)) if last == name => *n += 1,
            _ => groups.push((name.to_string(), 1)),
        }
    }
    groups
}

/// The position `at` asks for in a list of `len`, kept within it (a slider's value can outlive
/// the list it was set for).
pub fn clamp(at: Option<i64>, len: usize) -> usize {
    let last = len.saturating_sub(1);
    at.and_then(|a| usize::try_from(a).ok()).unwrap_or(0).min(last)
}

/// Text clipped to `max` characters, with an ellipsis when cut (slider labels are 80 at most).
pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Spanish names sorted as people read them: case and accents aside (`Árboles` with the A's).
fn sort_key(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ë' => 'e',
            'í' | 'ì' | 'ï' => 'i',
            'ó' | 'ò' | 'ö' => 'o',
            'ú' | 'ù' | 'ü' => 'u',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::Raw;

    fn entry(id: &str, name: &str, category: Option<&str>, subcategory: Option<&str>) -> Entry {
        let url = format!("https://x/rest/services/a/MapServer/{id}");
        Entry::new(&Raw { id: Some(id), name: Some(name), url: Some(&url), category, subcategory, ..Raw::default() })
            .expect("a valid entry")
    }

    #[test]
    fn categories_subcategories_and_services() {
        let all = [
            entry("1", "Lote", Some("Catastro"), Some("Predios")),
            entry("2", "Árboles", Some("Ambiente"), Some("Arborización")),
            entry("3", "Manzana", Some("Catastro"), Some("Predios")),
            entry("4", "Sin tema", None, None),
            entry("5", "agua", Some("Ambiente"), Some("Arborización")),
            entry("6", "Ruido", Some("Ambiente"), None),
        ];
        let group = |n: &str, c: usize| (n.to_string(), c);
        assert_eq!(categories(&all, "Otros"), [group("Ambiente", 3), group("Catastro", 2), group("Otros", 1)]);
        assert_eq!(subcategories(&all, "Ambiente", "Otros"), [group("Arborización", 2), group("Otros", 1)]);
        assert_eq!(subcategories(&all, "Otros", "Otros"), [group("Otros", 1)]);
        assert!(subcategories(&all, "Nada", "Otros").is_empty());
        let names = |c: &str, s: &str| services(&all, c, s, "Otros").iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(names("Ambiente", "Arborización"), ["agua", "Árboles"]);
        assert_eq!(names("Ambiente", "Otros"), ["Ruido"]);
        assert_eq!(names("Catastro", "Predios"), ["Lote", "Manzana"]);
        assert!(names("Catastro", "Otros").is_empty());
        assert!(categories(&[], "Otros").is_empty());
    }

    #[test]
    fn labels_keep_their_count() {
        assert_eq!(label(&("Ambiente y Desarrollo Sostenible".to_string(), 66), 80), "Ambiente y Desarrollo Sostenible (66)");
        assert_eq!(label(&("Ambiente y Desarrollo Sostenible".to_string(), 66), 15), "Ambiente … (66)");
        assert_eq!(label(&(String::new(), 0), 2), " (0)");
    }

    #[test]
    fn positions_stay_in_the_list() {
        assert_eq!(clamp(Some(3), 10), 3);
        assert_eq!(clamp(Some(30), 10), 9);
        assert_eq!(clamp(Some(-1), 10), 0);
        assert_eq!(clamp(None, 10), 0);
        assert_eq!(clamp(Some(5), 0), 0);
    }

    #[test]
    fn clipping() {
        assert_eq!(clip("Lote", 80), "Lote");
        assert_eq!(clip("Ortoimagen del Distrito", 10), "Ortoimage…");
        assert_eq!(clip("ñññ", 2), "ñ…");
        assert_eq!(clip("", 0), "");
    }
}
