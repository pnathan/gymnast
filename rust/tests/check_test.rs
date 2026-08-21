use gymnast_rs::check::check;
use gymnast_rs::parser;

#[test]
fn test_e201_duplicate_mode() {
    let src = r#"
spec test = v 0.1 owner o exports Task, Task

mode Task = struct (text name)
mode Task = struct (text title)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    // Should have one E201 error for the second Task declaration
    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E201").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("duplicate mode name 'Task'"));
}

#[test]
fn test_e201_duplicate_actor() {
    let src = r#"
spec test = v 0.1 owner o exports User

actor user = person ()
actor user = person ()
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E201").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("duplicate actor name 'user'"));
}

#[test]
fn test_e201_duplicate_interface() {
    let src = r#"
spec test = v 0.1 owner o exports api

actor user = person ()

interface api = for user (
  cmd create = () text ! (err1)
)

interface api = for user (
  cmd read = () text ! (err1)
)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E201").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("duplicate interface name 'api'"));
}

#[test]
fn test_e202_unknown_mode_no_use() {
    let src = r#"
spec test = v 0.1 owner o exports Task

mode Task = struct (UnknownType name)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("unknown mode 'UnknownType'"));
}

#[test]
fn test_e202_unknown_mode_with_suggestion() {
    let src = r#"
spec test = v 0.1 owner o exports Task

mode Task = struct (txt name)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("unknown mode 'txt'"));
    assert!(errors[0].message.contains("did you mean 'text'"));
}

#[test]
fn test_w301_unknown_mode_with_use() {
    let src = r#"
spec test = v 0.1 owner o exports Task

use profiles/test @ 1.0 ()

mode Task = struct (UnknownType name)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    // Should be W301 (warning) instead of E202 (error) because there's a use declaration
    let warnings: Vec<_> = diags.iter().filter(|d| d.code == "W301").collect();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].message.contains("unknown mode 'UnknownType'"));
}

#[test]
fn test_e203_bad_interface_for_actor() {
    let src = r#"
spec test = v 0.1 owner o exports api

interface api = for unknown_actor (
  cmd create = () text ! (error1)
)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E203").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("unknown actor 'unknown_actor'"));
}

#[test]
fn test_e204_bad_behavior_interface() {
    let src = r#"
spec test = v 0.1 owner o exports b

actor user = person ()

interface iface = for user (
  cmd create = () text ! (err1)
)

behavior b = on unknown_iface.create (user, req) (
  requires true;
  returns 0
)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E204").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("unknown interface"));
}

#[test]
fn test_e204_bad_behavior_op() {
    let src = r#"
spec test = v 0.1 owner o exports b, api

actor user = person ()

interface api = for user (
  cmd create = () text ! (err1)
)

behavior b = on api.unknown_op (user, req) (
  requires true;
  returns 0
)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E204").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("has no operation"));
}

#[test]
fn test_e205_invariant_bad_scope_state() {
    let src = r#"
spec test = v 0.1 owner o exports inv_test

inv inv_test = on unknown_state always true
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E205").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("unknown scope 'unknown_state'"));
}

#[test]
fn test_e205_invariant_valid_state() {
    let src = r#"
spec test = v 0.1 owner o exports inv_test, my_state

state my_state = ()

inv inv_test = on my_state always true
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E205").collect();
    assert_eq!(errors.len(), 0);
}

#[test]
fn test_e205_constraint_bad_scope() {
    let src = r#"
spec test = v 0.1 owner o exports con_test

constraint con_test = workload on unknown_scope under () must true
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E205").collect();
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_e206_unknown_export() {
    let src = r#"
spec test = v 0.1 owner o exports Task

mode Other = struct (text name)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E206").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0]
        .message
        .contains("exported name 'Task' is not declared"));
}

#[test]
fn test_e206_export_with_suggestion() {
    let src = r#"
spec test = v 0.1 owner o exports Tasc

mode Task = struct (text name)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E206").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("did you mean 'Task'"));
}

#[test]
fn test_w301_export_with_use() {
    let src = r#"
spec test = v 0.1 owner o exports Unknown

use profiles/test @ 1.0 ()
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    // Should be W301 (warning) instead of E206 (error)
    let warnings: Vec<_> = diags.iter().filter(|d| d.code == "W301").collect();
    assert!(warnings
        .iter()
        .any(|w| w.message.contains("exported name 'Unknown'")));
}

#[test]
fn test_w302_mode_not_capitalized() {
    let src = r#"
spec test = v 0.1 owner o exports task

mode task = struct (text name)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let warnings: Vec<_> = diags.iter().filter(|d| d.code == "W302").collect();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0]
        .message
        .contains("mode name 'task' should be capitalized"));
}

