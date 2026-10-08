//! Speed schedule: "between 22:00 and 06:00 allow 5000 KiB/s". Pure logic with tests.
//!
//! One rule per line: `HH:MM-HH:MM kbps` (kbps 0 = unlimited in that window).
//! Windows may cross midnight. Lines starting with `#` and empty lines are ignored.
//! The first rule that matches the current time wins.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    /// Minutes since midnight.
    pub start: u32,
    pub end: u32,
    pub kbps: u64,
}

fn minutes(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

pub fn parse(text: &str) -> Result<Vec<Rule>, String> {
    let mut rules = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || {
            format!(
                "line {}: use HH:MM-HH:MM speed (example: 22:00-06:00 5000)",
                n + 1
            )
        };
        let (window, speed) = line.split_once(char::is_whitespace).ok_or_else(bad)?;
        let (a, b) = window.split_once('-').ok_or_else(bad)?;
        let (start, end) = (minutes(a).ok_or_else(bad)?, minutes(b).ok_or_else(bad)?);
        if start == end {
            return Err(format!("line {}: start and end are the same", n + 1));
        }
        let kbps: u64 = speed.trim().parse().map_err(|_| bad())?;
        rules.push(Rule { start, end, kbps });
    }
    Ok(rules)
}

/// The limit (KiB/s) that applies at `minute_of_day`, or None when no rule matches.
pub fn effective(rules: &[Rule], minute_of_day: u32) -> Option<u64> {
    rules
        .iter()
        .find(|r| {
            if r.start < r.end {
                (r.start..r.end).contains(&minute_of_day)
            } else {
                minute_of_day >= r.start || minute_of_day < r.end
            }
        })
        .map(|r| r.kbps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_and_comments() {
        let r = parse("# night\n22:00-06:00 0\n\n08:30-17:00 500\n").unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(
            r[1],
            Rule {
                start: 510,
                end: 1020,
                kbps: 500
            }
        );
    }

    #[test]
    fn rejects_bad_lines() {
        assert!(parse("22:00 500").is_err());
        assert!(parse("25:00-06:00 1").is_err());
        assert!(parse("10:00-10:00 1").is_err());
        assert!(parse("10:00-11:00 fast").is_err());
        assert!(parse("").unwrap().is_empty());
    }

    #[test]
    fn midnight_crossing_window() {
        let r = parse("22:00-06:00 0\n08:00-17:00 300").unwrap();
        assert_eq!(effective(&r, 23 * 60), Some(0));
        assert_eq!(effective(&r, 5 * 60 + 59), Some(0));
        assert_eq!(effective(&r, 6 * 60), None);
        assert_eq!(effective(&r, 12 * 60), Some(300));
        assert_eq!(effective(&r, 17 * 60), None);
    }

    #[test]
    fn first_match_wins() {
        let r = parse("08:00-12:00 100\n09:00-10:00 900").unwrap();
        assert_eq!(effective(&r, 9 * 60 + 30), Some(100));
    }
}
