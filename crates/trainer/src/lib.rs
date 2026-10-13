//! Drill engine (Milestone 2): pick a solved spot, walk to a decision, deal the player a hand,
//! grade the action by EV loss, explain it, and log the attempt.

pub mod db;
pub mod drill;
pub mod import;
pub mod ranges;
pub mod reason;
pub mod scoring;
pub mod spots;

pub use drill::{DrillView, Engine, Feedback, Filters};
