use gymnast_rs::lexer::{Lexer, TokenKind};
use gymnast_rs::span::Span;

#[test]
fn test_empty_input() {
    let (tokens, diags) = Lexer::tokenize("");
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].kind, TokenKind::Eof);
    assert_eq!(diags.len(), 0);
}

#[test]
fn test_simple_identifier() {
    let (tokens, diags) = Lexer::tokenize("hello");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens.len(), 2);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "hello"),
        _ => panic!("expected Ident"),
    }
    assert_eq!(tokens[1].kind, TokenKind::Eof);
}

#[test]
fn test_pascal_case_identifier() {
    let (tokens, diags) = Lexer::tokenize("ListId");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens.len(), 2);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "ListId"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn test_snake_case_identifier() {
    let (tokens, diags) = Lexer::tokenize("create_task");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens.len(), 2);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "create_task"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn test_underscore_identifier() {
    let (tokens, diags) = Lexer::tokenize("_private");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "_private"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn test_integer() {
    let (tokens, diags) = Lexer::tokenize("42");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens[0].kind, TokenKind::Int(42));
}

#[test]
fn test_zero() {
    let (tokens, diags) = Lexer::tokenize("0");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens[0].kind, TokenKind::Int(0));
}

#[test]
fn test_large_integer() {
    let (tokens, diags) = Lexer::tokenize("9223372036854775807");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens[0].kind, TokenKind::Int(i64::MAX));
}

#[test]
fn test_integer_overflow() {
    let (_tokens, diags) = Lexer::tokenize("9223372036854775808");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E003");
    assert!(diags[0].message.contains("out of range"));
}

#[test]
fn test_simple_string() {
    let (tokens, diags) = Lexer::tokenize("\"hello\"");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Str(s) => assert_eq!(s, "hello"),
        _ => panic!("expected Str"),
    }
}

#[test]
fn test_string_with_escaped_quote() {
    let (tokens, diags) = Lexer::tokenize("\"hello\\\"world\"");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Str(s) => assert_eq!(s, "hello\"world"),
        _ => panic!("expected Str"),
    }
}

#[test]
fn test_string_with_escaped_backslash() {
    let (tokens, diags) = Lexer::tokenize("\"hello\\\\world\"");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Str(s) => assert_eq!(s, "hello\\world"),
        _ => panic!("expected Str"),
    }
}

#[test]
fn test_string_with_both_escapes() {
    let (tokens, diags) = Lexer::tokenize("\"path\\\\to\\\"file\"");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Str(s) => assert_eq!(s, "path\\to\"file"),
        _ => panic!("expected Str"),
    }
}

#[test]
fn test_empty_string() {
    let (tokens, diags) = Lexer::tokenize("\"\"");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Str(s) => assert!(s.is_empty()),
        _ => panic!("expected Str"),
    }
}

#[test]
fn test_unterminated_string() {
    let (_tokens, diags) = Lexer::tokenize("\"hello");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E002");
    assert!(diags[0].message.contains("unterminated"));
}

#[test]
fn test_unterminated_string_with_continuation() {
    let (_tokens, diags) = Lexer::tokenize("\"hello\nworld");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E002");
}

#[test]
fn test_left_paren() {
    let (tokens, _) = Lexer::tokenize("(");
    assert_eq!(tokens[0].kind, TokenKind::LParen);
}

#[test]
fn test_right_paren() {
    let (tokens, _) = Lexer::tokenize(")");
    assert_eq!(tokens[0].kind, TokenKind::RParen);
}

#[test]
fn test_comma() {
    let (tokens, _) = Lexer::tokenize(",");
    assert_eq!(tokens[0].kind, TokenKind::Comma);
}

#[test]
fn test_semicolon() {
    let (tokens, _) = Lexer::tokenize(";");
    assert_eq!(tokens[0].kind, TokenKind::Semi);
}

#[test]
fn test_colon() {
    let (tokens, _) = Lexer::tokenize(":");
    assert_eq!(tokens[0].kind, TokenKind::Colon);
}

