use chrono::NaiveDate;

#[derive(Default)]
pub(crate) struct MatchContext {
    pub played_on: Option<NaiveDate>,
    pub event: Option<String>,
    pub round: Option<String>,
    pub place: Option<String>,
}

impl MatchContext {
    pub(super) fn new(
        played_on: Option<NaiveDate>,
        event: Option<&str>,
        round: Option<&str>,
        place: Option<&str>,
    ) -> Self {
        fn text(value: Option<&str>, limit: usize) -> Option<String> {
            value
                .map(str::trim)
                .filter(|value| {
                    !value.is_empty() && !value.contains('\0') && value.chars().count() <= limit
                })
                .map(str::to_owned)
        }

        Self {
            played_on,
            event: text(event, 240),
            round: text(round, 120),
            place: text(place, 240),
        }
    }

    pub(super) fn write_sgf(&self, output: &mut String) {
        let date = self.played_on.map(|value| value.to_string());
        for (key, value) in [
            ("DT", date.as_ref()),
            ("EV", self.event.as_ref()),
            ("RO", self.round.as_ref()),
            ("PC", self.place.as_ref()),
        ] {
            if let Some(value) = value {
                super::sgf::write_property(output, key, std::slice::from_ref(value));
            }
        }
    }
}

pub(crate) fn from_original(format: &str, bytes: &[u8]) -> anyhow::Result<MatchContext> {
    match format {
        "xg" => super::xg::read_match_context(bytes),
        "bgf" => super::bgf::read_match_context(bytes),
        _ => Ok(MatchContext::default()),
    }
}
