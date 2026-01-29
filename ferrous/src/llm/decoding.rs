use trie_rs::{Trie, TrieBuilder};
use std::sync::LazyLock;

fn reverse(s: &str) -> String {
    s.chars().rev().collect()
}

pub struct StopCondition {
    stop_trie: Option<Trie<u8>>,
    reversed_text: String,
}

impl std::fmt::Debug for StopCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StopCondition")
            .field("has_trie", &self.stop_trie.is_some())
            .field("reversed_text", &self.reversed_text)
            .finish()
    }
}

impl StopCondition {
    #[must_use]
    pub fn new(stop_words: Vec<String>) -> Self {
        let stop_trie = if stop_words.is_empty() {
            None
        } else {
            let mut builder = TrieBuilder::new();
            for word in stop_words {
                builder.push(reverse(&word));
            }
            Some(builder.build())
        };

        Self {
            stop_trie,
            reversed_text: String::new(),
        }
    }

    pub fn should_stop(&mut self, new_text: &str) -> (bool, usize) {
        if new_text.is_empty() {
            return (false, 0);
        }

        self.reversed_text = reverse(new_text) + &self.reversed_text;

        if let Some(trie) = &self.stop_trie {
            let matches = trie.common_prefix_search(&self.reversed_text);
            let matched_length = matches.into_iter().map(|x: Vec<u8>| x.len()).max();
            if let Some(len) = matched_length {
                return (true, len);
            }
        }

        (false, 0)
    }
}

pub static DEFAULT_STOP_WORDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    vec![
        "\n\n".to_string(),
        "\n\n  ".to_string(),
        "\n\n    ".to_string(),
        "<|file_sep|>".to_string(),
        "<fim_prefix>".to_string(),
        "<fim_suffix>".to_string(),
        "<fim_middle>".to_string(),
        "<|endoftext|>".to_string(),
        "<|im_start|>".to_string(),
        "<|im_end|>".to_string(),
    ]
});

pub fn get_stop_words_for_language(lang: &str) -> Vec<String> {
    let mut words = DEFAULT_STOP_WORDS.clone();
    
    // Add some basic language-specific stop words
    match lang.to_lowercase().as_str() {
        "rust" => {
            words.push("\nfn ".to_string());
            words.push("\npub fn ".to_string());
            words.push("\nimpl ".to_string());
            words.push("\nstruct ".to_string());
            words.push("\nenum ".to_string());
            words.push("\ntrait ".to_string());
        }
        "python" => {
            words.push("\ndef ".to_string());
            words.push("\nclass ".to_string());
            words.push("\nif __name__".to_string());
        }
        "cpp" | "c++" | "c" => {
            words.push("\nvoid ".to_string());
            words.push("\nint ".to_string());
            words.push("\nclass ".to_string());
            words.push("\nstruct ".to_string());
            words.push("\nnamespace ".to_string());
        }
        "javascript" | "typescript" | "js" | "ts" => {
            words.push("\nfunction ".to_string());
            words.push("\nclass ".to_string());
            words.push("\nexport ".to_string());
            words.push("\nconst ".to_string());
        }
        _ => {}
    }
    
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_condition_simple() {
        let mut sc = StopCondition::new(vec!["STOP".to_string()]);
        assert!(!sc.should_stop("HELLO").0);
        assert!(sc.should_stop("STOP").0);
    }

    #[test]
    fn test_stop_condition_prefix() {
        let mut sc = StopCondition::new(vec!["\nvoid".to_string()]);
        assert!(!sc.should_stop("int main() {").0);
        // "\nvoid" reversed is "diov\n"
        // If we pass "\n", then "void", the reversed text becomes "diov\n"
        assert!(!sc.should_stop("\n").0);
        assert!(sc.should_stop("void").0);
    }

    #[test]
    fn test_stop_condition_incremental() {
        let mut sc = StopCondition::new(vec!["\n\n".to_string()]);
        assert!(!sc.should_stop("first line").0);
        assert!(!sc.should_stop("\n").0);
        assert!(sc.should_stop("\n").0);
    }
}
