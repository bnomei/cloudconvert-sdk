//! Webhook payload signing and signature verification helpers.
//!
//! CloudConvert signs webhook bodies with the webhook signing secret. These
//! helpers compute the expected HMAC-SHA256 hex digest for outbound tests and
//! verify inbound `CloudConvert-Signature` header values.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

use crate::{Error, Result, SigningSecret};

type HmacSha256 = Hmac<Sha256>;

/// Returns `true` when `signature_hex` matches the HMAC-SHA256 digest of `payload`.
pub fn verify_signature(
    payload: impl AsRef<[u8]>,
    signature_hex: impl AsRef<str>,
    signing_secret: &SigningSecret,
) -> Result<bool> {
    let expected = hex::decode(signature_hex.as_ref()).map_err(|_| Error::InvalidSignatureHex)?;
    let mut mac = HmacSha256::new_from_slice(signing_secret.expose().as_bytes())
        .map_err(|_| Error::InvalidSignatureHex)?;
    mac.update(payload.as_ref());
    Ok(mac.verify_slice(&expected).is_ok())
}

/// Computes the HMAC-SHA256 hex digest CloudConvert uses for webhook payloads.
pub fn sign_payload(payload: impl AsRef<[u8]>, signing_secret: &SigningSecret) -> Result<String> {
    let mut mac = HmacSha256::new_from_slice(signing_secret.expose().as_bytes())
        .map_err(|_| Error::InvalidSignatureHex)?;
    mac.update(payload.as_ref());
    Ok(hex::encode(mac.finalize().into_bytes()))
}
