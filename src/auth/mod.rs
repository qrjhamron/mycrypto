//! Authentication module.
//!
//! Handles provider auth state, GitHub device flow, and secure API key storage.

pub mod apikey;
pub mod github;
pub mod store;

pub use apikey::{
    apply_keys_to_env, load_keys, remove_provider as remove_stored_provider, set_api_key,
    set_gradio, sync_auth_state,
};
pub use github::{AuthEvent, GitHubAuth, StoredGithubAuth};
pub use store::{default_auth_state, AuthProvider, AuthStatus};
