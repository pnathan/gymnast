use gymnast_rs::fingerprint::{fingerprint, fingerprint_string};
use gymnast_rs::sexpr::{canonical_serialize, Sexpr};

#[test]
fn test_sexpr_sym() {
    let s = Sexpr::sym("foo");
    assert_eq!(s.print(), "foo");
}

#[test]
fn test_sexpr_sym_with_spaces() {
    let s = Sexpr::sym("hello world");
    assert_eq!(s.print(), "hello world");
}

#[test]
fn test_sexpr_str_simple() {
    let s = Sexpr::Str("hello".to_string());
    assert_eq!(s.print(), "\"hello\"");
}

#[test]
fn test_sexpr_str_with_escaped_quote() {
    let s = Sexpr::Str("say \"hi\"".to_string());
    assert_eq!(s.print(), "\"say \\\"hi\\\"\"");
}

#[test]
fn test_sexpr_str_with_escaped_backslash() {
    let s = Sexpr::Str("path\\to\\file".to_string());
    assert_eq!(s.print(), "\"path\\\\to\\\\file\"");
}

#[test]
fn test_sexpr_str_with_both_escapes() {
    let s = Sexpr::Str("say \"\\\"hi\\\"\"".to_string());
    assert_eq!(s.print(), "\"say \\\"\\\\\\\"hi\\\\\\\"\\\"\"");
}

#[test]
fn test_sexpr_str_empty() {
    let s = Sexpr::Str("".to_string());
    assert_eq!(s.print(), "\"\"");
}

#[test]
fn test_sexpr_int_positive() {
    let s = Sexpr::Int(42);
    assert_eq!(s.print(), "42");
}

#[test]
fn test_sexpr_int_zero() {
    let s = Sexpr::Int(0);
    assert_eq!(s.print(), "0");
}

#[test]
fn test_sexpr_int_negative() {
    let s = Sexpr::Int(-123);
    assert_eq!(s.print(), "-123");
}

#[test]
fn test_sexpr_int_large() {
    let s = Sexpr::Int(9223372036854775807i64);
    assert_eq!(s.print(), "9223372036854775807");
}

#[test]
fn test_sexpr_empty_list_prints_nil() {
    let s = Sexpr::List(vec![]);
    assert_eq!(s.print(), "nil");
}

#[test]
fn test_sexpr_list_single_sym() {
    let s = Sexpr::List(vec![Sexpr::sym("foo")]);
    assert_eq!(s.print(), "(foo)");
}

#[test]
fn test_sexpr_list_multiple_items() {
    let s = Sexpr::List(vec![Sexpr::sym("a"), Sexpr::sym("b"), Sexpr::sym("c")]);
    assert_eq!(s.print(), "(a b c)");
}

#[test]
fn test_sexpr_list_mixed_types() {
    let s = Sexpr::List(vec![Sexpr::sym("quote"), Sexpr::Int(42)]);
    assert_eq!(s.print(), "(quote 42)");
}

#[test]
fn test_sexpr_nested_list() {
    let s = Sexpr::List(vec![
        Sexpr::sym("a"),
        Sexpr::List(vec![Sexpr::sym("b"), Sexpr::Int(1)]),
        Sexpr::Str("x".to_string()),
    ]);
    assert_eq!(s.print(), "(a (b 1) \"x\")");
}

#[test]
fn test_sexpr_complex_nested() {
    let s = Sexpr::List(vec![
        Sexpr::sym("defn"),
        Sexpr::sym("foo"),
        Sexpr::List(vec![Sexpr::sym("x")]),
        Sexpr::List(vec![Sexpr::sym("+ x 1")]),
    ]);
    assert_eq!(s.print(), "(defn foo (x) (+ x 1))");
}

#[test]
fn test_sexpr_list_no_trailing_space() {
    // Verify that lists don't have trailing spaces before closing paren
    let s = Sexpr::List(vec![Sexpr::sym("a"), Sexpr::sym("b")]);
    let printed = s.print();
    assert!(
        !printed.ends_with(" )"),
        "List should not have trailing space before )"
    );
    assert_eq!(printed, "(a b)");
}

