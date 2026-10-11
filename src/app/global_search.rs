//! Recherche globale de requêtes dans la collection par nom, méthode, chemin et URL brute.
//!
//! Ce module implémente la recherche globale ouverte par `Ctrl+K`. Elle cherche
//! dans l'ensemble des requêtes de la collection chargée selon leur nom affiché,
//! leur chemin relatif, leur méthode HTTP et leur URL brute telle qu'écrite dans
//! le fichier `.bru` (sans résolution de variables).

use std::ops::Range;
use std::path::PathBuf;

use super::text_input::TextInput;
use super::tree_filter::{contains_ci_diacritics, normalize_str};
use crate::collection::TreeNode;

/// Une requête candidate extraite de la collection en mémoire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchCandidate {
    /// Chemin relatif de la requête depuis la racine de la collection.
    pub path: PathBuf,
    /// Nom affiché (déclaré dans le fichier ou nom de fichier sans extension).
    pub name: String,
    /// Méthode HTTP en majuscules (ex: GET, POST).
    pub method: String,
    /// URL brute telle qu'écrite dans le `.bru`, avec ses {{variables}} non résolues.
    pub url: String,
}

/// Résultat d'une recherche pour une requête candidate avec score et surlignage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub candidate: SearchCandidate,
    /// Score de pertinence (plus élevé = plus pertinent).
    pub score: i32,
    /// Plages d'octets correspondant au motif dans le nom.
    pub name_matches: Vec<Range<usize>>,
    /// Plages d'octets correspondant au motif dans la méthode.
    pub method_matches: Vec<Range<usize>>,
    /// Plages d'octets correspondant au motif dans le chemin relatif.
    pub path_matches: Vec<Range<usize>>,
    /// Plages d'octets correspondant au motif dans l'URL.
    pub url_matches: Vec<Range<usize>>,
}

/// État du popup de recherche globale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalSearchPopup {
    /// Tampon de saisie de la recherche.
    pub input: TextInput,
    /// Indice du résultat actuellement sélectionné dans la liste filtrée.
    pub selected: usize,
    /// Décalage de défilement vertical de la liste des résultats.
    pub scroll: usize,
}

impl Default for GlobalSearchPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalSearchPopup {
    /// Crée un nouvel état de recherche globale avec une saisie vide.
    pub fn new() -> Self {
        Self {
            input: TextInput::new("", false),
            selected: 0,
            scroll: 0,
        }
    }

    /// Réinitialise la sélection et le défilement au début de la liste.
    pub fn reset_selection(&mut self) {
        self.selected = 0;
        self.scroll = 0;
    }

    /// Ajuste le défilement pour garantir que `selected` reste visible dans une
    /// fenêtre de hauteur `visible_height`.
    pub fn ensure_visible(&mut self, visible_height: usize) {
        if visible_height == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible_height {
            self.scroll = self.selected - visible_height + 1;
        }
    }
}

/// Extrait toutes les requêtes candidates de l'arborescence de la collection.
pub fn collect_candidates(tree: &[TreeNode]) -> Vec<SearchCandidate> {
    let mut candidates = Vec::new();
    collect_candidates_walk(tree, &mut candidates);
    candidates
}

fn collect_candidates_walk(nodes: &[TreeNode], candidates: &mut Vec<SearchCandidate>) {
    for node in nodes {
        match node {
            TreeNode::Request(req) => {
                candidates.push(SearchCandidate {
                    path: req.path.clone(),
                    name: node.name(),
                    method: req.view.method.clone(),
                    url: req.view.url.clone(),
                });
            }
            TreeNode::Folder(folder) => {
                collect_candidates_walk(&folder.children, candidates);
            }
            TreeNode::Error(_) => {}
        }
    }
}

