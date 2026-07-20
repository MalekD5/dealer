use dealer::manifest::package_name::{normalize_package_name, validate_package_name};

#[test]
fn normalizes_directory_names_for_npm() {
    assert_eq!(
        normalize_package_name("My Cool Package!"),
        "my-cool-package"
    );
    assert_eq!(normalize_package_name("__Dealer.Tools__"), "dealer.tools");
    assert_eq!(normalize_package_name("مشروع"), "package");
    assert_eq!(normalize_package_name("node_modules"), "package");
}

#[test]
fn validates_unscoped_and_scoped_package_names() {
    assert!(validate_package_name("dealer-tools").is_ok());
    assert!(validate_package_name("@dealer/tools").is_ok());
    assert!(validate_package_name("Dealer Tools").is_err());
    assert!(validate_package_name("_dealer").is_err());
    assert!(validate_package_name("@dealer").is_err());
}
