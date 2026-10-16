//! Stable semantic interaction IDs shared by deterministic rules and the audio client.
//! Append new cues instead of changing these values: captures contain their numeric IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum Cue {
    SparkPickup = 0,
    ShardPickup = 1,
    BloomUnfurl = 2,
    EmberIgnite = 3,
    InvincibilityShimmer = 4,
    ExtraLife = 5,
    ClayBreak = 6,
    BlockReveal = 7,
    Checkpoint = 8,
    Jump = 9,
    Spring = 10,
    SeedCast = 11,
    EnemyBounce = 12,
    ShellKick = 13,
    BossHit = 14,
    GuardianFreed = 15,
    Hurt = 16,
    Death = 17,
    GrottoEnter = 18,
    GrottoExit = 19,
    MenuConfirm = 20,
    CourseClear = 21,
    Homecoming = 22,
    MenuNavigate = 23,
    MenuBack = 24,
}

impl Cue {
    pub const ALL: [Self; 25] = [
        Self::SparkPickup,
        Self::ShardPickup,
        Self::BloomUnfurl,
        Self::EmberIgnite,
        Self::InvincibilityShimmer,
        Self::ExtraLife,
        Self::ClayBreak,
        Self::BlockReveal,
        Self::Checkpoint,
        Self::Jump,
        Self::Spring,
        Self::SeedCast,
        Self::EnemyBounce,
        Self::ShellKick,
        Self::BossHit,
        Self::GuardianFreed,
        Self::Hurt,
        Self::Death,
        Self::GrottoEnter,
        Self::GrottoExit,
        Self::MenuConfirm,
        Self::CourseClear,
        Self::Homecoming,
        Self::MenuNavigate,
        Self::MenuBack,
    ];

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::SparkPickup => "spark-pickup",
            Self::ShardPickup => "shard-pickup",
            Self::BloomUnfurl => "bloom-unfurl",
            Self::EmberIgnite => "ember-ignite",
            Self::InvincibilityShimmer => "invincibility-shimmer",
            Self::ExtraLife => "extra-life",
            Self::ClayBreak => "clay-break",
            Self::BlockReveal => "block-reveal",
            Self::Checkpoint => "checkpoint",
            Self::Jump => "jump",
            Self::Spring => "spring",
            Self::SeedCast => "seed-cast",
            Self::EnemyBounce => "enemy-bounce",
            Self::ShellKick => "shell-kick",
            Self::BossHit => "boss-hit",
            Self::GuardianFreed => "guardian-freed",
            Self::Hurt => "hurt",
            Self::Death => "death",
            Self::GrottoEnter => "grotto-enter",
            Self::GrottoExit => "grotto-exit",
            Self::MenuConfirm => "menu-confirm",
            Self::CourseClear => "course-clear",
            Self::Homecoming => "homecoming",
            Self::MenuNavigate => "menu-navigate",
            Self::MenuBack => "menu-back",
        }
    }

    /// Repeated ordinary actions stay below the score and important world responses.
    pub const fn gain(self) -> f32 {
        match self {
            Self::Jump | Self::SeedCast | Self::MenuNavigate => 0.13,
            Self::MenuConfirm | Self::MenuBack | Self::SparkPickup => 0.19,
            Self::EnemyBounce | Self::ShellKick | Self::BlockReveal => 0.25,
            Self::ClayBreak | Self::Spring | Self::GrottoEnter | Self::GrottoExit => 0.28,
            Self::Hurt | Self::Death | Self::BossHit => 0.32,
            _ => 0.30,
        }
    }

    pub const fn priority(self) -> u8 {
        match self {
            Self::Jump | Self::SeedCast | Self::MenuNavigate => 0,
            Self::SparkPickup | Self::EnemyBounce | Self::ShellKick => 1,
            Self::Hurt | Self::Death | Self::BossHit => 3,
            Self::GuardianFreed | Self::CourseClear | Self::Homecoming => 4,
            _ => 2,
        }
    }

    /// Collapse simultaneous spark clusters and contact repeats into one clear response.
    pub const fn retrigger_seconds(self) -> f32 {
        match self {
            Self::SparkPickup => 0.055,
            Self::MenuNavigate => 0.075,
            Self::EnemyBounce | Self::BossHit | Self::ShellKick => 0.10,
            Self::GuardianFreed | Self::CourseClear | Self::Homecoming => 0.75,
            _ => 0.08,
        }
    }
}
