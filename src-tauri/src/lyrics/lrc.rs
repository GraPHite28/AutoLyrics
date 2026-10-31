//! Разбор формата LRC: `[mm:ss.xx] текст`.

use super::{matching, Line, Query};

pub fn parse(src: &str) -> Vec<Line> {
    let mut offset_ms: i64 = 0;
    let mut out: Vec<Line> = Vec::new();

    for raw in src.lines() {
        let mut rest = raw.trim();
        let mut times: Vec<i64> = Vec::new();

        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some(ms) = parse_time(tag) {
                times.push(ms);
            } else if let Some(value) = tag.strip_prefix("offset:") {
                offset_ms = value.trim().parse().unwrap_or(0);
            }
            rest = rest[end + 1..].trim_start();
        }

        if times.is_empty() {
            continue;
        }
        let text = strip_word_tags(rest).replace('\u{a0}', " ").trim().to_string();
        for t in times {
            out.push(Line {
                t,
                text: text.clone(),
            });
        }
    }

    // По спецификации положительный offset сдвигает текст раньше.
    for line in &mut out {
        line.t = (line.t - offset_ms).max(0);
    }
    out.sort_by_key(|l| l.t);
    out.dedup_by(|b, a| a.text.is_empty() && b.text.is_empty());
    out
}

/// Убирает служебные строки в начале: «作词 : …», «Lyrics by: …» и заголовок
/// «Название - Артист» — их добавляют сервисы, и в караоке они только мешают.
pub fn strip_credits(lines: Vec<Line>, q: &Query) -> Vec<Line> {
    const MARKERS: [&str; 12] = [
        "作词", "作曲", "编曲", "制作", "作詞", "词：", "曲：", "lyrics by", "composed by",
        "produced by", "arranged by", "composer",
    ];
    let title = matching::flat(&q.clean_title);
    let artist = matching::flat(&q.main_artist);

    lines
        .into_iter()
        .filter(|line| {
            if line.t > 20_000 {
                return true;
            }
            let lower = line.text.to_lowercase();
            if MARKERS.iter().any(|m| lower.contains(m)) {
                return false;
            }
            if line.t <= 5_000 && !title.is_empty() && !artist.is_empty() {
                let flat = matching::flat(&line.text);
                let is_header = flat.contains(&title)
                    && flat.contains(&artist)
                    && flat.chars().count() <= title.chars().count() + artist.chars().count() + 4;
                return !is_header;
            }
            true
        })
        .collect()
}

fn parse_time(tag: &str) -> Option<i64> {
    let (min, rest) = tag.split_once(':')?;
    let min: i64 = min.trim().parse().ok()?;
    let (sec, frac) = match rest.split_once('.') {
        Some((s, f)) => (s, f),
        None => (rest, ""),
    };
    let sec: i64 = sec.trim().parse().ok()?;
    let frac_ms: i64 = if frac.is_empty() {
        0
    } else {
        let digits: String = frac.chars().take(3).collect();
        format!("{digits:0<3}").parse().ok()?
    };
    Some((min * 60 + sec) * 1000 + frac_ms)
}

/// Убирает пословные метки расширенного LRC: `<00:12.34>`.
fn strip_word_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}
