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

    // Lowercase the query and each item exactly once, then sort using the
    // precomputed lowercase form. This avoids the O(n^2) lowercase churn
    // that the previous implementation incurred inside the sort closure
    // (where `a.to_lowercase()` and `b.to_lowercase()` ran on every
    // comparison).
    let query_lower = query.to_lowercase();
    let mut scored: Vec<(String, String)> = items
        .iter()
        .filter_map(|item| {
            let item_lower = item.to_lowercase();
            let mut qi = query_lower.chars().peekable();
            for c in item_lower.chars() {
                if c == *qi.peek().unwrap_or(&'\0') {
                    qi.next();
                }
                if qi.peek().is_none() {
                    return Some((item_lower, item.clone()));
                }
            }
            if qi.peek().is_none() {
                Some((item_lower, item.clone()))
            } else {
                None
            }
        })
        .collect();

    // Sort by (prefix-match, original-length). `query_lower` and the
    // `item_lower` values in `scored` are already computed, so the
    // comparator is allocation-free.
    scored.sort_by(|a, b| {
        let a_starts = a.0.starts_with(&query_lower);
        let b_starts = b.0.starts_with(&query_lower);
        match (a_starts, b_starts) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            // Tiebreak by original (non-lowercased) length to preserve
            // pre-refactor semantics — `to_lowercase` can change byte
            // length for non-ASCII input.
            _ => a.1.len().cmp(&b.1.len()),
        }
    });

    scored.into_iter().map(|(_, original)| original).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_all_items() {
        let items = vec!["b".to_string(), "a".to_string()];
        let result = fuzzy_search("", &items);
        assert_eq!(result, items);
    }

    #[test]
    fn filters_non_matches() {
        let items = vec![
            "apple".to_string(),
            "banana".to_string(),
            "cherry".to_string(),
        ];
        let result = fuzzy_search("an", &items);
        assert_eq!(result, vec!["banana".to_string()]);
    }

    #[test]
    fn case_insensitive_match() {
        let items = vec!["Apple".to_string(), "BANANA".to_string()];
        let result = fuzzy_search("an", &items);
        assert_eq!(result, vec!["BANANA".to_string()]);
    }

    #[test]
    fn prefix_matches_rank_first_then_by_length() {
        let items = vec![
            "application".to_string(),
            "app".to_string(),
            "appendix".to_string(),
            "zapp".to_string(),
        ];
        let result = fuzzy_search("app", &items);
        assert_eq!(
            result,
            vec![
                "app".to_string(),
                "appendix".to_string(),
                "application".to_string(),
                "zapp".to_string(),
            ]
        );
    }

    #[test]
    fn no_match_returns_empty() {
        let items = vec!["xyz".to_string(), "qqq".to_string()];
        let result = fuzzy_search("ab", &items);
        assert!(result.is_empty());
    }

    #[test]
    fn ranking_correct_for_thousand_items() {
        // 1000-item regression test: ensures the O(n) refactor preserves
        // exact ranking semantics. Prefix matches must come first, sorted
        // by ascending length; non-prefix matches after.
        let mut items: Vec<String> = (0..1000).map(|i| format!("item_{i}")).collect();
        // Inject prefix matches and substring-only matches.
        items[2] = "abacus".to_string();         // prefix, len 6
        items[4] = "absolute".to_string();       // prefix, len 8
        items[7] = "abstract".to_string();       // prefix, len 8
        items[100] = "z_ab_extra".to_string();   // substring only, len 9
        items[101] = "z_ab".to_string();         // substring only, len 4
        items[500] = "xabx".to_string();         // substring only, len 4
        items[501] = "xaby".to_string();         // substring only, len 4

        let result = fuzzy_search("ab", &items);

        // Split into prefix and non-prefix blocks.
        let prefix_end = result
            .iter()
            .position(|s| !s.to_lowercase().starts_with("ab"))
            .unwrap_or(result.len());
        let (prefix, after) = result.split_at(prefix_end);

        // Prefix block must be sorted by ascending length.
        let mut prefix_sorted: Vec<&String> = prefix.iter().collect();
        prefix_sorted.sort_by_key(|s| s.len());
        assert_eq!(
            prefix,
            prefix_sorted.as_slice(),
            "prefix block must be length-sorted",
        );

        // Non-prefix block: no item starts with "ab" (case-insensitive).
        for s in after {
            assert!(
                !s.to_lowercase().starts_with("ab"),
                "non-prefix item {s:?} appeared before all prefix matches",
            );
        }

        // Sanity: expected prefix count is 3 (abacus, absolute, abstract).
        assert_eq!(prefix.len(), 3, "expected 3 prefix matches");
    }
}
