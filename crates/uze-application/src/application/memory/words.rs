//! Words as a person spells them: the one comparison finding a fact and
//! recognising a near duplicate share.

/// `text` as it is compared: lowercase, Latin letters without their
/// diacritics, and no `-` or `_`, so `Decisão`, `decisao` and `pt-br`
/// match `decisão` and `ptbr` however they were typed.
pub(super) fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for letter in text.chars().flat_map(char::to_lowercase) {
        match letter {
            '-' | '_' => {}
            'ß' => folded.push_str("ss"),
            'æ' => folded.push_str("ae"),
            'œ' => folded.push_str("oe"),
            letter => folded.push(base(letter)),
        }
    }
    folded
}

/// The Latin-1 and Latin Extended-A letter `letter` is written on.
fn base(letter: char) -> char {
    match letter {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' | 'ŧ' => 't',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        letter => letter,
    }
}

/// The words of `text` that say what it is about: folded, split on
/// anything that is not a letter or digit, without those under three
/// letters or the commonest English and Portuguese ones.
pub(super) fn telling(text: &str) -> Vec<String> {
    let mut words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .map(fold)
        .filter(|word| word.chars().count() >= 3 && !COMMON.contains(&word.as_str()))
        .collect();
    words.sort_unstable();
    words.dedup();
    words
}

/// Words that appear in nearly every fact, so say nothing about any.
const COMMON: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "are", "was", "not", "its", "into", "but",
    "has", "have", "than", "then", "when", "what", "which", "who", "how", "all", "any", "que",
    "com", "para", "uma", "nao", "dos", "das", "por", "mais", "como", "sem", "seu", "sua", "nos",
    "nas", "ser", "foi", "tem", "sao",
];

/// How alike two sets of [`telling`] words are: the share they hold in
/// common, with how many that is.
pub(super) fn overlap(left: &[String], right: &[String]) -> (usize, f64) {
    let shared = left.iter().filter(|word| right.contains(word)).count();
    let union = left.len() + right.len() - shared;
    if union == 0 {
        return (0, 0.0);
    }
    (shared, shared as f64 / union as f64)
}

#[cfg(test)]
mod tests {
    use super::{fold, overlap, telling};

    #[test]
    fn telling_words_leave_the_common_and_the_short_out() {
        assert_eq!(
            telling("Reply in pt-BR: answer the operator in Brazilian Portuguese"),
            ["answer", "brazilian", "operator", "portuguese", "reply"]
        );
        assert_eq!(overlap(&telling("a b"), &telling("c")), (0, 0.0));
    }

    #[test]
    fn accents_case_and_separators_fold_away() {
        assert_eq!(fold("Decisão"), "decisao");
        assert_eq!(fold("ÍNDICE da memória"), "indice da memoria");
        assert_eq!(fold("pt-BR reply_in"), "ptbr replyin");
        assert_eq!(fold("Straße œuvre"), "strasse oeuvre");
        assert_eq!(fold("日本"), "日本");
    }
}
