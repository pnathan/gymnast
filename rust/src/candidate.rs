//! The candidate protocol and the validation firewall
//! (`docs/rust-port-plan-phase4.md`, section B; ports `src/candidate.lisp`
//! for behavioral intent, against the Rust IR/plan contract). A model can
//! propose data; it cannot mutate a plan node or decide whether its own
//! output is acceptable. Candidates are UNTRUSTED text: every accessor and
//! `candidate_diagnostics` itself must be total (no panics, no unbounded
//! recursion) over arbitrary `Sexpr` input.

use crate::diag::diag_sexpr;
use crate::plan::{target_language, PlanNode};
use crate::sexpr::Sexpr;

/// Content substrings that mark a file as Lisp-shaped, checked by E507
/// (target-language-violation) — transcribed verbatim from
/// `src/candidate.lisp`'s `gymnast-candidate-diagnostics`.
const LISP_MARKERS: &[&str] = &[
    "(defun ",
    "(defvar ",
    "(defmacro ",
    "(define ",
    "(lambda ",
    "(module ",
    "(setq ",
    "(let* ",
];

/// A parsed, UNTRUSTED model candidate. Field access is total: missing or
/// malformed fields read as empty rather than panicking, both because
/// `sexpr` is a public field a caller could hand-construct outside
/// `from_sexpr`'s validation, and because the value itself came from an
/// untrusted model.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub sexpr: Sexpr,
}

impl Candidate {
    /// `Some(_)` iff `v` is exactly a `(candidate (...))` tagged alist:
    /// a two-element list headed by the bare symbol `candidate`, whose
    /// second element is itself a list (the field-pairs body) — the
    /// nested-alist convention `docs/ir-contract-deltas.md` uses
    /// throughout the Rust IR ("every alist is one nested list"), the
    /// same shape `prompt.rs`'s `OUTPUT PROTOCOL` projection already
    /// uses. Anything else (wrong tag, wrong arity, non-list body, or a
    /// bare symbol/string/int) is `None`.
    pub fn from_sexpr(v: Sexpr) -> Option<Candidate> {
        let items = v.as_list()?;
        if items.len() != 2 {
            return None;
        }
        if items[0].as_sym() != Some("candidate") {
            return None;
        }
        items[1].as_list()?;
        Some(Candidate { sexpr: v })
    }

    /// The field-pairs body (the second element of the tagged list), if
    /// `sexpr` has that shape. `None` for a malformed/hand-built value —
    /// every accessor below treats that as "no fields", never a panic.
    fn body(&self) -> Option<&Sexpr> {
        self.sexpr.as_list().and_then(|items| items.get(1))
    }

    fn field(&self, key: &str) -> Option<&Sexpr> {
        self.body()?.assoc(key)
    }

    pub fn node_id(&self) -> Option<&str> {
        self.field("node-id").and_then(|v| v.as_str())
    }

