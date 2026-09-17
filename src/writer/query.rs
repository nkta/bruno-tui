//! Chaîne de requête d'une URL, découpée et reconstruite selon les règles
//! de Bruno (`@usebruno/common`, `parseQueryParams` et `buildQueryString`) :
//! aucun encodage ni décodage, découpe sur `&` puis sur le premier `=`,
//! paires de nom vide ignorées.

/// URL découpée en trois parties : avant `?`, chaîne de requête, fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SplitUrl<'a> {
    pub base: &'a str,
    /// Texte après le premier `?` (et avant `#`), `None` sans `?`.
    pub query: Option<&'a str>,
    /// Texte après le premier `#`, `None` sans `#`.
    pub fragment: Option<&'a str>,
}

/// Découpe `url` sur le premier `#`, puis sur le premier `?` de ce qui
/// précède.
pub(crate) fn split_url(url: &str) -> SplitUrl<'_> {
    let (before_hash, fragment) = match url.split_once('#') {
        Some((before, after)) => (before, Some(after)),
        None => (url, None),
    };
    let (base, query) = match before_hash.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (before_hash, None),
    };
    SplitUrl {
        base,
        query,
        fragment,
    }
}

/// Paires `(nom, valeur)` d'une chaîne de requête, dans l'ordre.
pub(crate) fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (!name.is_empty()).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect()
}

/// Reconstruit l'URL à partir de sa base, de son fragment et des paires
/// activées : `nom=valeur`, ou `nom` pour une valeur vide, jointes par `&` ;
/// sans paire, l'URL ne porte pas de `?`.
pub(crate) fn build_url(split: &SplitUrl<'_>, enabled: &[(&str, &str)]) -> String {
    let query = enabled
        .iter()
        .filter(|(name, _)| !name.trim().is_empty())
        .map(|(name, value)| {
            if value.is_empty() {
                (*name).to_owned()
            } else {
                format!("{name}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    let mut url = split.base.to_owned();
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query);
    }
    if let Some(fragment) = split.fragment {
        url.push('#');
        url.push_str(fragment);
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_without_query_nor_fragment() {
        let split = split_url("https://{{host}}/items");
        assert_eq!(split.base, "https://{{host}}/items");
        assert_eq!(split.query, None);
        assert_eq!(split.fragment, None);
    }

    #[test]
    fn split_with_query_and_fragment() {
        let split = split_url("https://h/items?page=2#top");
        assert_eq!(split.base, "https://h/items");
        assert_eq!(split.query, Some("page=2"));
        assert_eq!(split.fragment, Some("top"));
    }

    #[test]
    fn question_mark_after_hash_belongs_to_the_fragment() {
        let split = split_url("https://h/a#frag?x=1");
        assert_eq!(split.base, "https://h/a");
        assert_eq!(split.query, None);
        assert_eq!(split.fragment, Some("frag?x=1"));
    }

    #[test]
    fn parse_duplicates_flags_and_equals_in_value() {
        assert_eq!(
            parse_query("tag=a&tag=b&flag&a=b=c&=ignored&"),
            vec![
                ("tag".to_owned(), "a".to_owned()),
                ("tag".to_owned(), "b".to_owned()),
                ("flag".to_owned(), String::new()),
                ("a".to_owned(), "b=c".to_owned()),
            ]
        );
    }

    #[test]
    fn variables_are_not_touched() {
        assert_eq!(
            parse_query("id={{id}}&q={{ term }}"),
            vec![
                ("id".to_owned(), "{{id}}".to_owned()),
                ("q".to_owned(), "{{ term }}".to_owned()),
            ]
        );
    }

    #[test]
    fn build_keeps_base_and_fragment() {
        let split = split_url("https://h/items?page=2#top");
        assert_eq!(build_url(&split, &[]), "https://h/items#top");
        assert_eq!(
            build_url(&split, &[("page", "3"), ("flag", "")]),
            "https://h/items?page=3&flag#top"
        );
    }

    #[test]
    fn build_without_query_on_plain_url() {
        let split = split_url("https://{{host}}/items");
        assert_eq!(
            build_url(&split, &[("page", "2")]),
            "https://{{host}}/items?page=2"
        );
    }
}
