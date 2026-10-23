//! "Did you mean" suggestions for names a model got wrong.

/// Levenshtein distance between `a` and `b`, in characters.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == *cb {
                diagonal
            } else {
                1 + diagonal.min(above).min(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

/// Up to `limit` of `candidates` closest to `wanted`: names containing it
/// first, then by edit distance. Candidates further than half their length
/// away are dropped.
pub fn suggest<'a>(
    wanted: &str,
    candidates: impl IntoIterator<Item = &'a str>,
    limit: usize,
) -> Vec<&'a str> {
    let wanted = wanted.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    let mut scored: Vec<(usize, &str)> = candidates
        .into_iter()
        .filter_map(|candidate| {
            let distance = if candidate.contains(wanted.as_str()) {
                0
            } else {
                edit_distance(&wanted, candidate)
            };
            (distance <= candidate.len().max(wanted.len()) / 2).then_some((distance, candidate))
        })
        .collect();
    scored.sort();
    scored
        .into_iter()
        .take(limit)
        .map(|(_, name)| name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_counts_edits() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("same", "same"), 0);
    }

    #[test]
    fn suggestions_prefer_close_names() {
        let names = [
            "filter_1_cutoff",
            "filter_2_cutoff",
            "filter_1_resonance",
            "volume",
        ];
        assert_eq!(suggest("filter1_cutoff", names, 1), ["filter_1_cutoff"]);
        assert_eq!(
            suggest("cutoff", names, 3),
            ["filter_1_cutoff", "filter_2_cutoff"]
        );
        assert!(suggest("zzzzzzzz", names, 3).is_empty());
    }
}
