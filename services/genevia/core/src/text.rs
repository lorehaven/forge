//! Text helpers: folding for search.

/// Lower-cases and drops diacritics, so `Łaziska` matches `laziska` and `Węgrzyn` matches `wegrzyn`.
#[must_use]
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let f = match c {
            'ą' | 'ä' | 'á' | 'à' | 'â' => 'a',
            'ć' | 'č' => 'c',
            'ę' | 'é' | 'ě' | 'è' => 'e',
            'í' | 'ì' => 'i',
            'ł' => 'l',
            'ń' | 'ň' => 'n',
            'ó' | 'ö' | 'ô' => 'o',
            'ś' | 'š' => 's',
            'ť' => 't',
            'ú' | 'ü' | 'ů' => 'u',
            'ý' => 'y',
            'ź' | 'ż' | 'ž' => 'z',
            other => other,
        };
        if c == 'ß' {
            out.push_str("ss");
        } else {
            out.push(f);
        }
    }
    out
}
