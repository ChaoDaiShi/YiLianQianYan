use std::collections::HashSet;

use crate::utils::text::is_cjk;

pub(in crate::db) struct LexicalQuery {
    text: String,
    words: HashSet<String>,
    pairs: HashSet<(char, char)>,
}

impl LexicalQuery {
    pub(in crate::db) fn new(query: &str) -> Self {
        let text = query.trim().to_lowercase();
        Self {
            words: words(&text).map(str::to_owned).collect(),
            pairs: cjk_pairs(&text).collect(),
            text,
        }
    }

    pub(in crate::db) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub(in crate::db) fn score(&self, content: &str) -> f64 {
        if self.is_empty() {
            return 0.0;
        }
        let content = content.to_lowercase();
        let leading_word = self.text.chars().next().is_some_and(word_character);
        let trailing_word = self.text.chars().next_back().is_some_and(word_character);
        if content
            .match_indices(self.text.as_str())
            .any(|(start, matched)| {
                let before = content[..start].chars().next_back();
                let after = content[start + matched.len()..].chars().next();
                (!leading_word || !before.is_some_and(word_character))
                    && (!trailing_word || !after.is_some_and(word_character))
            })
        {
            return 0.9;
        }

        let words_found = words(&content)
            .filter(|word| self.words.contains(*word))
            .collect::<HashSet<_>>()
            .len();
        let pairs_found = cjk_pairs(&content)
            .filter(|pair| self.pairs.contains(pair))
            .collect::<HashSet<_>>()
            .len();
        let word_score = 0.72 * words_found as f64 / self.words.len().max(1) as f64;
        let phrase_score = 0.74 * pairs_found as f64 / self.pairs.len().max(1) as f64;
        word_score.max(phrase_score)
    }
}

fn word_character(character: char) -> bool {
    (character.is_alphanumeric() && !is_cjk(character)) || matches!(character, '_' | '-')
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character| !word_character(character))
        .map(|word| word.trim_matches(['_', '-']))
        .filter(|word| word.chars().count() >= 2)
}

fn cjk_pairs(text: &str) -> impl Iterator<Item = (char, char)> + '_ {
    text.chars()
        .zip(text.chars().skip(1))
        .filter(|&(first, second)| is_cjk(first) && is_cjk(second))
}
