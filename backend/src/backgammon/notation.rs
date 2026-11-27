//! Human-readable checker point and step formatting/parsing in mover-relative
//! coordinates (`25` = bar, `1..=24` = points, `0` = off).
use std::borrow::Cow;

pub(crate) fn format_point(point: usize) -> Cow<'static, str> {
    match point {
        25 => Cow::Borrowed("bar"),
        0 => Cow::Borrowed("off"),
        point => Cow::Owned(point.to_string()),
    }
}

pub(crate) fn format_step(from: usize, to: usize, hit: bool) -> String {
    format!(
        "{}/{}{}",
        format_point(from),
        format_point(to),
        if hit { "*" } else { "" }
    )
}

/// Parses mover-relative checker notation such as `"24/20 13/8"`,
/// `"bar/22* 6/4(2)"`, `"13/7/5"`, or `"Cannot move"` into `(from, to)` runs
/// (`25` = bar, `0` = off). Rejects empty, overlong (>160 bytes), or >4-run
/// inputs.
pub(crate) fn parse_steps(notation: &str) -> Option<Vec<(usize, usize)>> {
    let trimmed = notation.trim();
    if trimmed.is_empty() || notation.len() > 160 {
        return None;
    }
    if matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "cannot move" | "can't move" | "dance"
    ) {
        return Some(Vec::new());
    }
    let mut runs = Vec::new();
    for token in trimmed.split_whitespace() {
        let (path, count) =
            if let Some((path, count)) = token.strip_suffix(')').and_then(|s| s.rsplit_once('(')) {
                let count: usize = count.parse().ok()?;
                if !(1..=4).contains(&count) {
                    return None;
                }
                (path, count)
            } else {
                (token, 1)
            };
        let points: Option<Vec<usize>> = path
            .split('/')
            .map(|point| {
                let point = point.strip_suffix('*').unwrap_or(point);
                match point.to_ascii_lowercase().as_str() {
                    "bar" => Some(25),
                    "off" => Some(0),
                    number => number
                        .parse::<usize>()
                        .ok()
                        .filter(|p| (1..=24).contains(p)),
                }
            })
            .collect();
        let points = points?;
        if points.len() < 2 {
            return None;
        }
        for _ in 0..count {
            for pair in points.windows(2) {
                runs.push((pair[0], pair[1]));
            }
        }
        if runs.len() > 4 {
            return None;
        }
    }
    Some(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses_checker_notation() {
        assert_eq!(format_step(25, 22, true), "bar/22*");
        assert_eq!(format_step(6, 0, false), "6/off");
        assert_eq!(parse_steps("Cannot move"), Some(vec![]));
        assert_eq!(parse_steps("dance"), Some(vec![]));
        assert_eq!(
            parse_steps("bar/22* 6/4(2) 2/off"),
            Some(vec![(25, 22), (6, 4), (6, 4), (2, 0)])
        );
        assert_eq!(parse_steps("13/8/5"), Some(vec![(13, 8), (8, 5)]));
        assert_eq!(parse_steps(""), None);
        assert_eq!(parse_steps("6/5(5)"), None);
        assert_eq!(parse_steps("25/20"), None);
    }
}