#[test]
fn test_equals() {
    let (tokens, _) = Lexer::tokenize("=");
    assert_eq!(tokens[0].kind, TokenKind::Eq);
}

#[test]
fn test_bang() {
    let (tokens, _) = Lexer::tokenize("!");
    assert_eq!(tokens[0].kind, TokenKind::Bang);
}

#[test]
fn test_dot() {
    let (tokens, _) = Lexer::tokenize(".");
    assert_eq!(tokens[0].kind, TokenKind::Dot);
}

#[test]
fn test_at() {
    let (tokens, _) = Lexer::tokenize("@");
    assert_eq!(tokens[0].kind, TokenKind::At);
}

#[test]
fn test_slash() {
    let (tokens, _) = Lexer::tokenize("/");
    assert_eq!(tokens[0].kind, TokenKind::Slash);
}

#[test]
fn test_less_than() {
    let (tokens, _) = Lexer::tokenize("<");
    assert_eq!(tokens[0].kind, TokenKind::Lt);
}

#[test]
fn test_less_than_or_equal() {
    let (tokens, _) = Lexer::tokenize("<=");
    assert_eq!(tokens[0].kind, TokenKind::Le);
}

#[test]
fn test_arrow() {
    let (tokens, _) = Lexer::tokenize("->");
    assert_eq!(tokens[0].kind, TokenKind::Arrow);
}

#[test]
fn test_dot_dot() {
    let (tokens, _) = Lexer::tokenize("..");
    assert_eq!(tokens[0].kind, TokenKind::DotDot);
}

#[test]
fn test_bare_minus_error() {
    let (_tokens, diags) = Lexer::tokenize("-");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E001");
    assert!(diags[0].message.contains("-"));
}

#[test]
fn test_dot_vs_dotdot() {
    let (tokens, diags) = Lexer::tokenize(". ..");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens[0].kind, TokenKind::Dot);
    assert_eq!(tokens[1].kind, TokenKind::DotDot);
    assert_eq!(tokens[2].kind, TokenKind::Eof);
}

#[test]
fn test_lt_vs_le() {
    let (tokens, diags) = Lexer::tokenize("< <=");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens[0].kind, TokenKind::Lt);
    assert_eq!(tokens[1].kind, TokenKind::Le);
}

