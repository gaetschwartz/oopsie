use std::collections::BTreeSet;

use ra_ap_rustc_parse_format::{Count, ParseMode, Parser, Piece, Position};

use super::*;

#[derive(Debug, PartialEq, Eq)]
struct Summary {
    positional: usize,
    names: BTreeSet<String>,
}

fn ours(s: &str) -> Option<Summary> {
    format_arg_usage(s).ok().map(|u| Summary {
        positional: u.positional,
        names: u.names.iter().map(|&n| n.to_owned()).collect(),
    })
}

fn rustc(s: &str) -> Option<Summary> {
    let mut parser = Parser::new(s, None, None, false, ParseMode::Format);
    let pieces: Vec<Piece<'_>> = parser.by_ref().collect();
    if !parser.errors.is_empty() {
        return None;
    }
    let mut summary = Summary {
        positional: 0,
        names: BTreeSet::new(),
    };
    let count = |c: &Count<'_>, summary: &mut Summary| match *c {
        Count::CountIsName(name, _) => {
            summary.names.insert(name.to_owned());
        }
        Count::CountIsParam(i) | Count::CountIsStar(i) => {
            summary.positional = summary.positional.max(i + 1);
        }
        Count::CountIs(_) | Count::CountImplied => {}
    };
    for piece in &pieces {
        let Piece::NextArgument(arg) = piece else {
            continue;
        };
        match arg.position {
            Position::ArgumentImplicitlyIs(i) | Position::ArgumentIs(i) => {
                summary.positional = summary.positional.max(i + 1);
            }
            Position::ArgumentNamed(name) => {
                summary.names.insert(name.to_owned());
            }
        }
        count(&arg.format.width, &mut summary);
        count(&arg.format.precision, &mut summary);
    }
    Some(summary)
}

#[track_caller]
fn assert_agrees(s: &str) {
    assert_eq!(ours(s), rustc(s), "format string {s:?}");
}

type Expected = Option<(usize, &'static [&'static str])>;

#[test]
fn table_matches_expected_usage() {
    let cases: &[(&str, Expected)] = &[
        ("", Some((0, &[]))),
        ("plain", Some((0, &[]))),
        ("{{}}", Some((0, &[]))),
        ("{{{}}}", Some((1, &[]))),
        ("{} {}", Some((2, &[]))),
        ("{1} {}", Some((2, &[]))),
        ("{0} {0}", Some((1, &[]))),
        ("{name} {name:?}", Some((0, &["name"]))),
        ("{:>width$}", Some((1, &["width"]))),
        ("{:.prec$}", Some((1, &["prec"]))),
        ("{:1$}", Some((2, &[]))),
        ("{:.3$}", Some((4, &[]))),
        ("{:.*}", Some((2, &[]))),
        ("{0:.*}", Some((1, &[]))),
        ("{:0$}", Some((1, &[]))),
        ("{:08.3}", Some((1, &[]))),
        ("{:*^+#012.5x?}", Some((1, &[]))),
        ("{:}>5}", Some((1, &[]))),
        ("{:x?} {:X?} {:#?}", Some((3, &[]))),
        ("{:e}", Some((1, &[]))),
        ("{ }", Some((1, &[]))),
        ("{name }", Some((0, &["name"]))),
        ("{name :?}", Some((0, &["name"]))),
        ("{type}", Some((0, &["type"]))),
        ("{é}", Some((0, &["é"]))),
        ("{_x}", Some((0, &["_x"]))),
        ("{:.}", Some((1, &[]))),
        ("{65535}", Some((65536, &[]))),
        ("{", None),
        ("}", None),
        ("a } b", None),
        ("{ name}", None),
        ("{r#type}", None),
        ("{_}", None),
        ("{:_}", None),
        ("{:?x}", None),
        ("{:?#}", None),
        ("{name.field}", None),
        ("{0a}", None),
        ("{65536}", None),
        ("{:.5$x}", Some((6, &[]))),
        ("{name?}", None),
    ];
    for &(s, expected) in cases {
        let expected = expected.map(|(positional, names)| Summary {
            positional,
            names: names.iter().map(|&n| n.to_owned()).collect(),
        });
        assert_eq!(ours(s), expected, "format string {s:?}");
        assert_agrees(s);
    }
}

#[test]
fn unmatched_close_is_distinguished() {
    assert_eq!(
        format_arg_usage("ok }} then } here"),
        Err(FormatStrError::UnmatchedClose { at: 11 })
    );
    assert_eq!(
        format_arg_usage("{"),
        Err(FormatStrError::Malformed { at: 1 })
    );
}

