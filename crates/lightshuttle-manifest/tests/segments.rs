//! `segments` tests: the literal/reference decomposition that
//! `Interpolator::resolve` and `Interpolator::scan` are rebuilt on top of.
//!
//! Error cases mirror the ones in `tests/interpolation.rs` verbatim: the
//! grammar `segments` parses is the same one `Interpolator::resolve`
//! rejects on.

use std::fmt::Write as _;

use lightshuttle_manifest::{
    InterpolationContext, Interpolator, ManifestError, Reference, Segment, segments,
};

#[test]
fn purely_literal_string_is_a_single_literal_segment() {
    let out = segments("hello world").unwrap();
    assert_eq!(out, vec![Segment::Literal("hello world".to_owned())]);
}

#[test]
fn lone_dollar_sign_stays_literal() {
    let out = segments("costs $5").unwrap();
    assert_eq!(out, vec![Segment::Literal("costs $5".to_owned())]);
}

#[test]
fn escape_form_unfolds_to_a_single_literal() {
    let out = segments("${{ not.a.reference }}").unwrap();
    assert_eq!(
        out,
        vec![Segment::Literal("${ not.a.reference }".to_owned())]
    );
}

#[test]
fn env_reference_without_default() {
    let out = segments("${env.TAG}").unwrap();
    assert_eq!(
        out,
        vec![Segment::Env {
            name: "TAG".to_owned(),
            default: None,
        }]
    );
}

#[test]
fn env_reference_with_literal_default() {
    let out = segments("${env.TAG:-1.0}").unwrap();
    assert_eq!(
        out,
        vec![Segment::Env {
            name: "TAG".to_owned(),
            default: Some(vec![Segment::Literal("1.0".to_owned())]),
        }]
    );
}

#[test]
fn env_reference_with_nested_env_default() {
    let out = segments("${env.A:-${env.B}}").unwrap();
    assert_eq!(
        out,
        vec![Segment::Env {
            name: "A".to_owned(),
            default: Some(vec![Segment::Env {
                name: "B".to_owned(),
                default: None,
            }]),
        }]
    );
}

#[test]
fn resource_reference() {
    let out = segments("${resources.main_db.host}").unwrap();
    assert_eq!(
        out,
        vec![Segment::Resource {
            name: "main_db".to_owned(),
            property: "host".to_owned(),
        }]
    );
}

#[test]
fn mixed_literal_and_reference_segments_preserve_order() {
    let out = segments("prefix-${env.NAME}-middle-${resources.db.host}-suffix").unwrap();
    assert_eq!(
        out,
        vec![
            Segment::Literal("prefix-".to_owned()),
            Segment::Env {
                name: "NAME".to_owned(),
                default: None,
            },
            Segment::Literal("-middle-".to_owned()),
            Segment::Resource {
                name: "db".to_owned(),
                property: "host".to_owned(),
            },
            Segment::Literal("-suffix".to_owned()),
        ]
    );
}

#[test]
fn rejects_unterminated_interpolation() {
    let err = segments("${env.X").unwrap_err();
    assert!(
        matches!(err, ManifestError::InvalidInterpolation(_)),
        "got: {err:?}"
    );
}

#[test]
fn rejects_unknown_reference_scheme() {
    let err = segments("${secret.X}").unwrap_err();
    assert!(
        matches!(err, ManifestError::InvalidInterpolation(_)),
        "got: {err:?}"
    );
}

#[test]
fn rejects_nested_interpolation_outside_env_default() {
    let err = segments("${resources.${env.X}.host}")
        .expect_err("nested interpolation in a resource name is invalid");
    assert!(
        matches!(err, ManifestError::InvalidInterpolation(_)),
        "got: {err:?}"
    );
}

#[test]
fn rejects_interpolation_deeper_than_limit() {
    let mut input = String::new();
    for i in 0..11 {
        write!(input, "${{env.A{i}:-").unwrap();
    }
    input.push('x');
    for _ in 0..11 {
        input.push('}');
    }

    let err = segments(&input).expect_err("11 levels should exceed the cap");
    assert!(
        matches!(err, ManifestError::InterpolationTooDeep { limit: 10, .. }),
        "got: {err:?}"
    );
}

/// Guards the single-grammar invariant: `segments` and `scan` must agree on
/// which references a string holds, in the same order. If the two ever drift,
/// the export renders placeholders the planner never validated.
#[test]
fn segments_and_scan_report_the_same_references_in_the_same_order() {
    let input = "prefix-${env.NAME}-${resources.db.host}-${env.TAG:-${env.FALLBACK}}";

    let from_segments = collect_references(&segments(input).expect("segments parses"));

    let context = InterpolationContext::new();
    let from_scan = Interpolator::new(&context)
        .scan(input)
        .expect("scan parses")
        .into_iter()
        .map(|reference| match reference {
            Reference::Env { name, .. } => format!("env.{name}"),
            Reference::Resource { name, property } => format!("resources.{name}.{property}"),
        })
        .collect::<Vec<_>>();

    assert_eq!(from_segments, from_scan);
}

fn collect_references(parsed: &[Segment]) -> Vec<String> {
    let mut out = Vec::new();
    for segment in parsed {
        match segment {
            Segment::Literal(_) => {}
            Segment::Env { name, default } => {
                out.push(format!("env.{name}"));
                if let Some(default) = default {
                    out.extend(collect_references(default));
                }
            }
            Segment::Resource { name, property } => {
                out.push(format!("resources.{name}.{property}"));
            }
        }
    }
    out
}
