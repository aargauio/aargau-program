pub mod admin;
pub mod keeper;
pub mod vault;

// Anchor's #[program] macro generates internal types (e.g. __client_accounts_*)
// that are only accessible via glob re-exports. The ambiguous_glob_reexports
// warning is a known Anchor limitation — all modules intentionally export
// a private `handler` fn with the same name, which is called via module path
// (e.g. `create_vault::handler(...)`), not via this glob.
#[allow(ambiguous_glob_reexports)]
pub use admin::*;
#[allow(ambiguous_glob_reexports)]
pub use keeper::*;
#[allow(ambiguous_glob_reexports)]
pub use vault::*;
