//! Real-time adapter. The original scene, geometry and offline renderer remain shared.
#[cfg(unix)]
pub mod control;
pub mod controller;
pub mod devkit;
pub mod game;
#[cfg(feature = "presentation")]
pub mod game_client;
pub mod game_edit;
pub mod game_example;
pub mod game_explore;
#[cfg(feature = "physics")]
pub mod game_scenario;
#[cfg(feature = "gamepad")]
pub mod gamepad;
pub mod icon;
pub mod identity;
#[cfg(feature = "native-client")]
pub mod input;
pub mod interaction;
#[cfg(feature = "presentation")]
pub mod kit;
#[cfg(feature = "presentation")]
pub mod mesh;
#[cfg(feature = "narrative")]
pub mod narrative;
pub mod netplay;
pub mod presentation;
pub mod profile;
pub mod room;
pub mod savestate;
pub mod stock_audio;
pub mod stock_presentation;

pub mod wrench;

pub mod camera;

pub mod shutdown;
pub mod simulation;

pub mod props;
pub mod spans;

mod house;
pub mod maps;
pub mod test_lab;

pub mod authoring;
pub mod builder;
pub mod capabilities;
pub mod content;

mod landscaping;

pub mod arena;
pub mod combat;
pub mod fps;
#[cfg(feature = "physics")]
pub mod prop_physics;
pub mod stock_demo;
pub mod weapons;

pub mod lifecycle;
pub mod metrics;
pub mod net;
#[cfg(feature = "physics")]
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
#[cfg(feature = "physics")]
pub mod scenario;
pub mod symbols;
pub mod ui_check;
pub mod verify;

#[cfg(any(feature = "presentation", feature = "two-d-draw"))]
pub mod focus;
#[cfg(any(feature = "presentation", feature = "two-d-draw"))]
pub mod game_text;
#[cfg(feature = "presentation")]
pub mod game_visuals;
#[cfg(any(feature = "presentation", feature = "two-d-draw"))]
pub mod startup;

#[cfg(feature = "presentation")]
pub mod character_skins;
/// Rendering-independent placement, safe removal and atomic map saves.
pub mod creative;
#[cfg(feature = "native-client")]
pub mod game_input;
#[cfg(feature = "presentation")]
pub mod game_ui;

#[cfg(feature = "presentation")]
pub mod character;
#[cfg(feature = "presentation")]
pub mod wrench_view;

#[cfg(all(feature = "client", feature = "physics"))]
pub mod local_client;

#[cfg(feature = "physics")]
pub mod game_session;
#[cfg(all(feature = "client", feature = "physics"))]
pub mod playable;
#[cfg(all(feature = "presentation", feature = "physics"))]
pub mod prop_view;
