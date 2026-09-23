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

const LITERALS: &[&str] = &["a", " ", "{{", "}}", "}", "{", "=", ",", "中"];
const POSITIONS: &[&str] = &[
    "", "0", "1", "2", "name", "type", "é", "_", "_x", "r#type", " ", "0 ", "name ", " name",
    "65536", "a.b", "1a",
];
const ALIGNS: &[&str] = &["", "<", ">", "^", "*<", "}>", "{^", "0>", " >"];
const FLAGS: &[&str] = &["", "+", "-", "#", "+#", "0", "-#0", "#+"];
const WIDTHS: &[&str] = &[
    "", "5", "0$", "1$", "name$", "w$", "_$", "_", "x$", "a", "65536", "3 ",
];
const PRECISIONS: &[&str] = &["", ".", ".*", ".3", ".1$", ".name$", ".*.*", "._", ". 2"];
const TYPES: &[&str] = &[
    "", "?", "x?", "X?", "x", "X", "e", "?x", "?#", "?X", "#?", "abc", "_", "é", "? ",
];
const CLOSES: &[&str] = &["}", "}", "}", " }", "", "}}", "x}", "\u{3000}}"];

/// One piece of a generated format string: a literal run or a placeholder
/// assembled from grammar fragments, with `junk` spliced in where a byte asks.
fn render_piece(picks: [u8; 8], junk: char) -> String {
    let pick = |table: &[&str], i: usize| -> String {
        let n = usize::from(picks[i]);
        table
            .get(n % (table.len() + 1))
            .map_or_else(|| junk.to_string(), |&f| f.to_owned())
    };
    if picks[0].is_multiple_of(4) {
        return pick(LITERALS, 1);
    }
    let mut out = String::from("{");
    out.push_str(&pick(POSITIONS, 1));
    if picks[0] % 4 != 1 {
        out.push(':');
        out.push_str(&pick(ALIGNS, 2));
        out.push_str(&pick(FLAGS, 3));
        out.push_str(&pick(WIDTHS, 4));
        out.push_str(&pick(PRECISIONS, 5));
        out.push_str(&pick(TYPES, 6));
    }
    out.push_str(&pick(CLOSES, 7));
    out
}

#[test]
fn scanner_agrees_with_rustc_parser() {
    use bolero::generator::TypeGenerator as _;
    bolero::check!()
        .with_generator(<Vec<([u8; 8], char)>>::produce().with().len(0..=6usize))
        .with_iterations(50_000)
        .for_each(|pieces| {
            let s: String = pieces
                .iter()
                .map(|&(picks, junk)| render_piece(picks, junk))
                .collect();
            assert_agrees(&s);
        });
}