#[test]
fn test_comment_to_eol() {
    let (tokens, diags) = Lexer::tokenize("hello # comment\nworld");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens.len(), 3);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "hello"),
        _ => panic!("expected Ident"),
    }
    match &tokens[1].kind {
        TokenKind::Ident(s) => assert_eq!(s, "world"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn test_comment_at_end() {
    let (tokens, diags) = Lexer::tokenize("hello # comment");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens.len(), 2);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "hello"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn test_whitespace_skipped() {
    let (tokens, diags) = Lexer::tokenize("  hello  \t  world  \n");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens.len(), 3);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "hello"),
        _ => panic!("expected Ident"),
    }
    match &tokens[1].kind {
        TokenKind::Ident(s) => assert_eq!(s, "world"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn test_mixed_punctuation() {
    let (tokens, diags) = Lexer::tokenize("( ) , ; : = ! . @ /");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens[0].kind, TokenKind::LParen);
    assert_eq!(tokens[1].kind, TokenKind::RParen);
    assert_eq!(tokens[2].kind, TokenKind::Comma);
    assert_eq!(tokens[3].kind, TokenKind::Semi);
    assert_eq!(tokens[4].kind, TokenKind::Colon);
    assert_eq!(tokens[5].kind, TokenKind::Eq);
    assert_eq!(tokens[6].kind, TokenKind::Bang);
    assert_eq!(tokens[7].kind, TokenKind::Dot);
    assert_eq!(tokens[8].kind, TokenKind::At);
    assert_eq!(tokens[9].kind, TokenKind::Slash);
}

#[test]
fn test_unknown_character() {
    let (tokens, diags) = Lexer::tokenize("hello $ world");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E001");
    assert!(diags[0].message.contains("$"));
    assert_eq!(tokens.len(), 3);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "hello"),
        _ => panic!("expected Ident"),
    }
    match &tokens[1].kind {
        TokenKind::Ident(s) => assert_eq!(s, "world"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn test_multiple_errors_recovery() {
    let (tokens, diags) = Lexer::tokenize("hello $ world % test");
    assert_eq!(diags.len(), 2);
    assert_eq!(diags[0].code, "E001");
    assert_eq!(diags[1].code, "E001");
    assert_eq!(tokens.len(), 4);
}

#[test]
fn test_span_single_char() {
    let (tokens, _) = Lexer::tokenize("(");
    assert_eq!(tokens[0].span, Span { start: 0, end: 1 });
}

#[test]
fn test_span_multi_char_ident() {
    let (tokens, _) = Lexer::tokenize("hello");
    assert_eq!(tokens[0].span, Span { start: 0, end: 5 });
}

#[test]
fn test_span_integer() {
    let (tokens, _) = Lexer::tokenize("42");
    assert_eq!(tokens[0].span, Span { start: 0, end: 2 });
}

#[test]
fn test_span_string() {
    let (tokens, _) = Lexer::tokenize("\"hello\"");
    assert_eq!(tokens[0].span, Span { start: 0, end: 7 });
}

#[test]
fn test_span_arrow() {
    let (tokens, _) = Lexer::tokenize("->");
    assert_eq!(tokens[0].span, Span { start: 0, end: 2 });
}

#[test]
fn test_span_dotdot() {
    let (tokens, _) = Lexer::tokenize("..");
    assert_eq!(tokens[0].span, Span { start: 0, end: 2 });
}

#[test]
fn test_span_le() {
    let (tokens, _) = Lexer::tokenize("<=");
    assert_eq!(tokens[0].span, Span { start: 0, end: 2 });
}

#[test]
fn test_spans_two_line_input() {
    let src = "hello\nworld";
    let (tokens, _) = Lexer::tokenize(src);
    assert_eq!(tokens[0].span, Span { start: 0, end: 5 });
    assert_eq!(tokens[1].span, Span { start: 6, end: 11 });
}

#[test]
fn test_spec_header_tokens() {
    let (tokens, diags) = Lexer::tokenize("spec todo = v 0.1 owner product");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens.len(), 10);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "spec"),
        _ => panic!("expected Ident"),
    }
    match &tokens[1].kind {
        TokenKind::Ident(s) => assert_eq!(s, "todo"),
        _ => panic!("expected Ident"),
    }
    assert_eq!(tokens[2].kind, TokenKind::Eq);
    match &tokens[3].kind {
        TokenKind::Ident(s) => assert_eq!(s, "v"),
        _ => panic!("expected Ident"),
    }
    assert_eq!(tokens[4].kind, TokenKind::Int(0));
    assert_eq!(tokens[5].kind, TokenKind::Dot);
    assert_eq!(tokens[6].kind, TokenKind::Int(1));
    match &tokens[7].kind {
        TokenKind::Ident(s) => assert_eq!(s, "owner"),
        _ => panic!("expected Ident"),
    }
    match &tokens[8].kind {
        TokenKind::Ident(s) => assert_eq!(s, "product"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn test_use_declaration_tokens() {
    let (tokens, diags) = Lexer::tokenize("use path/to/module @ 1.0 (key value)");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "use"),
        _ => panic!("expected Ident"),
    }
    match &tokens[1].kind {
        TokenKind::Ident(s) => assert_eq!(s, "path"),
        _ => panic!("expected Ident"),
    }
    assert_eq!(tokens[2].kind, TokenKind::Slash);
    match &tokens[3].kind {
        TokenKind::Ident(s) => assert_eq!(s, "to"),
        _ => panic!("expected Ident"),
    }
    assert_eq!(tokens[4].kind, TokenKind::Slash);
    match &tokens[5].kind {
        TokenKind::Ident(s) => assert_eq!(s, "module"),
        _ => panic!("expected Ident"),
    }
    assert_eq!(tokens[6].kind, TokenKind::At);
}