    /// `(path, content)` pairs from the `files` field, in list order.
    /// Entries that are not a two-element `(string string)` pair are
    /// skipped rather than treated as an error.
    pub fn files(&self) -> Vec<(String, String)> {
        self.field("files")
            .and_then(|v| v.as_list())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        let pair = entry.as_list()?;
                        if pair.len() != 2 {
                            return None;
                        }
                        let path = pair[0].as_str()?;
                        let content = pair[1].as_str()?;
                        Some((path.to_string(), content.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn implements(&self) -> Vec<String> {
        self.string_list("implements")
    }

    pub fn edge_uses(&self) -> Vec<String> {
        self.string_list("edge-uses")
    }

    /// A vocabulary-term list field: entries may print as either a
    /// quoted string (e.g. `implements`, which carries IR node ids) or a
    /// bare symbol (e.g. `edge-uses`, which carries capability names) —
    /// both are accepted so the accessor stays total over either
    /// encoding a candidate might use.
    fn string_list(&self, key: &str) -> Vec<String> {
        self.field(key)
            .and_then(|v| v.as_list())
            .map(|items| items.iter().filter_map(sexpr_as_vocab_string).collect())
            .unwrap_or_default()
    }

    /// `true` when `assumptions` is absent or nil (the empty list) — the
    /// "no added assumptions" condition E505 checks the negation of.
    pub fn assumptions_empty(&self) -> bool {
        is_nil_or_absent(self.field("assumptions"))
    }

    /// `true` when `unresolved` is absent or nil — the "no unresolved
    /// contract" condition E506 checks the negation of.
    pub fn unresolved_empty(&self) -> bool {
        is_nil_or_absent(self.field("unresolved"))
    }
}

fn sexpr_as_vocab_string(s: &Sexpr) -> Option<String> {
    s.as_str()
        .map(String::from)
        .or_else(|| s.as_sym().map(String::from))
}

fn is_nil_or_absent(v: Option<&Sexpr>) -> bool {
    match v {
        None => true,
        Some(Sexpr::List(items)) => items.is_empty(),
        Some(_) => false,
    }
}

fn contains_lisp_marker(content: &str) -> bool {
    LISP_MARKERS.iter().any(|m| content.contains(m))
}

/// The firewall: every check from `src/candidate.lisp`, as diagnostics
/// against `node`'s contract. A model can propose data; it cannot mutate
/// a plan node or decide whether its own output is acceptable — this
/// function is the sole authority on whether a candidate is acceptable,
/// and it never trusts anything the candidate itself claims.
///
/// Checks run in table order (`docs/rust-port-plan-phase4.md`, section
/// B): E501 is exclusive — when the value is not a `(candidate (...))`
/// tagged alist, no further check runs and exactly one diagnostic (E501)
/// is returned. Otherwise E502 through E508 all run and every applicable
/// one fires (never short-circuiting each other).
pub fn candidate_diagnostics(node: &PlanNode, candidate: &Sexpr) -> Vec<Sexpr> {
    let c = match Candidate::from_sexpr(candidate.clone()) {
        Some(c) => c,
        None => {
            return vec![diag_sexpr(
                "error",
                "E501",
                (0, 0),
                "model output is not a candidate value".to_string(),
            )];
        }
    };

    let mut diags = Vec::new();

    // E502 candidate-node-mismatch.
    if c.node_id() != Some(node.id.as_str()) {
        diags.push(diag_sexpr(
            "error",
            "E502",
            (0, 0),
            format!(
                "candidate names a different plan node: expected {}, got {}",
                node.id,
                c.node_id().unwrap_or("<none>")
            ),
        ));
    }

    let files = c.files();

    // E503 unauthorized-output-path: one per candidate file path not in
    // `node.may_write`.
    for (path, _) in &files {
        if !node.may_write.iter().any(|allowed| allowed == path) {
            diags.push(diag_sexpr(
                "error",
                "E503",
                (0, 0),
                format!("candidate writes outside its node contract: {}", path),
            ));
        }
    }

    // E504 missing-output-file: one per required `node.may_write` path
    // absent from the candidate's files.
    for allowed in &node.may_write {
        if !files.iter().any(|(path, _)| path == allowed) {
            diags.push(diag_sexpr(
                "error",
                "E504",
                (0, 0),
                format!("candidate omitted a required artifact: {}", allowed),
            ));
        }
    }

    // E505 candidate-added-assumptions.
    if !c.assumptions_empty() {
        diags.push(diag_sexpr(
            "error",
            "E505",
            (0, 0),
            "candidate may not add assumptions".to_string(),
        ));
    }

    // E506 candidate-unresolved.
    if !c.unresolved_empty() {
        diags.push(diag_sexpr(
            "error",
            "E506",
            (0, 0),
            "candidate reported an unresolved contract".to_string(),
        ));
    }

    // E507 target-language-violation: non-lamedh/lisp/scheme target AND
    // a file's content looks like Lisp. One per offending file.
    let lang = target_language(&node.target);
    let non_lisp_target = match lang.as_deref() {
        Some("lamedh") | Some("lisp") | Some("scheme") => false,
        Some(_) => true,
        None => false,
    };
    if non_lisp_target {
        let lang_name = lang.as_deref().unwrap_or("the target language");
        for (path, content) in &files {
            if contains_lisp_marker(content) {
                diags.push(diag_sexpr(
                    "error",
                    "E507",
                    (0, 0),
                    format!(
                        "file content appears to be Lisp, not {} as required by TARGET: {}",
                        lang_name, path
                    ),
                ));
            }
        }
    }

    // E508 undeclared-edge-use: one per `edge-uses` entry not in
    // `node.capabilities`.
    for edge in c.edge_uses() {
        if !node.capabilities.iter().any(|cap| cap == &edge) {
            diags.push(diag_sexpr(
                "error",
                "E508",
                (0, 0),
                format!(
                    "candidate uses capability edge not declared in node contract: {}",
                    edge
                ),
            ));
        }
    }

    diags
}

/// `true` iff `candidate_diagnostics` reports no error-severity
/// diagnostic (mirrors `gymnast-candidate-valid-p`, which checks
/// `gymnast-has-errors-p` rather than mere emptiness — every code this
/// firewall emits is error-severity today, but validity is defined as
/// "no errors", not "no diagnostics at all", so a future warning-level
/// candidate diagnostic would not flip this).
pub fn candidate_valid(node: &PlanNode, candidate: &Sexpr) -> bool {
    !candidate_diagnostics(node, candidate).iter().any(|d| {
        d.assoc("severity")
            .and_then(|s| s.as_sym())
            .map(|s| s == "error")
            .unwrap_or(true)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sexpr::Sexpr;

    fn plain_node() -> PlanNode {
        PlanNode::new(
            "m/plan/x".to_string(),
            "generative",
            "recipe-v1",
            vec![],
            vec![],
            Sexpr::list(vec![Sexpr::sym("ruby"), Sexpr::sym("rails")]),
            Sexpr::sym("none"),
            vec!["out/a.rb".to_string()],
            vec!["clock".to_string()],
            vec![],
            vec![],
        )
    }

    fn nil() -> Sexpr {
        Sexpr::list(vec![])
    }

    fn candidate(node_id: &str, files: &[(&str, &str)]) -> Sexpr {
        Sexpr::list(vec![
            Sexpr::sym("candidate"),
            Sexpr::list(vec![
                Sexpr::pair("node-id", Sexpr::Str(node_id.to_string())),
                Sexpr::pair(
                    "files",
                    Sexpr::list(
                        files
                            .iter()
                            .map(|(p, c)| {
                                Sexpr::list(vec![
                                    Sexpr::Str(p.to_string()),
                                    Sexpr::Str(c.to_string()),
                                ])
                            })
                            .collect(),
                    ),
                ),
                Sexpr::pair("assumptions", nil()),
                Sexpr::pair("unresolved", nil()),
            ]),
        ])
    }

    #[test]
    fn from_sexpr_none_for_non_list() {
        assert!(Candidate::from_sexpr(Sexpr::Int(1)).is_none());
        assert!(Candidate::from_sexpr(Sexpr::Str("x".to_string())).is_none());
    }

    #[test]
    fn from_sexpr_none_for_wrong_arity() {
        assert!(
            Candidate::from_sexpr(Sexpr::list(vec![Sexpr::sym("candidate"), nil(), nil()]))
                .is_none()
        );
    }

    #[test]
    fn from_sexpr_none_when_body_not_a_list() {
        assert!(Candidate::from_sexpr(Sexpr::list(vec![
            Sexpr::sym("candidate"),
            Sexpr::Str("oops".to_string())
        ]))
        .is_none());
    }

    #[test]
    fn diagnostics_total_on_hand_built_malformed_candidate() {
        // A Candidate can be hand-constructed (the `sexpr` field is
        // public) bypassing from_sexpr's validation; diagnostics must
        // still never panic.
        let node = plain_node();
        let weird = Sexpr::sym("not-even-a-list");
        let diags = candidate_diagnostics(&node, &weird);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn valid_matches_empty_diagnostics_on_good_input() {
        let node = plain_node();
        let good = candidate("m/plan/x", &[("out/a.rb", "puts 1")]);
        assert!(candidate_diagnostics(&node, &good).is_empty());
        assert!(candidate_valid(&node, &good));
    }
}
