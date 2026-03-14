use uuid::Uuid;

use crate::types::SessionToken;

pub fn generate_token() -> SessionToken {
    SessionToken(Uuid::new_v4().to_string())
}

pub fn validate_token(provided: &str, expected: &SessionToken) -> bool {
    // Constant-time comparison to prevent timing attacks
    if provided.len() != expected.0.len() {
        return false;
    }
    provided
        .bytes()
        .zip(expected.0.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1.0, t2.0);
        assert_eq!(t1.0.len(), 36); // UUID v4 format
    }

    #[test]
    fn test_validate_token() {
        let token = generate_token();
        assert!(validate_token(&token.0, &token));
        assert!(!validate_token("wrong", &token));
    }
}
