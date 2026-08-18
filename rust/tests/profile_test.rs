use gymnast_rs::ast::{Decl, ModeExpr};
use gymnast_rs::profile;

#[test]
fn test_lookup_hit() {
    let profile = profile::lookup("oddities/profiles/todo_standard", "1.0");
    assert!(profile.is_some());
}

#[test]
fn test_lookup_miss() {
    let profile = profile::lookup("unknown/profile", "1.0");
    assert!(profile.is_none());
}

#[test]
fn test_generator_produces_four_decls() {
    let prof = profile::lookup("oddities/profiles/todo_standard", "1.0").unwrap();
    let decls = (prof.generate)(&[]);
    assert_eq!(decls.len(), 4);
}

#[test]
fn test_generator_decl_names() {
    let prof = profile::lookup("oddities/profiles/todo_standard", "1.0").unwrap();
    let decls = (prof.generate)(&[]);

    let names: Vec<String> = decls
        .iter()
        .filter_map(|decl| {
            if let Decl::Mode(mode_decl) = decl {
                Some(mode_decl.name.text.clone())
            } else {
                None
            }
        })
        .collect();

    assert_eq!(names, vec!["Cursor", "Page", "Membership", "Invitation"]);
}

#[test]
fn test_membership_struct_field_names_in_order() {
    let prof = profile::lookup("oddities/profiles/todo_standard", "1.0").unwrap();
    let decls = (prof.generate)(&[]);

    // Membership is the 3rd declaration (index 2)
    if let Decl::Mode(mode_decl) = &decls[2] {
        assert_eq!(mode_decl.name.text, "Membership");
        if let ModeExpr::Struct(fields) = &mode_decl.expr {
            let field_names: Vec<&str> = fields.iter().map(|f| f.name.text.as_str()).collect();
            assert_eq!(field_names, vec!["list", "principal", "role", "version"]);
        } else {
            panic!("Membership should be a struct");
        }
    } else {
        panic!("Third declaration should be Membership mode");
    }
}

#[test]
fn test_invitation_struct_field_names_in_order() {
    let prof = profile::lookup("oddities/profiles/todo_standard", "1.0").unwrap();
    let decls = (prof.generate)(&[]);

    // Invitation is the 4th declaration (index 3)
    if let Decl::Mode(mode_decl) = &decls[3] {
        assert_eq!(mode_decl.name.text, "Invitation");
        if let ModeExpr::Struct(fields) = &mode_decl.expr {
            let field_names: Vec<&str> = fields.iter().map(|f| f.name.text.as_str()).collect();
            assert_eq!(field_names, vec!["list", "principal", "role", "version"]);
        } else {
            panic!("Invitation should be a struct");
        }
    } else {
        panic!("Fourth declaration should be Invitation mode");
    }
}
