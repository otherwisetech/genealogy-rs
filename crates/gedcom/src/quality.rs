//! Data-quality review: likely errors across the whole tree (impossible dates, odd ages, broken
//! links, cycles, possible duplicates).
//!
//! Dates are compared as day ranges: «1850» is all of 1850, «BEF 1850» is open to the left,
//! «BET 1840 AND 1850» spans both, «ABT 1850» is widened by a year and marks the issue as
//! approximate (one severity lower). A check only fires when it holds for every day in the ranges,
//! so imprecise dates never produce false certainties; free-text dates are ignored.
use crate::date::{DateValue, Precision};
use crate::tree::{Person, Sex, Tree, year_of};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub const ALL: [Severity; 3] = [Severity::Error, Severity::Warning, Severity::Info];

    /// Localized group title: Errores / Avisos / Información.
    pub fn label(self) -> String {
        match self {
            Severity::Error => i18n::t!("core.quality.severity.error"),
            Severity::Warning => i18n::t!("core.quality.severity.warning"),
            Severity::Info => i18n::t!("core.quality.severity.info"),
        }
    }

    fn lower(self) -> Severity {
        match self {
            Severity::Error => Severity::Warning,
            _ => Severity::Info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IssueKind {
    OwnAncestor,
    DeathBeforeBirth,
    BurialBeforeDeath,
    BornBeforeFather,
    BornBeforeMother,
    BornAfterFatherDeath,
    BornAfterMotherDeath,
    MarriedBeforeBirth,
    VeryOld,
    FatherTooYoung,
    MotherTooYoung,
    FatherTooOld,
    MotherTooOld,
    MarriedTooYoung,
    MarriedAfterDeath,
    SiblingsTooClose,
    HusbandFemale,
    WifeMale,
    DuplicateChild,
    EmptyFamily,
    MissingName,
    PossiblyDeceased,
    LargeAgeGap,
    SingleMemberFamily,
    Isolated,
    PossibleDuplicate,
}

impl IssueKind {
    /// Localized group title («Defunción antes del nacimiento»…).
    pub fn label(self) -> String {
        use IssueKind::*;
        match self {
            OwnAncestor => i18n::t!("core.quality.kind.own_ancestor"),
            DeathBeforeBirth => i18n::t!("core.quality.kind.death_before_birth"),
            BurialBeforeDeath => i18n::t!("core.quality.kind.burial_before_death"),
            BornBeforeFather => i18n::t!("core.quality.kind.born_before_father"),
            BornBeforeMother => i18n::t!("core.quality.kind.born_before_mother"),
            BornAfterFatherDeath => i18n::t!("core.quality.kind.born_after_father_death"),
            BornAfterMotherDeath => i18n::t!("core.quality.kind.born_after_mother_death"),
            MarriedBeforeBirth => i18n::t!("core.quality.kind.married_before_birth"),
            VeryOld => i18n::t!("core.quality.kind.very_old"),
            FatherTooYoung => i18n::t!("core.quality.kind.father_too_young"),
            MotherTooYoung => i18n::t!("core.quality.kind.mother_too_young"),
            FatherTooOld => i18n::t!("core.quality.kind.father_too_old"),
            MotherTooOld => i18n::t!("core.quality.kind.mother_too_old"),
            MarriedTooYoung => i18n::t!("core.quality.kind.married_too_young"),
            MarriedAfterDeath => i18n::t!("core.quality.kind.married_after_death"),
            SiblingsTooClose => i18n::t!("core.quality.kind.siblings_too_close"),
            HusbandFemale => i18n::t!("core.quality.kind.husband_female"),
            WifeMale => i18n::t!("core.quality.kind.wife_male"),
            DuplicateChild => i18n::t!("core.quality.kind.duplicate_child"),
            EmptyFamily => i18n::t!("core.quality.kind.empty_family"),
            MissingName => i18n::t!("core.quality.kind.missing_name"),
            PossiblyDeceased => i18n::t!("core.quality.kind.possibly_deceased"),
            LargeAgeGap => i18n::t!("core.quality.kind.large_age_gap"),
            SingleMemberFamily => i18n::t!("core.quality.kind.single_member_family"),
            Isolated => i18n::t!("core.quality.kind.isolated"),
            PossibleDuplicate => i18n::t!("core.quality.kind.possible_duplicate"),
        }
    }
}

/// One finding. `people[0]` is the person the message is about (the one to focus); the rest are
/// the others involved (parent, partner, sibling, duplicate, cycle members).
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub kind: IssueKind,
    pub severity: Severity,
    pub people: Vec<String>,
    pub family: Option<String>,
    /// numbers shown in the message (years, ages, days); meaning depends on `kind`
    pub a: i64,
    pub b: i64,
    /// raw name of the other person named in the message ("" when unnamed)
    pub other: Option<String>,
    /// an «ABT» date was involved, so the severity was lowered one step
    pub approximate: bool,
}

impl Issue {
    fn new(kind: IssueKind, severity: Severity, people: Vec<String>) -> Issue {
        Issue { kind, severity, people, family: None, a: 0, b: 0, other: None, approximate: false }
    }

    fn fam(mut self, f: &str) -> Self {
        self.family = Some(f.to_string());
        self
    }

    fn nums(mut self, a: i64, b: i64) -> Self {
        self.a = a;
        self.b = b;
        self
    }

    fn other(mut self, p: Option<&Person>) -> Self {
        self.other = p.map(raw_name);
        self
    }

    fn approx(mut self, approx: bool) -> Self {
        if approx {
            self.approximate = true;
            self.severity = self.severity.lower();
        }
        self
    }

    /// Stable identity, e.g. for a session «ignore» set.
    pub fn key(&self) -> String {
        format!("{:?}|{}|{}", self.kind, self.people.join(","), self.family.as_deref().unwrap_or(""))
    }

    /// Localized sentence: «Nació en 1850, antes que su madre (1862)».
    pub fn message(&self) -> String {
        use IssueKind::*;
        let (a, b) = (self.a, self.b);
        let name = || match self.other.as_deref().map(str::trim) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => i18n::t!("core.person.unnamed"),
        };
        let fam = self.family.as_deref().unwrap_or("");
        match self.kind {
            OwnAncestor => i18n::tn!("core.quality.msg.own_ancestor", self.people.len()),
            DeathBeforeBirth => i18n::t!("core.quality.msg.death_before_birth", death = a, birth = b),
            BurialBeforeDeath => i18n::t!("core.quality.msg.burial_before_death", burial = a, death = b),
            BornBeforeFather => i18n::t!("core.quality.msg.born_before_father", birth = a, parent = b),
            BornBeforeMother => i18n::t!("core.quality.msg.born_before_mother", birth = a, parent = b),
            BornAfterFatherDeath => i18n::t!("core.quality.msg.born_after_father_death", birth = a, death = b),
            BornAfterMotherDeath => i18n::t!("core.quality.msg.born_after_mother_death", birth = a, death = b),
            MarriedBeforeBirth => i18n::t!("core.quality.msg.married_before_birth", marriage = a, birth = b),
            VeryOld => i18n::t!("core.quality.msg.very_old", age = a),
            FatherTooYoung | FatherTooOld => i18n::t!("core.quality.msg.father_age", age = a),
            MotherTooYoung | MotherTooOld => i18n::t!("core.quality.msg.mother_age", age = a),
            MarriedTooYoung => i18n::t!("core.quality.msg.married_too_young", age = a, name = name()),
            MarriedAfterDeath => i18n::t!("core.quality.msg.married_after_death", marriage = a, death = b, name = name()),
            SiblingsTooClose => i18n::tn!("core.quality.msg.siblings_too_close", a, name = name()),
            HusbandFemale => i18n::t!("core.quality.msg.husband_female", family = fam),
            WifeMale => i18n::t!("core.quality.msg.wife_male", family = fam),
            DuplicateChild => i18n::t!("core.quality.msg.duplicate_child", family = fam),
            EmptyFamily => i18n::t!("core.quality.msg.empty_family", family = fam),
            MissingName => i18n::t!("core.quality.msg.missing_name"),
            PossiblyDeceased => i18n::t!("core.quality.msg.possibly_deceased", birth = a),
            LargeAgeGap => i18n::t!("core.quality.msg.large_age_gap", years = a, name = name()),
            SingleMemberFamily => i18n::t!("core.quality.msg.single_member_family", family = fam),
            Isolated => i18n::t!("core.quality.msg.isolated"),
            PossibleDuplicate => i18n::t!("core.quality.msg.possible_duplicate", name = name(), score = a),
        }
    }
}

