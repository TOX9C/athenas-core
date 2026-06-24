//! Random human-name generator for idle Shell panes.
//!
//! A Shell pane with no agent running shows a short, friendly first name
//! (like the reference app's "Theo" / "Cole") instead of a bare "Shell".
//! The name is derived **deterministically** from the pane id so it stays
//! stable across re-renders and app restarts — no flicker, no persisted state.

/// Curated wordlist of short, distinct first names. Kept lowercase-cased at
/// the source and title-cased on output. ~50 entries balances variety against
/// collision probability for typical (≤16) pane counts.
const FIRST_NAMES: &[&str] = &[
    "Theo", "Cole", "Maya", "Leo", "Zara", "Finn", "Iris", "Kai", "Nova", "Ezra", "Luna", "Jude",
    "Sage", "Rowan", "Wren", "Otto", "Enzo", "Cleo", "Juno", "Reza", "Milo", "Nori", "Asa", "Dax",
    "Eve", "Fern", "Gus", "Hana", "Ivo", "Jett", "Knox", "Lyra", "Mira", "Nico", "Onyx", "Pax",
    "Quinn", "Remy", "Soren", "Tova", "Uma", "Vida", "Wells", "Xan", "Yael", "Zane", "Arlo",
    "Bram", "Coda", "Dune",
];

/// Deterministically map a pane id to a stable name.
///
/// Hashes the id with a simple FNV-1a-style fold (no extra dependency) and
/// reduces modulo the wordlist length. Same id → same name, every time, so a
/// pane keeps its identity across re-renders and restarts. Returns a title-cased
/// name from the list; falls back to the first entry for empty input.
pub fn name_for_pane(pane_id: &str) -> String {
    let idx = if FIRST_NAMES.is_empty() || pane_id.is_empty() {
        0
    } else {
        // FNV-1a 32-bit fold over the id bytes. Cheap, dependency-free, and
        // well-distributed enough for picking a name from a ~50-entry list.
        let mut hash: u32 = 0x811c9dc5;
        for &b in pane_id.as_bytes() {
            hash ^= b as u32;
            hash = hash.wrapping_mul(0x0100_0193);
        }
        (hash as usize) % FIRST_NAMES.len()
    };
    FIRST_NAMES[idx].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_id_yields_same_name() {
        let a = name_for_pane("abc-123-pane");
        let b = name_for_pane("abc-123-pane");
        assert_eq!(a, b, "same id must map to the same name");
    }

    #[test]
    fn different_ids_usually_yield_different_names() {
        // With ~50 names and distinct ids, we expect spread. Not a hard
        // guarantee (collisions are valid), so just assert a reasonable number
        // of unique names out of a batch.
        let names: std::collections::HashSet<String> = (0..40)
            .map(|i| name_for_pane(&format!("pane-{i}")))
            .collect();
        assert!(
            names.len() >= 10,
            "expected >=10 distinct names across 40 ids, got {}: {names:?}",
            names.len()
        );
    }

    #[test]
    fn empty_id_does_not_panic() {
        let n = name_for_pane("");
        assert!(!n.is_empty());
    }

    #[test]
    fn all_entries_are_from_wordlist() {
        for i in 0..64 {
            let n = name_for_pane(&format!("id-{i}"));
            assert!(FIRST_NAMES.contains(&n.as_str()), "{n:?} not in wordlist");
        }
    }
}
