//! Lightweight lexical matching for hybrid search.
//!
//! Embeddings are good at meaning but smear rare exact tokens (names, versions, hardware models:
//! `RTX 4070`, `Neovim`, `osy-kernel`). This module finds such tokens without any index or stemmer:
//!
//! * text is lowercased, `ё` is folded to `е`, split into alphanumeric words;
//! * words match exactly, or by a common prefix (`кота` ~ `кот`, `использую` ~ `использует`),
//!   which covers Russian and English inflections; words with digits match exactly;
//! * only *specific* query words count: a word found in most documents (`пользователь`) carries no signal;
//! * the score is the idf-weighted share of matched specific query words: `(0, 1]`.
//!
//! The scan is stateless (no index to keep in sync) and linear in the number of documents.

use std::collections::HashSet;

/// Function words that never make a useful lexical match (Russian and English).
const STOP: &[&str] = &[
    // english
    "the", "and", "for", "are", "was", "were", "not", "but", "you", "your", "what", "how", "who", "why", "when", "with",
    "from", "that", "this", "have", "has", "had", "can", "will", "about", "into", "than", "then", "there", "their",
    // russian
    "как", "что", "где", "это", "для", "при", "или", "какой", "какая", "какие", "какое", "каких", "кто", "чей",
    "чем", "над", "под", "про", "без", "все", "был", "была", "были", "быть", "есть", "его", "ее", "их", "мне",
    "мой", "моя", "мои", "мою", "мое", "меня", "тебя", "нас", "вас", "они", "она", "оно", "мы", "вы", "ты",
    "так", "там", "тут", "уже", "еще", "ли", "же", "бы", "когда", "почему", "зачем", "можно", "нужно",
];

/// A query word must be found in at most this share of documents to be "specific".
const MAX_DOC_SHARE: f32 = 0.4;

/// Minimal prefix length (chars) for an inflection match.
const MIN_PREFIX: usize = 3;

/// Minimal share of the longer word covered by the common prefix.
const MIN_PREFIX_RATIO: f32 = 0.6;

/// Lowercases and folds `ё` to `е`.
fn fold(text: &str) -> String {
    text.to_lowercase().replace('ё', "е")
}

/// Splits text into folded alphanumeric words.
pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();

    for ch in fold(text).chars() {
        if ch.is_alphanumeric() {
            cur.push(ch);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }

    out
}

fn has_digit(word: &str) -> bool {
    word.chars().any(|c| c.is_numeric())
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// Returns true if a query word matches a document word (both already folded).
pub fn word_match(q: &str, d: &str) -> bool {
    if q == d {
        return true;
    }
    // numbers and versions must match exactly
    if has_digit(q) || has_digit(d) {
        return false;
    }

    let (lq, ld) = (q.chars().count(), d.chars().count());
    if lq < MIN_PREFIX || ld < MIN_PREFIX {
        return false;
    }

    let prefix = common_prefix(q, d);
    prefix >= MIN_PREFIX && prefix as f32 / lq.max(ld) as f32 >= MIN_PREFIX_RATIO
}

/// Unique meaningful words of the query (stop words and 1-2 letter words are dropped, digits are kept).
pub fn query_terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();

    words(query)
        .into_iter()
        .filter(|w| w.chars().count() >= 3 || has_digit(w))
        .filter(|w| !STOP.contains(&w.as_str()))
        .filter(|w| seen.insert(w.clone()))
        .collect()
}

/// Scores pre-tokenized documents (see [`words`]) against the query.
///
/// Returns `(document index, score)` only for documents with a match; the score is in `(0, 1]`.
pub fn score(query: &str, docs: &[Vec<String>]) -> Vec<(usize, f32)> {
    let terms = query_terms(query);
    if terms.is_empty() || docs.is_empty() {
        return vec![];
    }

    let n = docs.len();
    let mut hits = vec![vec![false; terms.len()]; n];
    let mut df = vec![0usize; terms.len()];

    for (i, doc) in docs.iter().enumerate() {
        for (j, term) in terms.iter().enumerate() {
            if doc.iter().any(|w| word_match(term, w)) {
                hits[i][j] = true;
                df[j] += 1;
            }
        }
    }

    // only words that exist in the corpus but are not everywhere carry signal
    let max_df = ((n as f32 * MAX_DOC_SHARE) as usize).max(1);
    let idf: Vec<f32> = df
        .iter()
        .map(|&d| {
            if d >= 1 && d <= max_df {
                (1.0 + (n as f32 - d as f32 + 0.5) / (d as f32 + 0.5)).ln()
            } else {
                0.0
            }
        })
        .collect();

    let total: f32 = idf.iter().sum();
    if total <= 0.0 {
        return vec![];
    }

    let mut out = Vec::new();
    for (i, row) in hits.iter().enumerate() {
        let s: f32 = row
            .iter()
            .zip(&idf)
            .filter(|(hit, _)| **hit)
            .map(|(_, w)| *w)
            .sum();
        if s > 0.0 {
            out.push((i, (s / total).min(1.0)));
        }
    }

    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus() -> Vec<Vec<String>> {
        [
            "Пользователь использует Neovim как основной редактор",
            "Пользователь живёт в Хельсинки",
            "У пользователя кот по кличке Том",
            "Пользователь пишет на Rust, проект называется osy",
            "Видеокарта пользователя RTX 4070",
            "Пользователь предпочитает тёмную тему",
            "User's favorite coffee is flat white",
            "Пользователь любит кофе и пьёт его без сахара",
            "Сын пользователя ходит в школу",
            "Пользователь работает над проектом osy-kernel на Rust",
        ]
        .iter()
        .map(|t| words(t))
        .collect()
    }

    fn top(query: &str) -> Vec<usize> {
        let mut ids: Vec<usize> = score(query, &corpus()).into_iter().map(|(i, _)| i).collect();
        ids.sort();
        ids
    }

    #[test]
    fn folds_and_splits() {
        assert_eq!(words("Живёт в Хельсинки!"), vec!["живет", "в", "хельсинки"]);
    }

    #[test]
    fn matches_inflections_but_not_different_words() {
        assert!(word_match("кота", "кот"));
        assert!(word_match("использую", "использует"));
        assert!(word_match("живу", "живет"));
        assert!(!word_match("пользователь", "пользуется"));
        assert!(!word_match("4070", "4071"));
    }

    #[test]
    fn finds_rare_exact_tokens() {
        assert_eq!(top("какой редактор я использую?"), vec![0]);
        assert_eq!(top("как зовут моего кота"), vec![2]);
        assert_eq!(top("какая у меня видеокарта RTX"), vec![4]);
        assert_eq!(top("RTX 4070 драйвера"), vec![4]);
        assert_eq!(top("где я живу"), vec![1]);
        assert_eq!(top("что я пью кофе"), vec![7]);
        assert_eq!(top("какой у меня проект на rust"), vec![3, 9]);
    }

    #[test]
    fn irrelevant_queries_match_nothing() {
        assert!(top("погода завтра").is_empty());
        assert!(top("посоветуй фильм на вечер").is_empty());
    }

    #[test]
    fn score_is_normalized() {
        for (_, s) in score("какая у меня видеокарта RTX", &corpus()) {
            assert!(s > 0.0 && s <= 1.0);
        }
    }
}