#[test]
fn test_w302_non_mode_capitalized() {
    let src = r#"
spec test = v 0.1 owner o exports User

actor User = person ()
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let warnings: Vec<_> = diags.iter().filter(|d| d.code == "W302").collect();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0]
        .message
        .contains("non-mode declaration 'User' should not be capitalized"));
}

#[test]
fn test_unknown_mode_in_op_param() {
    let src = r#"
spec test = v 0.1 owner o exports api

actor user = person ()

interface api = for user (
  cmd create = (UnknownType name) text ! (err1)
)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("unknown mode 'UnknownType'"));
}

#[test]
fn test_unknown_mode_in_op_output() {
    let src = r#"
spec test = v 0.1 owner o exports api

actor user = person ()

interface api = for user (
  cmd create = (text name) UnknownOutput ! (err1)
)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("unknown mode 'UnknownOutput'"));
}

#[test]
fn test_valid_spec_with_no_errors() {
    let src = r#"
spec test = v 0.1 owner o exports User, api

actor user = person ()

mode User = struct (text name)

interface api = for user (
  cmd create = (text title) User ! (err1)
)

state my_state = ()

inv check = on my_state always true

behavior b = on api.create (user, req) (
  requires true;
  returns 0
)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    // Filter only errors, not warnings
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == gymnast_rs::diag::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 0, "Expected no errors, got: {:?}", diags);
}

#[test]
fn test_unknown_mode_in_opaque() {
    let src = r#"
spec test = v 0.1 owner o exports Id

mode Id = opaque Unknown
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_unknown_mode_in_opt() {
    let src = r#"
spec test = v 0.1 owner o exports Task

mode Task = struct (opt Unknown due)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_unknown_mode_in_row() {
    let src = r#"
spec test = v 0.1 owner o exports Task

mode Task = struct ([] Unknown items)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_multiple_errors_and_warnings() {
    let src = r#"
spec test = v 0.1 owner o exports Task, Unknown

mode Task = struct (UnknownType name)
mode task = struct (text title)

actor User = person ()
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    // E202 for UnknownType
    let e202: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert!(e202.len() > 0);

    // W302 for capitalization
    let w302: Vec<_> = diags.iter().filter(|d| d.code == "W302").collect();
    assert!(w302.len() > 0);

    // E206 for Unknown export
    let e206: Vec<_> = diags.iter().filter(|d| d.code == "E206").collect();
    assert!(e206.len() > 0);
}

#[test]
fn test_builtin_modes_available() {
    let src = r#"
spec test = v 0.1 owner o exports Task, List

mode Task = struct (
  text name,
  int priority,
  bool done,
  local_date created,
  zoned_datetime updated
)

mode List = struct (
  void marker
)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    // Should have no E202 errors for builtin modes
    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert_eq!(
        errors.len(),
        0,
        "Expected no unknown mode errors, got: {:?}",
        diags
    );
}

#[test]
fn test_named_mode_reference() {
    let src = r#"
spec test = v 0.1 owner o exports Task, List

mode List = struct (text name)

mode Task = struct (List list)
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E202").collect();
    assert_eq!(errors.len(), 0);
}

#[test]
fn test_invariant_on_interface() {
    let src = r#"
spec test = v 0.1 owner o exports inv, api

actor user = person ()

interface api = for user (
  cmd create = () text ! (err1)
)

inv inv = on api always true
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E205").collect();
    assert_eq!(errors.len(), 0);
}

#[test]
fn test_constraint_on_component() {
    let src = r#"
spec test = v 0.1 owner o exports con, comp

component comp = ()

constraint con = workload on comp under () must true
"#;
    let (ast, _) = parser::parse(src);
    let file = ast.unwrap();
    let diags = check(&file);

    let errors: Vec<_> = diags.iter().filter(|d| d.code == "E205").collect();
    assert_eq!(errors.len(), 0);
}

#[test]
fn test_suggestion_tiebreak_is_deterministic() {
    // Two candidates at equal edit distance: the suggestion must be the
    // alphabetically first, not whatever HashMap iteration happens to yield.
    let src = r#"
spec test = v 0.1 owner o exports Alpha1

mode Alpha1 = opaque text
mode Alpha2 = opaque text
mode Holder = struct (Alpha3 field)
"#;
    let (ast, parse_diags) = parser::parse(src);
    assert!(parse_diags.is_empty(), "{:#?}", parse_diags);
    let diags = check(&ast.unwrap());
    let unknown = diags
        .iter()
        .find(|d| d.code == "E202")
        .expect("Alpha3 must be flagged");
    assert!(
        unknown.message.contains("did you mean 'Alpha1'?"),
        "suggestion must tie-break alphabetically, got: {}",
        unknown.message
    );
}
