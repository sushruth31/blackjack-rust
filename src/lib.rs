//! A multiplayer blackjack table.
//!
//! The rules live here and know nothing about sockets: [`table::Table`] takes
//! a [`table::Command`] and returns [`table::Event`]s, which makes every rule
//! in the game testable without a network. `src/main.rs` is only a transport
//! that moves lines between TCP clients and one owning task.

pub mod card;
pub mod config;
pub mod hand;
pub mod protocol;
pub mod rules;
pub mod table;
