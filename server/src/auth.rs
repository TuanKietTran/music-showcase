//! Authentication: local bearer tokens (accounts.tsv + bootstrap MUSIC_TOKEN) and, with feature `aws`,
//! optional Cognito/OIDC JWTs. Both produce the same `Account` subject attributes.
use crate::accounts::{Account, Accounts};

pub struct Auth {
    pub accounts: Accounts,
    #[cfg(feature = "aws")]
    pub oidc: Option<crate::cognito::Oidc>,
}

impl Auth {
    pub async fn authenticate(&self, token: &str) -> Option<Account> {
        #[cfg(feature = "aws")]
        if let Some(o) = &self.oidc {
            // A JWT has exactly two dots; random bearer tokens (hex/base64url without dots) never take this path.
            if token.matches('.').count() == 2 { return o.verify(token).await; }
        }
        self.accounts.lookup(token)
    }
}
