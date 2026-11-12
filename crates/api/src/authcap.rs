//! The sign-in methods this deployment offers (REQ-040).
//!
//! The client renders the methods that exist; a method that is absent is absent, not a disabled
//! button. This is why the report is a document and not a list of booleans: `sso` carries the
//! provider's name when one is configured.

use serde::{Deserialize, Serialize};

/// The sign-in methods this deployment offers.
///
/// The struct has one field per method the client can render, so a method that does not exist is
/// absent from the report instead of being a disabled button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools)]
pub struct AuthCapabilities {
    /// The single sign-on provider, when one is configured.
    #[serde(default)]
    pub sso: Option<SsoProvider>,
    /// Whether a person may sign in with a password.
    pub password_auth: bool,
    /// Whether a person may reset a forgotten password.
    pub password_reset: bool,
    /// Where the reset screen lives.
    #[serde(default)]
    pub reset_url: Option<String>,
    /// Whether a person may sign in with a code sent by e-mail.
    pub email_code: bool,
    /// Whether billing is offered.
    pub billing: bool,
    /// The relying-party id passkeys are bound to.
    #[serde(default)]
    pub passkey_rp_id: Option<String>,
    /// Whether the native client may use passkeys.
    pub native_passkeys: bool,
}

impl AuthCapabilities {
    /// The report of a deployment that offers password sign-in and nothing else.
    #[must_use]
    pub fn password_only(reset_url: Option<String>, passkey_rp_id: Option<String>) -> Self {
        Self {
            sso: None,
            password_auth: true,
            password_reset: true,
            reset_url,
            email_code: false,
            billing: false,
            passkey_rp_id,
            native_passkeys: false,
        }
    }
}

/// The single sign-on provider a deployment is configured with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SsoProvider {
    /// The name a person sees on the button.
    pub name: String,
    /// Whether the provider is reachable right now.
    pub availability: String,
}

#[cfg(test)]
mod tests {
    use super::AuthCapabilities;

    #[test]
    fn req_040_the_report_names_every_method_the_client_renders() {
        let capabilities = AuthCapabilities::password_only(
            Some("http://127.0.0.1:5173/reset-password".to_owned()),
            Some("127.0.0.1".to_owned()),
        );
        let json = serde_json::to_value(&capabilities).expect("the report serialises");
        let object = json.as_object().expect("the report is an object");
        for key in [
            "sso",
            "passwordAuth",
            "passwordReset",
            "resetUrl",
            "emailCode",
            "billing",
            "passkeyRpId",
            "nativePasskeys",
        ] {
            assert!(object.contains_key(key), "{key} is missing from the report");
        }
        assert!(json["sso"].is_null());
        assert_eq!(json["passwordAuth"], true);
        assert_eq!(json["emailCode"], false);
        assert_eq!(json["billing"], false);
        assert_eq!(json["nativePasskeys"], false);
    }
}
