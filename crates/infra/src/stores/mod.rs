//! The stores: one adapter per port family, all of them talking to Postgres.
//!
//! Every adapter that reads or writes tenant data goes through
//! [`crate::tenant`], so the database always knows which tenant the query acts in (REQ-043).

pub mod accounts;
pub mod artifacts;
pub mod audit;
pub mod bots;
pub mod cards;
pub mod computer;
mod computer_row;
pub mod connectors;
pub mod events;
pub mod invitations;
pub mod mcp;
pub mod memory;
pub mod messages;
pub mod pending_tool_calls;
pub mod plugin_installs;
pub mod plugin_submissions;
pub mod rate_limits;
pub mod reactions;
pub mod read_states;
pub mod roster;
pub mod routines;
pub mod rows;
pub mod runs;
pub mod screen;
pub mod secrets;
pub mod sessions;
pub mod skills;
pub mod spaces;
pub mod threads;
pub mod tools;
pub mod users;
