pub fn valid_project_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        assert!(valid_project_name("astra-api"));
        assert!(valid_project_name("project_01"));
    }

    #[test]
    fn rejects_invalid_names() {
        assert!(!valid_project_name(""));
        assert!(!valid_project_name("../danger"));
        assert!(!valid_project_name("project name"));
    }
}
