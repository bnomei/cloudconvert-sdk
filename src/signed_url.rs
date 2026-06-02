use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use url::form_urlencoded;

use crate::{JobCreateRequest, Result, SigningSecret};

type HmacSha256 = Hmac<Sha256>;

pub fn sign_job_url(
    base_url: impl AsRef<str>,
    signing_secret: &SigningSecret,
    job: &JobCreateRequest,
    cache_key: Option<&str>,
) -> Result<String> {
    let base_url = base_url.as_ref();
    let job_json = serde_json::to_vec(job)?;
    let encoded_job = URL_SAFE_NO_PAD.encode(job_json);
    let separator = if base_url.contains('?') { '&' } else { '?' };
    let mut unsigned = format!("{base_url}{separator}job={encoded_job}");

    if let Some(cache_key) = cache_key {
        unsigned.push_str("&cache_key=");
        unsigned.push_str(&encode_query_component(cache_key));
    }

    let mut mac = HmacSha256::new_from_slice(signing_secret.expose().as_bytes())
        .expect("HMAC accepts keys of any size");
    mac.update(unsigned.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    Ok(format!("{unsigned}&s={signature}"))
}

fn encode_query_component(value: &str) -> String {
    form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