#[test]
fn test_mode_declaration() {
    let (tokens, diags) = Lexer::tokenize("mode Task = struct ( text title, int count )");
    assert_eq!(diags.len(), 0);
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "mode")));
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "Task")));
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "struct")));
}

#[test]
fn test_interface_tokens() {
    let (tokens, diags) =
        Lexer::tokenize("interface service = for user ( cmd create = (int id) string ! (error) )");
    assert_eq!(diags.len(), 0);
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "interface")));
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "cmd")));
    assert_eq!(
        tokens.iter().filter(|t| t.kind == TokenKind::Bang).count(),
        1
    );
}

#[test]
fn test_flow_tokens() {
    let (tokens, diags) = Lexer::tokenize("flow auth = user -> service : cmd");
    assert_eq!(diags.len(), 0);
    let arrow_count = tokens.iter().filter(|t| t.kind == TokenKind::Arrow).count();
    assert_eq!(arrow_count, 1);
}

#[test]
fn test_behavior_tokens() {
    let (tokens, diags) = Lexer::tokenize(
        "behavior create = on service.create (user, req) ( requires authenticated (user); ensures result )",
    );
    assert_eq!(diags.len(), 0);
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "requires")));
    let semi_count = tokens.iter().filter(|t| t.kind == TokenKind::Semi).count();
    assert_eq!(semi_count, 1);
}

#[test]
fn test_refined_mode() {
    let (tokens, diags) = Lexer::tokenize("mode Limited = text (1..200)");
    assert_eq!(diags.len(), 0);
    assert!(tokens.iter().any(|t| t.kind == TokenKind::DotDot));
}

#[test]
fn test_invariant_tokens() {
    let (tokens, diags) = Lexer::tokenize("inv isolation = on state always no_observation");
    assert_eq!(diags.len(), 0);
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "inv")));
}

#[test]
fn test_constraint_tokens() {
    let (tokens, diags) = Lexer::tokenize(
        "constraint load = workload on service under (users 100) must high_capacity",
    );
    assert_eq!(diags.len(), 0);
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "constraint")));
}

#[test]
fn test_quantifier_tokens() {
    let (tokens, diags) = Lexer::tokenize("for all State x: valid_state");
    assert_eq!(diags.len(), 0);
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "for")));
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "all")));
}

#[test]
fn test_multiple_comments() {
    let (tokens, diags) = Lexer::tokenize("a # comment1\n b # comment2\n c");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens.len(), 4);
    match &tokens[0].kind {
        TokenKind::Ident(s) => assert_eq!(s, "a"),
        _ => panic!("expected Ident"),
    }
    match &tokens[1].kind {
        TokenKind::Ident(s) => assert_eq!(s, "b"),
        _ => panic!("expected Ident"),
    }
    match &tokens[2].kind {
        TokenKind::Ident(s) => assert_eq!(s, "c"),
        _ => panic!("expected Ident"),
    }
}

#[test]
fn test_numeric_literals_sequence() {
    let (tokens, diags) = Lexer::tokenize("1 22 333");
    assert_eq!(diags.len(), 0);
    assert_eq!(tokens[0].kind, TokenKind::Int(1));
    assert_eq!(tokens[1].kind, TokenKind::Int(22));
    assert_eq!(tokens[2].kind, TokenKind::Int(333));
}

#[test]
fn test_string_with_spaces() {
    let (tokens, diags) = Lexer::tokenize("\"hello world\"");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Str(s) => assert_eq!(s, "hello world"),
        _ => panic!("expected Str"),
    }
}

#[test]
fn test_string_with_special_chars() {
    let (tokens, diags) = Lexer::tokenize("\"hello@world.test\"");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Str(s) => assert_eq!(s, "hello@world.test"),
        _ => panic!("expected Str"),
    }
}

#[test]
fn test_multiple_unknown_chars() {
    let (_tokens, diags) = Lexer::tokenize("hello $ @ # world");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E001");
}

