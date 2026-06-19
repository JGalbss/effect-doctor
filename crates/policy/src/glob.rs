//! Minimal path-glob matching for policy patterns. Supports:
//!   - `**`  — any number of path segments (including zero),
//!   - `*`   — any run of characters within a single segment (never `/`),
//!   - literals otherwise.
//!
//! Deliberately small (no external glob/regex dep): policy patterns are simple
//! path shapes like `src/core/**` and `**/*.gen.ts`.

/// Does `path` match `pattern`? Both are forward-slash paths.
pub fn matches(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let txt: Vec<&str> = path.split('/').collect();
    match_segments(&pat, &txt)
}

fn match_segments(pat: &[&str], txt: &[&str]) -> bool {
    match pat.first() {
        None => txt.is_empty(),
        Some(&"**") => (0..=txt.len()).any(|skip| match_segments(&pat[1..], &txt[skip..])),
        Some(segment) => match txt.first() {
            Some(first) if segment_matches(segment, first) => {
                match_segments(&pat[1..], &txt[1..])
            }
            _ => false,
        },
    }
}

/// Wildcard match within one segment (`*` only), via the classic greedy
/// two-pointer algorithm with backtracking.
fn segment_matches(pattern: &str, text: &str) -> bool {
    let pat = pattern.as_bytes();
    let txt = text.as_bytes();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < txt.len() {
        if pi < pat.len() && pat[pi] == b'*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < pat.len() && pat[pi] == txt[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(star_pos) = star {
            pi = star_pos + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == b'*' {
        pi += 1;
    }
    pi == pat.len()
}

/// Does `path` match any pattern in `patterns`?
pub fn matches_any(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|pattern| matches(pattern, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_star_spans_segments() {
        assert!(matches("src/core/**", "src/core/a/b.ts"));
        assert!(matches("src/core/**", "src/core/a.ts"));
        assert!(matches("src/core/**", "src/core")); // zero segments
        assert!(!matches("src/core/**", "src/ui/a.ts"));
    }

    #[test]
    fn leading_double_star_and_suffix() {
        assert!(matches("**/*.gen.ts", "a/b/c.gen.ts"));
        assert!(matches("**/*.gen.ts", "c.gen.ts"));
        assert!(!matches("**/*.gen.ts", "c.ts"));
    }

    #[test]
    fn single_star_stays_within_segment() {
        assert!(matches("src/*.ts", "src/a.ts"));
        assert!(!matches("src/*.ts", "src/sub/a.ts"));
    }

    #[test]
    fn literal_exact() {
        assert!(matches("src/db/migrations", "src/db/migrations"));
        assert!(!matches("src/db/migrations", "src/db"));
    }

    #[test]
    fn matches_any_helper() {
        let patterns = vec!["**/*.gen.ts".to_string(), "src/db/**".to_string()];
        assert!(matches_any(&patterns, "src/db/x.ts"));
        assert!(matches_any(&patterns, "a/b.gen.ts"));
        assert!(!matches_any(&patterns, "src/app/main.ts"));
    }
}