/// Grammar fragments for one placeholder slot: `valid` ones rustc accepts on
/// their own, `invalid` ones that exercise its error paths.
struct Fragments {
    valid: &'static [&'static str],
    invalid: &'static [&'static str],
}

const LITERALS: Fragments = Fragments {
    valid: &["a", " ", "{{", "}}", "=", ",", "中"],
    invalid: &["}", "{"],
};
const POSITIONS: Fragments = Fragments {
    valid: &[
        "", "0", "1", "2", "10", "12", "name", "type", "é", "_x", "ñame", "x1",
    ],
    invalid: &[
        "_", "r#type", " ", "0 ", "name ", " name", "65536", "a.b", "1a",
    ],
};
const ALIGNS: Fragments = Fragments {
    valid: &["", "<", ">", "^", "*<", "}>", "{^", "0>", " >"],
    invalid: &[],
};
const FLAGS: Fragments = Fragments {
    valid: &["", "+", "-", "#", "+#", "0", "-#0"],
    invalid: &[],
};
const WIDTHS: Fragments = Fragments {
    valid: &[
        "", "5", "0$", "1$", "12$", "name$", "w$", "é$", "ñame$", "_x$", "x$",
    ],
    invalid: &["_$", "_", "a", "65536", "3 ", "r#type$", "r#w$"],
};
const PRECISIONS: Fragments = Fragments {
    valid: &["", ".", ".*", ".3", ".1$", ".11$", ".name$", ".é$"],
    invalid: &[".*.*", "._", ". 2", ".r#type$"],
};
const TYPES: Fragments = Fragments {
    valid: &["", "?", "x?", "X?", "x", "X", "e", "E", "o", "b", "p"],
    invalid: &["?x", "?#", "?X", "#?", "abc", "_", "é", "? "],
};
const CLOSES: Fragments = Fragments {
    valid: &["}", " }"],
    invalid: &["", "}}", "x}", "\u{3000}}"],
};

impl Fragments {
    /// The fragment `pick` selects: always a valid one outside chaos mode;
    /// in chaos mode any fragment, or `junk`.
    fn pick(&self, pick: u8, chaos: bool, junk: char) -> String {
        let pick = usize::from(pick);
        if !chaos {
            return self.valid[pick % self.valid.len()].to_owned();
        }
        let n = self.valid.len() + self.invalid.len();
        match pick % (n + 1) {
            i if i < self.valid.len() => self.valid[i].to_owned(),
            i if i < n => self.invalid[i - self.valid.len()].to_owned(),
            _ => junk.to_string(),
        }
    }
}

/// A generated format string: one piece per `(picks, junk)`. One in eight is
/// built in chaos mode, where any fragment may be invalid or `junk`; the rest
/// draw only fragments rustc accepts, so they mostly consume arguments.
fn render_format_string(mode: u8, pieces: &[([u8; 9], char)]) -> String {
    let chaos = mode.is_multiple_of(8);
    let mut out = String::new();
    for &(picks, junk) in pieces {
        if picks[0].is_multiple_of(4) {
            out.push_str(&LITERALS.pick(picks[2], chaos, junk));
            continue;
        }
        out.push('{');
        out.push_str(&POSITIONS.pick(picks[2], chaos, junk));
        if !picks[1].is_multiple_of(3) {
            out.push(':');
            let spec = [&ALIGNS, &FLAGS, &WIDTHS, &PRECISIONS, &TYPES];
            for (fragments, &pick) in spec.into_iter().zip(&picks[3..8]) {
                out.push_str(&fragments.pick(pick, chaos, junk));
            }
        }
        out.push_str(&CLOSES.pick(picks[8], chaos, junk));
    }
    out
}

/// Randomly seeded; a failure prints `BOLERO_RANDOM_SEED` to replay it.
#[test]
fn scanner_agrees_with_rustc_parser() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bolero::generator::TypeGenerator as _;

    let total = AtomicUsize::new(0);
    let consuming = AtomicUsize::new(0);
    bolero::check!()
        .with_generator((
            u8::produce(),
            <Vec<([u8; 9], char)>>::produce().with().len(1..=6usize),
        ))
        .with_iterations(50_000)
        .for_each(|(mode, pieces)| {
            let s = render_format_string(*mode, pieces);
            let expected = rustc(&s);
            assert_eq!(ours(&s), expected, "format string {s:?}");
            total.fetch_add(1, Ordering::Relaxed);
            if expected.is_some_and(|u| u.positional > 0 || !u.names.is_empty()) {
                consuming.fetch_add(1, Ordering::Relaxed);
            }
        });
    let (total, consuming) = (total.into_inner(), consuming.into_inner());
    assert!(
        consuming * 10 > total * 7,
        "only {consuming} of {total} generated strings are valid and consume arguments"
    );
}
