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
        for (key, value) in self.sgf_properties() {
            super::sgf::write_property(output, key, &[value]);
        }
    }

    pub(super) fn sgf_properties(&self) -> impl Iterator<Item = (&'static str, String)> {
        [
            ("DT", self.played_on.map(|value| value.to_string())),
            ("EV", self.event.clone()),
            ("RO", self.round.clone()),
            ("PC", self.place.clone()),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
    }
}