fn raw_name(p: &Person) -> String {
    format!("{} {}", p.given.trim(), p.surname.trim()).trim().to_string()
}

const YEAR: f64 = 365.2425;
const OPEN_LO: i64 = i64::MIN / 4;
const OPEN_HI: i64 = i64::MAX / 4;
const MAX_AGE: i64 = 110;
const FATHER_MIN: i64 = 13;
const MOTHER_MIN: i64 = 12;
const FATHER_MAX: i64 = 80;
const MOTHER_MAX: i64 = 55;
const MARRIAGE_MIN: i64 = 13;
const AGE_GAP: i64 = 30;
/// births closer than this (and further apart than `TWINS`) are flagged; ≈ 9 months
const SIBLING_GAP_DAYS: i64 = 274;
/// twins can straddle midnight
const TWINS_DAYS: i64 = 2;
const DUPLICATE_MIN: f32 = 0.85;

/// A date as the range of days it can mean.
#[derive(Debug, Clone, Copy)]
struct Span {
    lo: i64,
    hi: i64,
    approx: bool,
    /// a full exact day ("12 MAR 1852")
    day: bool,
    /// year shown in messages
    year: i64,
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// (first day, last day, has day) of a normalized "12 MAR 1852" / "MAR 1852" / "1852".
fn bounds(date: &str) -> Option<(i64, i64, bool)> {
    const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
    let w: Vec<&str> = date.split_whitespace().collect();
    let month = |s: &str| MONTHS.iter().position(|m| *m == s).map(|i| i as i64 + 1);
    let year = |s: &str| s.parse::<i64>().ok().filter(|y| *y > 0);
    let last = |y: i64, m: i64| if m == 12 { days_from_civil(y + 1, 1, 1) - 1 } else { days_from_civil(y, m + 1, 1) - 1 };
    match w.as_slice() {
        [y] => {
            let y = year(y)?;
            Some((days_from_civil(y, 1, 1), days_from_civil(y, 12, 31), false))
        }
        [m, y] => {
            let (m, y) = (month(m)?, year(y)?);
            Some((days_from_civil(y, m, 1), last(y, m), false))
        }
        [d, m, y] => {
            let (d, m, y) = (d.parse::<i64>().ok()?, month(m)?, year(y)?);
            let day = days_from_civil(y, m, d);
            Some((day, day, true))
        }
        _ => None,
    }
}

fn span(raw: &str) -> Option<Span> {
    let dv = DateValue::parse(raw);
    let year = year_of(raw)? as i64;
    let (lo, hi, day, approx) = match dv.precision {
        Precision::Exact => {
            let (l, h, d) = bounds(&dv.date)?;
            (l, h, d, false)
        }
        Precision::About => {
            let (l, h, _) = bounds(&dv.date)?;
            (l - 366, h + 366, false, true)
        }
        Precision::Before => (OPEN_LO, bounds(&dv.date)?.1, false, false),
        Precision::After => (bounds(&dv.date)?.0, OPEN_HI, false, false),
        Precision::Between => {
            let (a, b) = (bounds(&dv.date)?, bounds(&dv.date2)?);
            (a.0.min(b.0), a.1.max(b.1), false, false)
        }
        Precision::Phrase => return None,
    };
    Some(Span { lo, hi, approx, day, year })
}

/// Fewest days that can separate `from` and `to` (negative: `to` may come first).
fn min_days(from: Span, to: Span) -> Option<i64> {
    (from.hi < OPEN_HI && to.lo > OPEN_LO).then_some(to.lo - from.hi)
}

/// Most days that can separate `from` and `to`.
fn max_days(from: Span, to: Span) -> Option<i64> {
    (from.lo > OPEN_LO && to.hi < OPEN_HI).then_some(to.hi - from.lo)
}

/// `a` is certainly before `b`.
fn before(a: Span, b: Span) -> bool {
    min_days(a, b).is_some_and(|d| d > 0)
}

fn years(days: i64) -> i64 {
    (days as f64 / YEAR).floor() as i64
}

fn today() -> i64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (secs / 86_400) as i64
}

