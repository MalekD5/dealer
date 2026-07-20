const MAX_PACKAGE_NAME_LENGTH: usize = 214;

pub fn normalize_package_name(value: &str) -> String {
    let mut name = String::with_capacity(value.len().min(MAX_PACKAGE_NAME_LENGTH));
    let mut last_was_separator = false;

    for character in value.chars().flat_map(char::to_lowercase) {
        let character = if is_name_character(character) {
            character
        } else {
            '-'
        };

        if character == '-' && last_was_separator {
            continue;
        }
        if name.len() + character.len_utf8() > MAX_PACKAGE_NAME_LENGTH {
            break;
        }

        last_was_separator = character == '-';
        name.push(character);
    }

    let name = name.trim_matches(['-', '_', '.', '~']);
    if validate_package_name(name).is_ok() {
        name.to_string()
    } else {
        "package".to_string()
    }
}

pub fn validate_package_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("package name cannot be empty".to_string());
    }
    if name.len() > MAX_PACKAGE_NAME_LENGTH {
        return Err(format!(
            "package name cannot exceed {MAX_PACKAGE_NAME_LENGTH} characters"
        ));
    }
    if name == "node_modules" || name == "favicon.ico" {
        return Err(format!("`{name}` is a reserved package name"));
    }

    let parts = if let Some(scoped_name) = name.strip_prefix('@') {
        let Some((scope, package)) = scoped_name.split_once('/') else {
            return Err("scoped package names must use the format `@scope/name`".to_string());
        };
        if package.contains('/') {
            return Err("package name can contain at most one `/`".to_string());
        }
        vec![scope, package]
    } else {
        vec![name]
    };

    for part in parts {
        if part.is_empty() {
            return Err("package name segments cannot be empty".to_string());
        }
        if part.starts_with(['-', '_', '.', '~']) {
            return Err("package name segments must start with a letter or number".to_string());
        }
        if !part.chars().all(is_name_character) {
            return Err(
                "package names must be lowercase and contain only URL-safe characters".to_string(),
            );
        }
    }

    Ok(())
}

fn is_name_character(character: char) -> bool {
    character.is_ascii_lowercase()
        || character.is_ascii_digit()
        || matches!(character, '-' | '_' | '.' | '~')
}
