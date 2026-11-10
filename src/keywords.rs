//! Search keywords for patients.
//!
//! The original app split names on spaces only, so `Ma'ruf` or `Sumarni,`
//! were indexed with their punctuation and could only be found by typing it
//! exactly. Here apostrophes are dropped (`ma'ruf` -> `maruf`) and every other
//! non-alphanumeric character separates words. The same normalisation is
//! applied to queries, so whatever the user types matches the stored form.

use std::collections::BTreeSet;

/// Maximum number of distinct words accepted in one search, as before.
pub const MAX_QUERY_KEYWORDS: usize = 30;

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '\u{2018}' | '`' | '\u{02BC}')
}

/// Lower-cased, de-duplicated, sorted words of `text`.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut words = BTreeSet::new();
    let mut current = String::new();
    for c in text.chars() {
        if is_apostrophe(c) {
            continue;
        }
        if c.is_alphanumeric() {
            current.extend(c.to_lowercase());
        } else if !current.is_empty() {
            words.insert(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.insert(current);
    }
    words.into_iter().collect()
}

/// Keywords indexed for a patient: the words of the name and the head of
/// family name.
pub fn patient_keywords(name: &str, head_family_name: &str) -> Vec<String> {
    tokenize(&format!("{name} {head_family_name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_lowercases() {
        assert_eq!(tokenize("  Siti   NUR Aisyah "), ["aisyah", "nur", "siti"]);
    }

    #[test]
    fn handles_punctuation() {
        assert_eq!(tokenize("Ma'ruf"), ["maruf"]);
        assert_eq!(tokenize("Ma\u{2019}ruf"), ["maruf"]);
        assert_eq!(tokenize("Sumarni, S.Pd"), ["pd", "s", "sumarni"]);
        assert_eq!(
            tokenize("Bayi Ny. (Rina)/Joko"),
            ["bayi", "joko", "ny", "rina"]
        );
        assert!(tokenize("( ). /").is_empty());
    }

    #[test]
    fn deduplicates_name_and_family_name() {
        assert_eq!(
            patient_keywords("Joko Santoso", "Santoso"),
            ["joko", "santoso"]
        );
    }

    #[test]
    fn keeps_non_ascii_letters() {
        assert_eq!(tokenize("Éka ÇAHYA"), ["çahya", "éka"]);
    }
}