/// Recherche et classe les candidats correspondant au motif.
pub fn search(candidates: &[SearchCandidate], query: &str) -> Vec<SearchResult> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return candidates
            .iter()
            .map(|c| SearchResult {
                candidate: c.clone(),
                score: 0,
                name_matches: Vec::new(),
                method_matches: Vec::new(),
                path_matches: Vec::new(),
                url_matches: Vec::new(),
            })
            .collect();
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let query_norm = normalize_str(trimmed);
    let mut results = Vec::new();

    for candidate in candidates {
        let path_str = candidate.path.to_string_lossy();
        let path_norm = normalize_str(&path_str);
        let name_norm = normalize_str(&candidate.name);
        let method_norm = normalize_str(&candidate.method);
        let url_norm = normalize_str(&candidate.url);

        // Tous les mots saisis doivent correspondre à au moins l'un des 4 champs.
        let all_tokens_match = tokens.iter().all(|token| {
            contains_ci_diacritics(&candidate.name, token)
                || contains_ci_diacritics(&path_str, token)
                || contains_ci_diacritics(&candidate.method, token)
                || contains_ci_diacritics(&candidate.url, token)
        });

        if !all_tokens_match {
            continue;
        }

        // Calcul du score de pertinence
        let mut score = 0i32;

        // 1. Correspondance exacte ou préfixe sur la requête entière
        if name_norm == query_norm {
            score += 1500;
        } else if name_norm.starts_with(&query_norm) {
            score += 800;
        } else if name_norm.contains(&query_norm) {
            score += 400;
        }

        if method_norm == query_norm {
            score += 600;
        } else if method_norm.starts_with(&query_norm) {
            score += 300;
        }

        let file_stem = candidate
            .path
            .file_stem()
            .map(|s| normalize_str(&s.to_string_lossy()))
            .unwrap_or_default();
        if file_stem == query_norm {
            score += 700;
        } else if file_stem.starts_with(&query_norm) {
            score += 400;
        } else if path_norm.contains(&query_norm) {
            score += 250;
        }

        if url_norm == query_norm {
            score += 500;
        } else if url_norm.starts_with(&query_norm) {
            score += 300;
        } else if url_norm.contains(&query_norm) {
            score += 150;
        }

        // 2. Score par token
        for token in &tokens {
            let t_norm = normalize_str(token);
            if name_norm == t_norm {
                score += 300;
            } else if name_norm.starts_with(&t_norm) {
                score += 150;
            } else if name_norm.contains(&t_norm) {
                score += 80;
            }

            if method_norm == t_norm {
                score += 200;
            } else if method_norm.starts_with(&t_norm) {
                score += 100;
            }

            if path_norm.contains(&t_norm) {
                score += 70;
            }

            if url_norm.contains(&t_norm) {
                score += 50;
            }
        }

        // Bonus de concision pour favoriser les noms plus courts à égalité de match
        score += 20_i32.saturating_sub(candidate.name.len() as i32);

        // Recherche des plages de correspondances pour la mise en évidence
        let name_matches = find_all_matches(&candidate.name, trimmed);
        let method_matches = find_all_matches(&candidate.method, trimmed);
        let path_matches = find_all_matches(&path_str, trimmed);
        let url_matches = find_all_matches(&candidate.url, trimmed);

        results.push(SearchResult {
            candidate: candidate.clone(),
            score,
            name_matches,
            method_matches,
            path_matches,
            url_matches,
        });
    }

    // Tri par score décroissant, puis par chemin pour la stabilité
    results.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.candidate.path.cmp(&b.candidate.path))
    });

    results
}

/// Recherche toutes les occurrences de `query` (phrase complète et tokens individuels)
/// dans `haystack`, sans tenir compte de la casse ni des diacritiques.
pub fn find_all_matches(haystack: &str, query: &str) -> Vec<Range<usize>> {
    let trimmed = query.trim();
    if trimmed.is_empty() || haystack.is_empty() {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    ranges.extend(find_matches_in_str(haystack, trimmed));
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.len() > 1 {
        for token in tokens {
            ranges.extend(find_matches_in_str(haystack, token));
        }
    }
    merge_ranges(ranges)
}

/// Recherche toutes les occurrences d'un terme unique dans une chaîne.
#[allow(clippy::needless_range_loop)]
fn find_matches_in_str(haystack: &str, needle: &str) -> Vec<Range<usize>> {
    let trimmed = needle.trim();
    if trimmed.is_empty() || haystack.is_empty() {
        return Vec::new();
    }
    let needle_norm = normalize_str(trimmed);
    let char_indices: Vec<(usize, char)> = haystack.char_indices().collect();
    let num_chars = char_indices.len();
    let mut matches = Vec::new();

    let mut start_idx = 0;
    while start_idx < num_chars {
        let byte_start = char_indices[start_idx].0;
        let mut found = false;

        for end_idx in (start_idx + 1)..=num_chars {
            let byte_end = if end_idx < num_chars {
                char_indices[end_idx].0
            } else {
                haystack.len()
            };
            let sub = &haystack[byte_start..byte_end];
            let sub_norm = normalize_str(sub);

            if sub_norm == needle_norm {
                matches.push(byte_start..byte_end);
                start_idx = end_idx;
                found = true;
                break;
            } else if sub_norm.len() > needle_norm.len() + 2 {
                break;
            }
        }

        if !found {
            start_idx += 1;
        }
    }

    matches
}

/// Fusionne et trie les plages d'octets qui se chevauchent ou se touchent.
pub fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    if ranges.is_empty() {
        return Vec::new();
    }
    ranges.sort_by_key(|r| (r.start, r.end));
    let mut merged = Vec::with_capacity(ranges.len());
    let mut current = ranges[0].clone();
    for r in ranges.into_iter().skip(1) {
        if r.start <= current.end {
            current.end = current.end.max(r.end);
        } else {
            merged.push(current);
            current = r;
        }
    }
    merged.push(current);
    merged
}

