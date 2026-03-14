use chrono::{DateTime, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};

use crate::error::{HeraldError, Result};

#[derive(Debug, Clone)]
pub struct OtpRecord {
    hash: Vec<u8>,
    expires_at: DateTime<Utc>,
    attempts: u32,
    max_attempts: u32,
}

pub fn generate_otp(length: usize, timeout_secs: u64, max_attempts: u32) -> (String, OtpRecord) {
    let mut rng = rand::thread_rng();
    let code: String = (0..length)
        .map(|_| rng.gen_range(0..10).to_string())
        .collect();

    let hash = Sha256::digest(code.as_bytes()).to_vec();
    let expires_at = Utc::now() + chrono::Duration::seconds(timeout_secs as i64);

    let record = OtpRecord {
        hash,
        expires_at,
        attempts: 0,
        max_attempts,
    };

    (code, record)
}

pub fn verify_otp(input: &str, record: &mut OtpRecord) -> Result<bool> {
    if Utc::now() > record.expires_at {
        return Err(HeraldError::Auth("OTP expired".to_string()));
    }

    if record.attempts >= record.max_attempts {
        return Err(HeraldError::Auth("Too many OTP attempts".to_string()));
    }

    record.attempts += 1;
    let input_hash = Sha256::digest(input.as_bytes()).to_vec();
    Ok(input_hash == record.hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_otp_length() {
        let (code, _) = generate_otp(6, 300, 3);
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_verify_otp_correct() {
        let (code, mut record) = generate_otp(6, 300, 3);
        assert!(verify_otp(&code, &mut record).unwrap());
    }

    #[test]
    fn test_verify_otp_incorrect() {
        let (_code, mut record) = generate_otp(6, 300, 3);
        assert!(!verify_otp("000000", &mut record).unwrap());
    }

    #[test]
    fn test_verify_otp_max_attempts() {
        let (_code, mut record) = generate_otp(6, 300, 2);
        let _ = verify_otp("wrong1", &mut record);
        let _ = verify_otp("wrong2", &mut record);
        assert!(verify_otp("wrong3", &mut record).is_err());
    }
}