#[test]
fn test_no_keyword_tokens() {
    let (tokens, diags) = Lexer::tokenize("mode spec actor interface behavior");
    assert_eq!(diags.len(), 0);
    for i in 0..5 {
        match &tokens[i].kind {
            TokenKind::Ident(_) => {}
            _ => panic!("expected Ident at position {}", i),
        }
    }
}

#[test]
fn test_contextual_keywords() {
    let (tokens, diags) = Lexer::tokenize("owner list local_date");
    assert_eq!(diags.len(), 0);
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "owner")));
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "list")));
    assert!(tokens
        .iter()
        .any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "local_date")));
}

#[test]
fn test_escape_other_char() {
    let (tokens, diags) = Lexer::tokenize("\"hello\\nworld\"");
    assert_eq!(diags.len(), 0);
    match &tokens[0].kind {
        TokenKind::Str(s) => assert_eq!(s, "hello\\nworld"),
        _ => panic!("expected Str"),
    }
}

#[test]
fn test_complex_real_world() {
    let src = r#"spec todo = v 0.1 owner product
use path/to/module @ 1.0 (key value)
mode Task = struct (
  int id,
  text title,
  opt Due due
)
interface service = for user (
  cmd create = (int list, text title) Task ! (forbidden),
  qry query = (int list) Page ! (not_found)
)"#;
    let (tokens, diags) = Lexer::tokenize(src);
    assert!(
        diags.is_empty(),
        "expected no diagnostics, got: {:?}",
        diags
    );
    assert!(tokens.len() > 0);
    assert_eq!(tokens.last().unwrap().kind, TokenKind::Eof);
}

#[test]
fn test_all_punctuation_in_sequence() {
    let (tokens, diags) = Lexer::tokenize("(),.;:=!.@/<->..");
    assert_eq!(diags.len(), 0);
    let mut types = tokens.iter().take(tokens.len() - 1).map(|t| &t.kind);
    assert_eq!(types.next().unwrap(), &TokenKind::LParen);
    assert_eq!(types.next().unwrap(), &TokenKind::RParen);
    assert_eq!(types.next().unwrap(), &TokenKind::Comma);
    assert_eq!(types.next().unwrap(), &TokenKind::Dot);
    assert_eq!(types.next().unwrap(), &TokenKind::Semi);
    assert_eq!(types.next().unwrap(), &TokenKind::Colon);
    assert_eq!(types.next().unwrap(), &TokenKind::Eq);
    assert_eq!(types.next().unwrap(), &TokenKind::Bang);
    assert_eq!(types.next().unwrap(), &TokenKind::Dot);
    assert_eq!(types.next().unwrap(), &TokenKind::At);
    assert_eq!(types.next().unwrap(), &TokenKind::Slash);
    assert_eq!(types.next().unwrap(), &TokenKind::Lt);
    assert_eq!(types.next().unwrap(), &TokenKind::Arrow);
    assert_eq!(types.next().unwrap(), &TokenKind::DotDot);
}

#[test]
fn test_utf8_string_content_roundtrips() {
    // Multi-byte UTF-8 in string literals must not be corrupted into
    // per-byte mojibake.
    let (tokens, diags) = Lexer::tokenize("\"Café — geführt\"");
    assert!(diags.is_empty());
    match &tokens[0].kind {
        TokenKind::Str(s) => assert_eq!(s, "Café — geführt"),
        other => panic!("expected string token, got {:?}", other),
    }
}

#[test]
fn test_non_ascii_outside_string_is_one_error_per_char() {
    // One E001 per character (not per byte), and lexing continues.
    let (tokens, diags) = Lexer::tokenize("mode é = opaque text");
    assert_eq!(diags.iter().filter(|d| d.code == "E001").count(), 1);
    let idents: Vec<_> = tokens
        .iter()
        .filter_map(|t| match &t.kind {
            TokenKind::Ident(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(idents, vec!["mode", "opaque", "text"]);
}
