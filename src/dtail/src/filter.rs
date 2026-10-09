use regex::Regex;

/// Decides which containers to tail, by label.
pub struct Filter {
    include: Vec<Regex>,
    exclude: Vec<Regex>,
}

impl Filter {
    pub fn new(include: &[String], exclude: &[String]) -> Result<Self, regex::Error> {
        let compile = |ps: &[String]| ps.iter().map(|p| Regex::new(p)).collect::<Result<_, _>>();
        Ok(Self {
            include: compile(include)?,
            exclude: compile(exclude)?,
        })
    }

    pub fn matches(&self, label: &str) -> bool {
        (self.include.is_empty() || self.include.iter().any(|r| r.is_match(label)))
            && !self.exclude.iter().any(|r| r.is_match(label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(include: &[&str], exclude: &[&str]) -> Filter {
        let s = |v: &[&str]| v.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        Filter::new(&s(include), &s(exclude)).unwrap()
    }

    #[test]
    fn no_patterns_matches_everything() {
        assert!(filter(&[], &[]).matches("anything"));
    }

    #[test]
    fn any_include_pattern_matches() {
        let f = filter(&["order", "^iam$"], &[]);
        assert!(f.matches("kyberlifeorder"));
        assert!(f.matches("iam"));
        assert!(!f.matches("kyberlifeiam"));
    }

    #[test]
    fn exclude_wins_over_include() {
        let f = filter(&["kyberlife"], &["analytics"]);
        assert!(f.matches("kyberlifeorder"));
        assert!(!f.matches("kyberlifeanalytics"));
    }
}
