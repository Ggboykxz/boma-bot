//! The stores: one adapter per port family, all of them talking to Postgres.
//!
//! Every adapter that reads or writes tenant data goes through
//! [`crate::tenant`], so the database always knows which tenant the query acts in (REQ-043).

pub mod accounts;
pub mod bots;
pub mod cards;
pub mod events;
pub mod invitations;
pub mod messages;
pub mod reactions;
pub mod read_states;
pub mod rows;
pub mod runs;
pub mod sessions;
pub mod spaces;
pub mod threads;
pub mod tools;
pub mod users;
