//! Fuzzy search utility — ported from src/utils/fuzzySearch.ts
//!
//! Character-scan fuzzy matcher that checks whether all query characters
//! appear in order within each candidate string.

/// Perform a fuzzy search over a list of items.
///
/// Returns matching items sorted by relevance: items starting with the
/// query come first, then sorted by ascending length.
pub fn fuzzy_search(query: &str, items: &[String]) -> Vec<String> {
    if query.is_empty() {
        return items.to_vec();
    }

    let query_lower = query.to_lowercase();
    let mut matches: Vec<String> = items
        .iter()
        .filter(|item| {
            let item_lower = item.to_lowercase();
            let mut qi = query_lower.chars().peekable();
            for c in item_lower.chars() {
                if c == *qi.peek().unwrap_or(&'\0') {
                    qi.next();
                }
                if qi.peek().is_none() {
                    return true;
                }
            }
            qi.peek().is_none()
        })
        .cloned()
        .collect();

    let q_lower = query.to_lowercase();
    matches.sort_by(|a, b| {
        let a_starts = a.to_lowercase().starts_with(&q_lower);
        let b_starts = b.to_lowercase().starts_with(&q_lower);
        match (a_starts, b_starts) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.len().cmp(&b.len()),
        }
    });

    matches
}
