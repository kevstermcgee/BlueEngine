//! Real-time adapter. The original scene, geometry and offline renderer remain shared.
pub mod controller;
pub mod game;
pub mod game_example;
#[cfg(feature = "client")]
pub mod input;
pub mod interaction;
#[cfg(feature = "client")]
pub mod mesh;
pub mod profile;
pub mod room;

pub mod wrench;

pub mod camera;

pub mod simulation;

pub mod props;

mod house;
pub mod maps;
pub mod test_lab;

pub mod authoring;
pub mod builder;
pub mod capabilities;
pub mod content;

mod landscaping;

pub mod fps;
pub mod prop_physics;
pub mod weapons;

pub mod lifecycle;
pub mod metrics;
pub mod net;
pub mod server;
pub mod spatial;

pub mod blueprint;
pub mod doc_drift;
pub mod gen;
pub mod lint;
pub mod mcp;
pub mod newgame;
pub mod pathing;
pub mod reach;
pub mod scenario;
pub mod symbols;
pub mod ui_check;
pub mod verify;