/// Tronque proprement une URL à `max_width` colonnes d'affichage tout en préservant
/// la mise en évidence des correspondances.
///
/// Si l'URL dépasse `max_width`, elle est tronquée avec une ellipse `…`.
/// Si une correspondance se trouve plus loin dans l'URL, la fenêtre d'affichage
/// est centrée pour rendre la correspondance visible avec des ellipses aux extrémités.
pub fn truncate_url_with_matches(
    url: &str,
    max_width: usize,
    matches: &[Range<usize>],
) -> (String, Vec<Range<usize>>) {
    let char_count = url.chars().count();
    if char_count <= max_width {
        return (url.to_owned(), matches.to_vec());
    }

    if max_width <= 2 {
        return ("…".to_owned(), Vec::new());
    }

    // Positions de caractères de chaque octet pour adapter les tranches
    let byte_to_char: Vec<usize> = url
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(url.len()))
        .collect();

    let first_match_char = matches
        .first()
        .and_then(|m| byte_to_char.iter().position(|&b| b >= m.start));

    // Si une correspondance est située au-delà de la zone visible initiale,
    // on centre la fenêtre autour de la correspondance.
    if let Some(m_start) = first_match_char
        && m_start >= max_width.saturating_sub(4)
    {
        // Fenêtre avec ellipse au début : "…<morceau>…"
        let prefix_dots = "…";
        let available = max_width.saturating_sub(2); // pour "…" au début et éventuellement à la fin
        let window_start = m_start.saturating_sub(available / 3);
        let window_end = (window_start + available).min(char_count);

        let byte_start = byte_to_char.get(window_start).copied().unwrap_or(0);
        let byte_end = byte_to_char.get(window_end).copied().unwrap_or(url.len());
        let sub = &url[byte_start..byte_end];

        let suffix_dots = if window_end < char_count { "…" } else { "" };
        let result = format!("{prefix_dots}{sub}{suffix_dots}");

        let offset = prefix_dots.len();
        let mut adjusted_matches = Vec::new();
        for m in matches {
            let clamped_start = m.start.max(byte_start).min(byte_end);
            let clamped_end = m.end.max(byte_start).min(byte_end);
            if clamped_start < clamped_end {
                adjusted_matches.push(
                    (clamped_start - byte_start + offset)..(clamped_end - byte_start + offset),
                );
            }
        }
        (result, adjusted_matches)
    } else {
        // Tronquage standard à la fin : "<début>…"
        let take_chars = max_width.saturating_sub(1);
        let byte_end = byte_to_char.get(take_chars).copied().unwrap_or(url.len());
        let result = format!("{}…", &url[..byte_end]);

        let mut adjusted_matches = Vec::new();
        for m in matches {
            let clamped_start = m.start.min(byte_end);
            let clamped_end = m.end.min(byte_end);
            if clamped_start < clamped_end {
                adjusted_matches.push(clamped_start..clamped_end);
            }
        }
        (result, adjusted_matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{RequestNode, RequestView};

    fn sample_candidate(name: &str, method: &str, path: &str, url: &str) -> SearchCandidate {
        SearchCandidate {
            name: name.to_owned(),
            method: method.to_owned(),
            path: PathBuf::from(path),
            url: url.to_owned(),
        }
    }

    fn sample_tree() -> Vec<TreeNode> {
        vec![
            TreeNode::Request(RequestNode {
                path: PathBuf::from("users/list.bru"),
                ast: None,
                view: RequestView {
                    name: Some("List Users".into()),
                    kind: None,
                    seq: Some(1),
                    method: "GET".into(),
                    url: "https://{{host}}/api/v1/users".into(),
                    headers: Vec::new(),
                    query_params: Vec::new(),
                    path_params: Vec::new(),
                    body: None,
                    auth: None,
                    has_pre_request_script: false,
                    has_post_response_script: false,
                    has_tests: false,
                    has_assert: false,
                    assertions: Vec::new(),
                },
            }),
            TreeNode::Request(RequestNode {
                path: PathBuf::from("users/create.bru"),
                ast: None,
                view: RequestView {
                    name: Some("Créer Utilisateur".into()),
                    kind: None,
                    seq: Some(2),
                    method: "POST".into(),
                    url: "https://{{host}}/api/v1/users".into(),
                    headers: Vec::new(),
                    query_params: Vec::new(),
                    path_params: Vec::new(),
                    body: None,
                    auth: None,
                    has_pre_request_script: false,
                    has_post_response_script: false,
                    has_tests: false,
                    has_assert: false,
                    assertions: Vec::new(),
                },
            }),
            TreeNode::Request(RequestNode {
                path: PathBuf::from("auth/login.bru"),
                ast: None,
                view: RequestView {
                    name: Some("Connexion".into()),
                    kind: None,
                    seq: Some(1),
                    method: "POST".into(),
                    url: "http://localhost:3000/auth/token".into(),
                    headers: Vec::new(),
                    query_params: Vec::new(),
                    path_params: Vec::new(),
                    body: None,
                    auth: None,
                    has_pre_request_script: false,
                    has_post_response_script: false,
                    has_tests: false,
                    has_assert: false,
                    assertions: Vec::new(),
                },
            }),
        ]
    }

    #[test]
    fn collect_candidates_from_tree() {
        let tree = sample_tree();
        let candidates = collect_candidates(&tree);
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates[0].name, "List Users");
        assert_eq!(candidates[0].method, "GET");
        assert_eq!(candidates[0].url, "https://{{host}}/api/v1/users");
        assert_eq!(candidates[1].name, "Créer Utilisateur");
        assert_eq!(candidates[2].name, "Connexion");
    }

    #[test]
    fn search_empty_query_returns_all_candidates() {
        let candidates = vec![
            sample_candidate("A", "GET", "a.bru", "http://a"),
            sample_candidate("B", "POST", "b.bru", "http://b"),
        ];
        let results = search(&candidates, "");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].candidate.name, "A");
        assert_eq!(results[1].candidate.name, "B");
    }

    #[test]
    fn search_matches_by_name_with_case_and_diacritics() {
        let candidates = vec![
            sample_candidate("Créer Utilisateur", "POST", "users/create.bru", "http://x"),
            sample_candidate("Autre Requête", "GET", "other.bru", "http://y"),
        ];
        // Recherche sans accent ni majuscule
        let results = search(&candidates, "creer");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].candidate.name, "Créer Utilisateur");
        assert!(!results[0].name_matches.is_empty());
        assert_eq!(
            &results[0].candidate.name[results[0].name_matches[0].clone()],
            "Créer"
        );

        // Recherche avec accent et majuscule
        let results2 = search(&candidates, "CRÉER");
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].candidate.name, "Créer Utilisateur");
    }

    #[test]
    fn search_matches_by_raw_url_and_unresolved_variables() {
        let candidates = vec![
            sample_candidate("A", "GET", "a.bru", "https://{{host}}/api/v1/items"),
            sample_candidate("B", "POST", "b.bru", "https://other.com/login"),
        ];
        // Recherche de la variable non résolue
        let results = search(&candidates, "{{host}}");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].candidate.name, "A");
        assert!(!results[0].url_matches.is_empty());
        assert_eq!(
            &results[0].candidate.url[results[0].url_matches[0].clone()],
            "{{host}}"
        );

        // Recherche d'un fragment de chemin dans l'URL
        let results2 = search(&candidates, "/api/v1");
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].candidate.name, "A");
    }

    #[test]
    fn search_matches_by_method() {
        let candidates = vec![
            sample_candidate("A", "GET", "a.bru", "http://a"),
            sample_candidate("B", "POST", "b.bru", "http://b"),
            sample_candidate("C", "DELETE", "c.bru", "http://c"),
        ];
        let results = search(&candidates, "delete");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].candidate.method, "DELETE");
        assert_eq!(
            &results[0].candidate.method[results[0].method_matches[0].clone()],
            "DELETE"
        );
    }

    #[test]
    fn search_matches_by_relative_path() {
        let candidates = vec![
            sample_candidate("A", "GET", "admin/special/list.bru", "http://a"),
            sample_candidate("B", "POST", "users/create.bru", "http://b"),
        ];
        let results = search(&candidates, "admin/special");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].candidate.name, "A");
        let path_str = results[0].candidate.path.to_string_lossy();
        assert_eq!(
            &path_str[results[0].path_matches[0].clone()],
            "admin/special"
        );
    }

    #[test]
    fn search_multi_token_matches_across_fields() {
        let candidates = vec![
            sample_candidate(
                "List Users",
                "GET",
                "users/list.bru",
                "https://{{host}}/users",
            ),
            sample_candidate(
                "Create User",
                "POST",
                "users/create.bru",
                "https://{{host}}/users",
            ),
        ];
        // "get user" : "get" dans la méthode, "user" dans le nom
        let results = search(&candidates, "get user");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].candidate.name, "List Users");
    }

    #[test]
    fn search_ranking_exact_name_beats_substring_or_url() {
        let candidates = vec![
            sample_candidate("Unrelated", "GET", "sub/ping.bru", "http://localhost/ping"),
            sample_candidate("ping", "GET", "test.bru", "http://localhost/test"),
            sample_candidate(
                "prefix_ping_suffix",
                "GET",
                "other.bru",
                "http://localhost/x",
            ),
        ];
        let results = search(&candidates, "ping");
        assert_eq!(results.len(), 3);
        // Le nom exact "ping" doit arriver en première position
        assert_eq!(results[0].candidate.name, "ping");
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn truncate_url_short_url_keeps_exact_matches() {
        let url = "https://{{host}}/api";
        let matches = vec![Range { start: 8, end: 16 }]; // "{{host}}"
        let (truncated, adjusted) = truncate_url_with_matches(url, 40, &matches);
        assert_eq!(truncated, url);
        assert_eq!(adjusted, matches);
    }

    #[test]
    fn truncate_url_long_url_with_ellipsis() {
        let url = "https://very-long-domain-name.example.com/api/v1/resource/subpath";
        let matches = vec![Range { start: 0, end: 5 }]; // "https"
        let (truncated, adjusted) = truncate_url_with_matches(url, 25, &matches);
        assert_eq!(truncated.chars().count(), 25);
        assert!(truncated.ends_with('…'));
        assert_eq!(adjusted, vec![Range { start: 0, end: 5 }]);
    }

    #[test]
    fn truncate_url_centers_window_around_distant_match() {
        let url = "https://domain.com/users/12345/profile/settings/avatar";
        let matches = find_matches_in_str(url, "avatar");
        assert_eq!(matches, vec![48..54]);
        let (truncated, adjusted) = truncate_url_with_matches(url, 20, &matches);
        assert!(truncated.contains("avatar"), "reçu {truncated}");
        assert!(!adjusted.is_empty());
        assert_eq!(&truncated[adjusted[0].clone()], "avatar");
    }

    #[test]
    fn truncate_url_very_small_width() {
        let url = "https://domain.com";
        let (truncated, adjusted) = truncate_url_with_matches(url, 2, &[]);
        assert_eq!(truncated, "…");
        assert!(adjusted.is_empty());
    }

    #[test]
    fn search_no_match_returns_empty() {
        let candidates = vec![sample_candidate("Ping", "GET", "ping.bru", "http://ping")];
        let results = search(&candidates, "nonexistent_token_xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn search_handles_ligatures() {
        let candidates = vec![sample_candidate(
            "Cœur de cible",
            "GET",
            "target.bru",
            "http://target",
        )];
        let results = search(&candidates, "coeur");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].candidate.name, "Cœur de cible");
    }

    #[test]
    fn popup_ensure_visible_adjusts_scroll() {
        let mut popup = GlobalSearchPopup::new();
        popup.selected = 10;
        popup.ensure_visible(5);
        // La fenêtre visible fait 5 éléments : selected (10) - 5 + 1 = 6
        assert_eq!(popup.scroll, 6);

        popup.selected = 3;
        popup.ensure_visible(5);
        assert_eq!(popup.scroll, 3);

        popup.reset_selection();
        assert_eq!(popup.selected, 0);
        assert_eq!(popup.scroll, 0);
    }
}