#[test]
fn test_sexpr_single_space_between_items() {
    // Verify exactly one space between items
    let s = Sexpr::List(vec![Sexpr::sym("a"), Sexpr::sym("b"), Sexpr::sym("c")]);
    let printed = s.print();
    assert_eq!(printed, "(a b c)");
    assert!(
        !printed.contains("  "),
        "Should only have single spaces between items"
    );
}

#[test]
fn test_sexpr_pair() {
    let s = Sexpr::pair("key", Sexpr::Int(42));
    assert_eq!(s.print(), "(key 42)");
}

#[test]
fn test_canonical_serialize_adds_newline() {
    let s = Sexpr::sym("foo");
    let serialized = canonical_serialize(&s);
    assert_eq!(serialized, "foo\n");
    assert!(serialized.ends_with('\n'));
    assert_eq!(
        serialized.matches('\n').count(),
        1,
        "Should have exactly one newline"
    );
}

#[test]
fn test_canonical_serialize_list() {
    let s = Sexpr::List(vec![Sexpr::sym("a"), Sexpr::Int(1)]);
    let serialized = canonical_serialize(&s);
    assert_eq!(serialized, "(a 1)\n");
    assert!(serialized.ends_with('\n'));
}

#[test]
fn test_canonical_serialize_empty_list() {
    let s = Sexpr::List(vec![]);
    let serialized = canonical_serialize(&s);
    assert_eq!(serialized, "nil\n");
}

#[test]
fn test_fingerprint_string_empty() {
    let fp = fingerprint_string("");
    // Empty string: hash = 0xCBF29CE484222325
    // As i64: -3750763034362895579
    assert_eq!(fp, "fnv1a64:-3750763034362895579");
}

#[test]
fn test_fingerprint_string_single_char_a() {
    let fp = fingerprint_string("a");
    // Start: 0xCBF29CE484222325
    // XOR with 'a' (97): 0xCBF29CE484222344
    // Multiply by 1099511628211: -5808556873153909620
    assert_eq!(fp, "fnv1a64:-5808556873153909620");
}

#[test]
fn test_fingerprint_string_consistency() {
    // Same input should always produce same output
    let fp1 = fingerprint_string("hello");
    let fp2 = fingerprint_string("hello");
    assert_eq!(fp1, fp2);
}

#[test]
fn test_fingerprint_string_different_inputs() {
    let fp1 = fingerprint_string("hello");
    let fp2 = fingerprint_string("world");
    assert_ne!(fp1, fp2);
}

#[test]
fn test_fingerprint_string_prefix() {
    let fp = fingerprint_string("test");
    assert!(fp.starts_with("fnv1a64:"));
}

#[test]
fn test_fingerprint_of_sexpr() {
    let s = Sexpr::sym("foo");
    let fp = fingerprint(&s);
    // Should fingerprint "foo" (the print output)
    let fp_direct = fingerprint_string("foo");
    assert_eq!(fp, fp_direct);
}

#[test]
fn test_fingerprint_of_list() {
    let s = Sexpr::List(vec![Sexpr::sym("a"), Sexpr::Int(1)]);
    let fp = fingerprint(&s);
    // Should fingerprint "(a 1)" (the print output)
    let fp_direct = fingerprint_string("(a 1)");
    assert_eq!(fp, fp_direct);
}

#[test]
fn test_fingerprint_of_nil() {
    let s = Sexpr::List(vec![]);
    let fp = fingerprint(&s);
    // Should fingerprint "nil"
    let fp_direct = fingerprint_string("nil");
    assert_eq!(fp, fp_direct);
}

#[test]
fn test_fingerprint_string_non_ascii() {
    // UTF-8 passes through unchanged
    let fp = fingerprint_string("café");
    assert!(fp.starts_with("fnv1a64:"));
    // Just verify it produces something consistent
    let fp2 = fingerprint_string("café");
    assert_eq!(fp, fp2);
}