#[derive(Default, Clone, Copy)]
struct Facts {
    /// BIRT, else CHR / BAPM
    birth: Option<Span>,
    /// DEAT, else BURI / CREM
    death: Option<Span>,
    deat: Option<Span>,
    burial: Option<Span>,
    /// any death-like event, dated or not
    dead: bool,
}

fn first_span(p: &Person, tags: &[&str]) -> Option<Span> {
    tags.iter().find_map(|t| p.events.iter().filter(|e| e.tag == *t).find_map(|e| e.date.as_deref().and_then(span)))
}

fn facts(p: &Person) -> Facts {
    let deat = first_span(p, &["DEAT"]);
    let burial = first_span(p, &["BURI", "CREM"]);
    Facts { birth: first_span(p, &["BIRT", "CHR", "BAPM"]), death: deat.or(burial), deat, burial, dead: p.death.is_some() }
}

impl Tree {
    /// Every likely error in the tree, most severe first, then grouped by kind.
    pub fn quality_issues(&self) -> Vec<Issue> {
        let facts: HashMap<&str, Facts> = self.people.iter().map(|(id, p)| (id.as_str(), facts(p))).collect();
        let fact = |id: &str| facts.get(id).copied().unwrap_or_default();
        let mut out = Vec::new();
        let now = today();

        for id in &self.order {
            let p = &self.people[id];
            let f = fact(id);
            let me = || vec![id.clone()];
            if p.given.trim().is_empty() && p.surname.trim().is_empty() {
                out.push(Issue::new(IssueKind::MissingName, Severity::Warning, me()));
            }
            let linked = p.famc.iter().chain(&p.fams).any(|x| self.family(x).is_some());
            if !linked && self.people.len() > 1 {
                out.push(Issue::new(IssueKind::Isolated, Severity::Info, me()));
            }
            if let (Some(b), Some(d)) = (f.birth, f.death) {
                if before(d, b) {
                    out.push(Issue::new(IssueKind::DeathBeforeBirth, Severity::Error, me()).nums(d.year, b.year).approx(b.approx || d.approx));
                } else if let Some(age) = min_days(b, d).map(years).filter(|a| *a > MAX_AGE) {
                    out.push(Issue::new(IssueKind::VeryOld, Severity::Warning, me()).nums(age, 0).approx(b.approx || d.approx));
                }
            }
            if let (Some(d), Some(bu)) = (f.deat, f.burial)
                && before(bu, d)
            {
                out.push(Issue::new(IssueKind::BurialBeforeDeath, Severity::Error, me()).nums(bu.year, d.year).approx(bu.approx || d.approx));
            }
            if !f.dead
                && let Some(b) = f.birth
                && b.hi < OPEN_HI
                && years(now - b.hi) > MAX_AGE
            {
                out.push(Issue::new(IssueKind::PossiblyDeceased, Severity::Info, me()).nums(b.year, 0));
            }
        }

        let mut fam_ids: Vec<&String> = self.families.keys().collect();
        fam_ids.sort_by_key(|id| (id.len(), id.as_str()));
        for fid in fam_ids {
            let fam = &self.families[fid];
            let husband = fam.husband.as_deref().and_then(|x| self.person(x));
            let wife = fam.wife.as_deref().and_then(|x| self.person(x));
            let mut seen = HashSet::new();
            let mut dup_reported = HashSet::new();
            let mut children: Vec<&Person> = Vec::new();
            for c in &fam.children {
                let Some(cp) = self.person(c) else { continue };
                if !seen.insert(c.as_str()) {
                    if dup_reported.insert(c.as_str()) {
                        out.push(Issue::new(IssueKind::DuplicateChild, Severity::Warning, vec![c.clone()]).fam(fid));
                    }
                } else {
                    children.push(cp);
                }
            }
            let mut members: Vec<&str> = husband.iter().chain(wife.iter()).map(|p| p.id.as_str()).collect();
            members.extend(children.iter().map(|p| p.id.as_str()));
            members.sort_unstable();
            members.dedup();
            match members.len() {
                0 => out.push(Issue::new(IssueKind::EmptyFamily, Severity::Warning, vec![]).fam(fid)),
                1 => out.push(Issue::new(IssueKind::SingleMemberFamily, Severity::Info, vec![members[0].to_string()]).fam(fid)),
                _ => {}
            }
            if let Some(h) = husband.filter(|h| h.sex == Sex::Female) {
                out.push(Issue::new(IssueKind::HusbandFemale, Severity::Warning, vec![h.id.clone()]).fam(fid));
            }
            if let Some(w) = wife.filter(|w| w.sex == Sex::Male) {
                out.push(Issue::new(IssueKind::WifeMale, Severity::Warning, vec![w.id.clone()]).fam(fid));
            }

            // parents' ages and deaths around each child's birth
            for c in &children {
                let Some(cb) = fact(&c.id).birth else { continue };
                let roles = [
                    (husband, IssueKind::BornBeforeFather, IssueKind::FatherTooYoung, IssueKind::FatherTooOld, FATHER_MIN, FATHER_MAX),
                    (wife, IssueKind::BornBeforeMother, IssueKind::MotherTooYoung, IssueKind::MotherTooOld, MOTHER_MIN, MOTHER_MAX),
                ];
                for (parent, k_before, k_young, k_old, min, max) in roles {
                    let Some(par) = parent.filter(|p| p.id != c.id) else { continue };
                    let pf = fact(&par.id);
                    let people = || vec![c.id.clone(), par.id.clone()];
                    if let Some(pb) = pf.birth {
                        let approx = pb.approx || cb.approx;
                        if before(cb, pb) {
                            out.push(Issue::new(k_before, Severity::Error, people()).fam(fid).nums(cb.year, pb.year).approx(approx));
                        } else if let Some(age) = max_days(pb, cb).map(years).filter(|a| *a < min) {
                            out.push(Issue::new(k_young, Severity::Warning, people()).fam(fid).nums(age, 0).approx(approx));
                        } else if let Some(age) = min_days(pb, cb).map(years).filter(|a| *a > max) {
                            out.push(Issue::new(k_old, Severity::Warning, people()).fam(fid).nums(age, 0).approx(approx));
                        }
                    }
                    if let Some(pd) = pf.death {
                        let approx = pd.approx || cb.approx;
                        // a father may die during the pregnancy
                        let posthumous = if k_before == IssueKind::BornBeforeFather {
                            min_days(pd, cb).is_some_and(|d| d > 366).then_some(IssueKind::BornAfterFatherDeath)
                        } else {
                            before(pd, cb).then_some(IssueKind::BornAfterMotherDeath)
                        };
                        if let Some(k) = posthumous {
                            out.push(Issue::new(k, Severity::Error, people()).fam(fid).nums(cb.year, pd.year).approx(approx));
                        }
                    }
                }
            }

            // marriage vs. each partner's life
            let marr = fam.events.iter().filter(|e| e.tag == "MARR").find_map(|e| e.date.as_deref().and_then(span));
            for (me, other) in [(husband, wife), (wife, husband)] {
                let Some(me) = me else { continue };
                let mf = fact(&me.id);
                let people = || [Some(me), other].into_iter().flatten().map(|p| p.id.clone()).collect::<Vec<_>>();
                if let Some(m) = marr {
                    if let Some(b) = mf.birth {
                        let approx = m.approx || b.approx;
                        if before(m, b) {
                            out.push(Issue::new(IssueKind::MarriedBeforeBirth, Severity::Error, people()).fam(fid).nums(m.year, b.year).approx(approx));
                        } else if let Some(age) = max_days(b, m).map(years).filter(|a| *a < MARRIAGE_MIN) {
                            out.push(Issue::new(IssueKind::MarriedTooYoung, Severity::Warning, people()).fam(fid).nums(age, 0).other(other).approx(approx));
                        }
                    }
                    if let Some(d) = mf.death
                        && before(d, m)
                    {
                        out.push(Issue::new(IssueKind::MarriedAfterDeath, Severity::Warning, people()).fam(fid).nums(m.year, d.year).other(other).approx(m.approx || d.approx));
                    }
                }
            }
            if let (Some(h), Some(w)) = (husband, wife)
                && let (Some(hb), Some(wb)) = (fact(&h.id).birth, fact(&w.id).birth)
            {
                let gap = min_days(hb, wb).filter(|d| *d > 0).or_else(|| min_days(wb, hb).filter(|d| *d > 0)).map(years);
                if let Some(g) = gap.filter(|g| *g > AGE_GAP) {
                    out.push(Issue::new(IssueKind::LargeAgeGap, Severity::Info, vec![h.id.clone(), w.id.clone()]).fam(fid).nums(g, 0).other(Some(w)));
                }
            }

            // siblings born too close together (only exact days; same day = twins)
            let mut births: Vec<(i64, &Person)> = children.iter().filter_map(|c| fact(&c.id).birth.filter(|b| b.day && !b.approx).map(|b| (b.lo, *c))).collect();
            births.sort_by_key(|(d, _)| *d);
            for pair in births.windows(2) {
                let ((d0, p0), (d1, p1)) = (pair[0], pair[1]);
                let gap = d1 - d0;
                if gap > TWINS_DAYS && gap < SIBLING_GAP_DAYS {
                    out.push(Issue::new(IssueKind::SiblingsTooClose, Severity::Warning, vec![p1.id.clone(), p0.id.clone()]).fam(fid).nums(gap, 0).other(Some(p0)));
                }
            }
        }

        out.extend(self.ancestry_cycles());

        for (a, b, score) in self.duplicate_pairs(DUPLICATE_MIN) {
            let other = self.person(&b);
            out.push(Issue::new(IssueKind::PossibleDuplicate, Severity::Info, vec![a, b.clone()]).nums((score * 100.).round() as i64, 0).other(other));
        }

        out.sort_by_key(|i| (i.severity, i.kind));
        out
    }

