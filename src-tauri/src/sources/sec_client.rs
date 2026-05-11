//! M13 — SEC HTTP client. Stub.

pub struct SecClient {
    pub user_agent: String,
}

impl SecClient {
    pub fn new(user_agent: String) -> Self { Self { user_agent } }
}
