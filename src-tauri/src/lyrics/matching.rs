//! Общие правила сопоставления: название/артист из плеера ↔ кандидат из базы текстов.

use std::sync::OnceLock;

use regex::Regex;

use super::Query;

/// Максимально допустимое расхождение длительности (live/remix — другая версия).
const MAX_DURATION_DIFF_S: f64 = 6.0;

/// Только буквы и цифры в нижнем регистре — для нестрогого сравнения.
pub fn flat(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Название без скобок, `feat.` и хвостов вроде «- Remastered 2011».
pub fn normalize_title(s: &str) -> String {
    static BRACKETS: OnceLock<Regex> = OnceLock::new();
    static SUFFIX: OnceLock<Regex> = OnceLock::new();
    static FEAT: OnceLock<Regex> = OnceLock::new();

    let brackets = BRACKETS.get_or_init(|| Regex::new(r"\s*[\(\[][^\)\]]*[\)\]]").unwrap());
    let suffix = SUFFIX.get_or_init(|| {
        Regex::new(r"(?i)\s+-\s+.*(remaster|live|version|edit|mono|stereo|radio|single|deluxe|bonus).*$")
            .unwrap()
    });
    let feat = FEAT.get_or_init(|| Regex::new(r"(?i)\s+(feat\.?|ft\.?|featuring)\s+.*$").unwrap());

    let t = brackets.replace_all(s, "");
    let t = suffix.replace(&t, "");
    let t = feat.replace(&t, "");
    t.trim().to_string()
}

/// Первый из перечисленных артистов («A, B & C» → «A»).
pub fn primary_artist(s: &str) -> String {
    static SPLIT: OnceLock<Regex> = OnceLock::new();
    let split = SPLIT.get_or_init(|| {
        Regex::new(r"(?i)\s*(,|&|、|\s+feat\.?\s|\s+ft\.?\s|\s+featuring\s|\s+x\s)\s*").unwrap()
    });
    let cleaned = s.trim().trim_end_matches(" - Topic");
    split
        .split(cleaned)
        .next()
        .unwrap_or(cleaned)
        .trim()
        .to_string()
}

/// Все артисты из строки («A & B, C feat. D» → [A, B, C, D]).
pub fn artist_list(s: &str) -> Vec<String> {
    static SPLIT: OnceLock<Regex> = OnceLock::new();
    let split = SPLIT.get_or_init(|| {
        Regex::new(r"(?i)\s*(,|&amp;|&|、|/|\s+feat\.?\s|\s+ft\.?\s|\s+featuring\s|\s+x\s)\s*").unwrap()
    });
    let mut out: Vec<String> = Vec::new();
    for part in split.split(s.trim().trim_end_matches(" - Topic")) {
        let part = part.trim();
        if !part.is_empty() && !out.iter().any(|a| flat(a) == flat(part)) {
            out.push(part.to_string());
        }
    }
    out
}

/// Варианты поисковой строки: у каждого артиста по отдельности (сервисы
/// записывают «Miyagi & Эндшпиль» по-разному), затем только название.
pub fn keywords(q: &Query) -> Vec<String> {
    let mut out: Vec<String> = q
        .artists
        .iter()
        .take(3)
        .map(|a| format!("{a} {}", q.clean_title).trim().to_string())
        .collect();
    out.push(q.clean_title.clone());
    out.dedup();
    out
}

/// Насколько кандидат подходит под запрос; `None` — не подходит совсем.
/// Чем больше число, тем лучше.
pub fn score(q: &Query, title: &str, artist: &str, duration_s: Option<f64>) -> Option<i32> {
    let mut score = 0;

    if q.duration_s > 0.0 {
        if let Some(d) = duration_s.filter(|d| *d > 0.0) {
            let diff = (d - q.duration_s).abs();
            if diff > MAX_DURATION_DIFF_S {
                return None;
            }
            score += (10.0 - diff.min(10.0)) as i32;
        }
    }

    let cand_title = flat(&normalize_title(title));
    let want_title = flat(&q.clean_title);
    if !want_title.is_empty() {
        if cand_title == want_title {
            score += 30;
        } else if cand_title.contains(&want_title) || want_title.contains(&cand_title) {
            score += 10;
        } else {
            return None;
        }
    }
    // Название совпало буква в букву, включая «(Live)» и т. п. — версия та самая.
    if flat(title) == flat(&q.title) {
        score += 8;
    }

    // подходит любой из перечисленных артистов
    let cand_artist = flat(artist);
    let wanted: Vec<String> = q.artists.iter().map(|a| flat(a)).filter(|a| !a.is_empty()).collect();
    if !wanted.is_empty() {
        let hit = wanted
            .iter()
            .any(|w| cand_artist.contains(w.as_str()) || (!cand_artist.is_empty() && w.contains(&cand_artist)));
        if hit {
            score += 20;
        } else {
            return None;
        }
    }
    Some(score)
}

/// Индексы подходящих кандидатов: лучшие первыми, при равенстве — в порядке выдачи сервиса.
pub fn rank(scores: &[Option<i32>], limit: usize) -> Vec<usize> {
    let mut ranked: Vec<(i32, usize)> = scores
        .iter()
        .enumerate()
        .filter_map(|(i, s)| s.map(|s| (s, i)))
        .collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    ranked.into_iter().take(limit).map(|(_, i)| i).collect()
}