    /// Groups of people who are (through parent links) their own ancestors: strongly connected
    /// components of the child → parent graph. Iterative Tarjan, so deep trees can't overflow.
    fn ancestry_cycles(&self) -> Vec<Issue> {
        let ix: HashMap<&str, usize> = self.order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
        let edges: Vec<Vec<usize>> = self
            .order
            .iter()
            .map(|id| {
                let mut v: Vec<usize> = self.people[id].famc.iter().filter_map(|f| self.family(f)).flat_map(|f| f.partners()).filter_map(|p| ix.get(p.as_str()).copied()).collect();
                v.sort_unstable();
                v.dedup();
                v
            })
            .collect();
        let n = edges.len();
        const UNSEEN: usize = usize::MAX;
        let (mut index, mut low, mut on_stack) = (vec![UNSEEN; n], vec![0; n], vec![false; n]);
        let (mut stack, mut next, mut out) = (Vec::new(), 0, Vec::new());
        for root in 0..n {
            if index[root] != UNSEEN {
                continue;
            }
            let mut work: Vec<(usize, usize)> = vec![(root, 0)];
            index[root] = next;
            low[root] = next;
            next += 1;
            stack.push(root);
            on_stack[root] = true;
            while let Some(&mut (v, ref mut ei)) = work.last_mut() {
                if let Some(&w) = edges[v].get(*ei) {
                    *ei += 1;
                    if index[w] == UNSEEN {
                        index[w] = next;
                        low[w] = next;
                        next += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        work.push((w, 0));
                    } else if on_stack[w] {
                        low[v] = low[v].min(index[w]);
                    }
                    continue;
                }
                work.pop();
                if let Some(&(u, _)) = work.last() {
                    low[u] = low[u].min(low[v]);
                }
                if low[v] == index[v] {
                    let mut comp = Vec::new();
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    if comp.len() > 1 || edges[v].contains(&v) {
                        comp.sort_unstable();
                        out.push(Issue::new(IssueKind::OwnAncestor, Severity::Error, comp.into_iter().map(|i| self.order[i].clone()).collect()));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny GEDCOM builder: `p(id, name, sex, birth, death)` and `f(id, husb, wife, children, marr)`.
    struct G(String);

    impl G {
        fn new() -> G {
            G("0 HEAD\n1 CHAR UTF-8\n".into())
        }
        fn p(mut self, id: &str, name: &str, sex: &str, birt: &str, deat: &str) -> G {
            self.0 += &format!("0 @{id}@ INDI\n");
            if !name.is_empty() {
                self.0 += &format!("1 NAME {name}\n");
            }
            self.0 += &format!("1 SEX {sex}\n");
            if !birt.is_empty() {
                self.0 += &format!("1 BIRT\n2 DATE {birt}\n");
            }
            if !deat.is_empty() {
                self.0 += &format!("1 DEAT\n2 DATE {deat}\n");
            }
            self
        }
        fn raw(mut self, text: &str) -> G {
            self.0 += text;
            self
        }
        fn f(mut self, id: &str, husb: &str, wife: &str, kids: &[&str], marr: &str) -> G {
            self.0 += &format!("0 @{id}@ FAM\n");
            if !husb.is_empty() {
                self.0 += &format!("1 HUSB @{husb}@\n");
            }
            if !wife.is_empty() {
                self.0 += &format!("1 WIFE @{wife}@\n");
            }
            for k in kids {
                self.0 += &format!("1 CHIL @{k}@\n");
            }
            if !marr.is_empty() {
                self.0 += &format!("1 MARR\n2 DATE {marr}\n");
            }
            self
        }
        /// Parse and add the FAMS/FAMC back-links the builder doesn't write.
        fn tree(self) -> Tree {
            let mut t = Tree::parse(&format!("{}0 TRLR\n", self.0)).unwrap();
            let fams: Vec<_> = t.families.values().cloned().collect();
            for f in fams {
                for p in f.partners() {
                    t.record_mut(p).unwrap().children.push(crate::node::Node::new("FAMS", format!("@{}@", f.id)));
                }
                for c in &f.children {
                    let rec = t.record_mut(c).unwrap();
                    let v = format!("@{}@", f.id);
                    if !rec.children.iter().any(|n| n.tag == "FAMC" && n.value == v) {
                        rec.children.push(crate::node::Node::new("FAMC", &v));
                    }
                }
            }
            t.reindex();
            t
        }
    }

    /// A consistent family: father 1900, mother 1905, married 1925, kids 1926 and 1930.
    fn family() -> G {
        G::new()
            .p("I1", "Abel /Norte/", "M", "1900", "1970")
            .p("I2", "Berta /Sur/", "F", "1905", "1980")
            .p("I3", "Ciro /Norte/", "M", "3 MAR 1926", "")
            .p("I4", "Dora /Norte/", "F", "5 JUN 1930", "1999")
            .f("F1", "I1", "I2", &["I3", "I4"], "1925")
    }

    fn kinds(t: &Tree) -> Vec<IssueKind> {
        t.quality_issues().into_iter().map(|i| i.kind).collect()
    }

    fn find(t: &Tree, k: IssueKind) -> Issue {
        t.quality_issues().into_iter().find(|i| i.kind == k).unwrap_or_else(|| panic!("no {k:?} in {:?}", kinds(t)))
    }

    #[test]
    fn clean_family_has_no_issues() {
        let t = family().p("I5", "Eva /Oeste/", "F", "1931", "").f("F2", "I3", "I5", &[], "1950").tree();
        assert_eq!(kinds(&t), vec![], "{:?}", t.quality_issues());
    }

    #[test]
    fn date_ranges() {
        let s = span("BEF 1850").unwrap();
        assert_eq!((s.lo, s.hi), (OPEN_LO, days_from_civil(1850, 12, 31)));
        let s = span("BET 1840 AND MAR 1850").unwrap();
        assert_eq!((s.lo, s.hi), (days_from_civil(1840, 1, 1), days_from_civil(1850, 3, 31)));
        assert!(span("ABT 1850").unwrap().approx);
        assert!(span("12 FEB 1850").unwrap().day);
        assert!(span("(en la guerra)").is_none());
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1) - days_from_civil(2000, 2, 28), 2, "leap year");
    }

    #[test]
    fn death_and_burial_order() {
        let t = G::new().p("I1", "Ana /Uno/", "F", "1850", "1840").raw("1 BURI\n2 DATE 1830\n").tree();
        let i = find(&t, IssueKind::DeathBeforeBirth);
        assert_eq!((i.severity, i.a, i.b), (Severity::Error, 1840, 1850));
        assert_eq!(find(&t, IssueKind::BurialBeforeDeath).severity, Severity::Error);
        // same year: not certain
        let t = G::new().p("I1", "Ana /Uno/", "F", "1850", "1850").tree();
        assert!(!kinds(&t).contains(&IssueKind::DeathBeforeBirth));
        // approximate: one step lower
        let t = G::new().p("I1", "Ana /Uno/", "F", "ABT 1850", "1840").tree();
        let i = find(&t, IssueKind::DeathBeforeBirth);
        assert!(i.approximate);
        assert_eq!(i.severity, Severity::Warning);
        // within the ABT slack: nothing
        let t = G::new().p("I1", "Ana /Uno/", "F", "ABT 1850", "1849").tree();
        assert!(!kinds(&t).contains(&IssueKind::DeathBeforeBirth));
        // BEF birth can't be compared from below
        let t = G::new().p("I1", "Ana /Uno/", "F", "BEF 1850", "1840").tree();
        assert!(!kinds(&t).contains(&IssueKind::DeathBeforeBirth));
    }

    #[test]
    fn very_old_and_possibly_deceased() {
        let t = G::new().p("I1", "Ana /Uno/", "F", "1700", "1815").p("I2", "Blas /Uno/", "M", "1800", "").tree();
        assert_eq!(find(&t, IssueKind::VeryOld).a, 114);
        let i = find(&t, IssueKind::PossiblyDeceased);
        assert_eq!((i.people[0].as_str(), i.a, i.severity), ("I2", 1800, Severity::Info));
        let t = G::new().p("I1", "Ana /Uno/", "F", "1800", "").raw("1 BURI\n").tree();
        assert!(!kinds(&t).contains(&IssueKind::PossiblyDeceased), "an undated burial means dead");
    }

    #[test]
    fn child_before_parent_and_parent_ages() {
        let t = G::new()
            .p("I1", "Abel /Norte/", "M", "1900", "")
            .p("I2", "Berta /Sur/", "F", "1862", "")
            .p("I3", "Ciro /Norte/", "M", "1850", "")
            .p("I4", "Dora /Norte/", "F", "1910", "")
            .f("F1", "I1", "I2", &["I3", "I4"], "")
            .tree();
        let i = find(&t, IssueKind::BornBeforeMother);
        assert_eq!((i.people.clone(), i.a, i.b), (vec!["I3".to_string(), "I2".to_string()], 1850, 1862));
        assert_eq!(find(&t, IssueKind::BornBeforeFather).severity, Severity::Error);
        assert_eq!(find(&t, IssueKind::FatherTooYoung).a, 10);
        assert!(!kinds(&t).contains(&IssueKind::MotherTooOld), "47 at most");
    }

    #[test]
    fn mother_too_old_and_father_too_old() {
        let t = G::new()
            .p("I1", "Abel /Norte/", "M", "1800", "")
            .p("I2", "Berta /Sur/", "F", "1830", "")
            .p("I3", "Ciro /Norte/", "M", "1890", "")
            .f("F1", "I1", "I2", &["I3"], "")
            .tree();
        assert_eq!(find(&t, IssueKind::MotherTooOld).a, 59);
        assert_eq!(find(&t, IssueKind::FatherTooOld).a, 89);
    }

    #[test]
    fn born_after_parent_death() {
        let t = G::new()
            .p("I1", "Abel /Norte/", "M", "1850", "1880")
            .p("I2", "Berta /Sur/", "F", "1855", "1890")
            .p("I3", "Ciro /Norte/", "M", "1881", "")
            .p("I4", "Dora /Norte/", "F", "1895", "")
            .f("F1", "I1", "I2", &["I3", "I4"], "")
            .tree();
        let after_father: Vec<_> = t.quality_issues().into_iter().filter(|i| i.kind == IssueKind::BornAfterFatherDeath).map(|i| i.people[0].clone()).collect();
        // 1881 may be within a year of a 1880 death; 1895 is not
        assert_eq!(after_father, vec!["I4"]);
        assert_eq!(find(&t, IssueKind::BornAfterMotherDeath).people[0], "I4");
    }

    #[test]
    fn marriage_checks_and_age_gap() {
        let t = G::new()
            .p("I1", "Abel /Norte/", "M", "1850", "1870")
            .p("I2", "Berta /Sur/", "F", "1890", "")
            .f("F1", "I1", "I2", &[], "1899")
            .tree();
        let i = find(&t, IssueKind::MarriedAfterDeath);
        assert_eq!((i.people[0].as_str(), i.a, i.b, i.other.as_deref()), ("I1", 1899, 1870, Some("Berta Sur")));
        assert_eq!(find(&t, IssueKind::MarriedTooYoung).people[0], "I2");
        assert_eq!(find(&t, IssueKind::LargeAgeGap).a, 39);
        let t = G::new().p("I1", "Abel /Norte/", "M", "1850", "").p("I2", "Berta /Sur/", "F", "1852", "").f("F1", "I1", "I2", &[], "1840").tree();
        assert_eq!(find(&t, IssueKind::MarriedBeforeBirth).severity, Severity::Error);
    }

    #[test]
    fn siblings_too_close_but_twins_ok() {
        let t = G::new()
            .p("I1", "Abel /Norte/", "M", "1900", "")
            .p("I2", "Ciro /Norte/", "M", "3 MAR 1926", "")
            .p("I3", "Dora /Norte/", "F", "3 MAR 1926", "")
            .p("I4", "Elio /Norte/", "M", "1 SEP 1926", "")
            .p("I5", "Fina /Norte/", "F", "1927", "")
            .f("F1", "I1", "", &["I2", "I3", "I4", "I5"], "")
            .tree();
        let close: Vec<_> = t.quality_issues().into_iter().filter(|i| i.kind == IssueKind::SiblingsTooClose).collect();
        assert_eq!(close.len(), 1, "twins and year-only dates are fine: {close:?}");
        assert_eq!(close[0].people[0], "I4");
        assert_eq!(close[0].a, 182);
    }

    #[test]
    fn link_problems() {
        let t = G::new()
            .p("I1", "Abel /Norte/", "F", "", "")
            .p("I2", "Berta /Sur/", "M", "", "")
            .p("I3", "", "U", "", "")
            .p("I4", "Ciro /Norte/", "M", "", "")
            .p("I5", "Dora /Este/", "F", "", "")
            .f("F1", "I1", "I2", &["I3", "I3"], "")
            .f("F2", "", "", &[], "")
            .f("F3", "I4", "", &[], "")
            .tree();
        let k = kinds(&t);
        for want in [IssueKind::HusbandFemale, IssueKind::WifeMale, IssueKind::DuplicateChild, IssueKind::EmptyFamily, IssueKind::SingleMemberFamily, IssueKind::MissingName, IssueKind::Isolated] {
            assert!(k.contains(&want), "{want:?} missing from {k:?}");
        }
        assert_eq!(find(&t, IssueKind::Isolated).people, vec!["I5"]);
        assert_eq!(find(&t, IssueKind::SingleMemberFamily).people, vec!["I4"]);
        assert_eq!(k.iter().filter(|k| **k == IssueKind::DuplicateChild).count(), 1);
        assert!(find(&t, IssueKind::DuplicateChild).message().contains("F1"));
    }

    #[test]
    fn own_ancestor_cycle_is_found_without_hanging() {
        // I1 → parent I2 → parent I3 → parent I1
        let t = G::new()
            .p("I1", "Abel /Norte/", "M", "", "")
            .p("I2", "Berta /Sur/", "F", "", "")
            .p("I3", "Ciro /Norte/", "M", "", "")
            .p("I4", "Dora /Este/", "F", "", "")
            .f("F1", "", "I2", &["I1"], "")
            .f("F2", "I3", "", &["I2"], "")
            .f("F3", "I1", "I4", &["I3"], "")
            .f("F4", "I4", "", &["I4"], "")
            .tree();
        let cycles: Vec<_> = t.quality_issues().into_iter().filter(|i| i.kind == IssueKind::OwnAncestor).collect();
        assert_eq!(cycles.len(), 2, "{cycles:?}");
        assert!(cycles.iter().any(|c| c.people.len() == 3));
        assert!(cycles.iter().any(|c| c.people == vec!["I4"]), "own parent");
        assert_eq!(cycles[0].severity, Severity::Error);
    }

    #[test]
    fn possible_duplicates_once_per_pair() {
        let t = G::new()
            .p("I1", "Tomás /Quiroga/", "M", "1801", "1870")
            .p("I2", "Tomas /Quiroga/", "M", "1801", "1870")
            .p("I3", "Tomás /Quiroga/", "U", "1850", "")
            .p("I4", "Lucía /Pardo/", "F", "1801", "")
            .tree();
        let d: Vec<_> = t.quality_issues().into_iter().filter(|i| i.kind == IssueKind::PossibleDuplicate).collect();
        assert_eq!(d.len(), 1, "{d:?}");
        let mut pair = d[0].people.clone();
        pair.sort();
        assert_eq!(pair, vec!["I1", "I2"]);
        assert_eq!(d[0].severity, Severity::Info);
    }

    #[test]
    fn sorted_by_severity_and_messages_localized() {
        let _l = crate::locale_lock();
        let t = G::new()
            .p("I1", "Abel /Norte/", "M", "1900", "")
            .p("I2", "Berta /Sur/", "F", "1862", "")
            .p("I3", "Ciro /Norte/", "M", "1850", "")
            .p("I4", "", "U", "", "")
            .f("F1", "I1", "I2", &["I3"], "")
            .tree();
        let all = t.quality_issues();
        assert!(all.windows(2).all(|w| w[0].severity <= w[1].severity));
        let prev = i18n::locale();
        i18n::set_locale("es");
        let m = all.iter().find(|i| i.kind == IssueKind::BornBeforeMother).unwrap().message();
        assert_eq!(m, "Nació en 1850, antes que su madre (1862)");
        for i in &all {
            assert!(!i.message().contains("core.quality"), "{:?}", i.kind);
            assert!(!i.kind.label().contains("core.quality"));
        }
        i18n::set_locale(prev);
        assert!(Severity::ALL.iter().all(|s| !s.label().contains("core.quality")));
    }

    #[test]
    fn fast_on_thousands_of_people() {
        // 4 000 people: couples with four children each, chained over generations
        let mut g = G::new();
        let given = ["Abel", "Berta", "Ciro", "Dora", "Elio", "Fina", "Gil", "Hebe", "Iris", "Jon", "Lino", "Mara"];
        let sur = ["Norte", "Sur", "Este", "Oeste", "Alto", "Bajo", "Llano", "Monte", "Valle", "Rio", "Lago", "Mar", "Sol", "Luna", "Roca", "Prado"];
        let n = 4000;
        for i in 0..n {
            let year = 1600 + (i / 40) as i32;
            let name = format!("{} /{} {}/", given[i % given.len()], sur[(i / 7) % sur.len()], sur[(i / 3) % sur.len()]);
            let birt = if i % 5 == 0 { String::new() } else { format!("{} {} {year}", 1 + i % 28, ["JAN", "APR", "JUL", "OCT"][i % 4]) };
            g = g.p(&format!("I{i}"), &name, if i % 2 == 0 { "M" } else { "F" }, &birt, "");
        }
        for f in 0..n / 6 {
            let kids: Vec<String> = (0..4).map(|k| format!("I{}", 2 * (n / 6) + 4 * f + k)).filter(|k| k[1..].parse::<usize>().unwrap() < n).collect();
            let kids: Vec<&str> = kids.iter().map(String::as_str).collect();
            g = g.f(&format!("F{f}"), &format!("I{}", 2 * f), &format!("I{}", 2 * f + 1), &kids, "");
        }
        let t = g.tree();
        let start = std::time::Instant::now();
        let issues = t.quality_issues();
        let took = start.elapsed();
        assert!(!issues.is_empty());
        assert!(took < std::time::Duration::from_millis(1000), "took {took:?}");
    }
}
