//! The same portable geometry/materials as the native kit, without native devices or workers.
#[path = "kit/batch.rs"]
pub mod batch;
#[path = "kit/lint.rs"]
pub mod lint;
#[path = "kit/look.rs"]
pub mod look;
#[path = "kit/shadow.rs"]
pub mod shadow;
#[path = "kit/shape.rs"]
pub mod shape;
#[path = "kit/view.rs"]
pub mod view;
pub use batch::{Batch, Template, Tint};
pub use look::{Look, Materials};
pub use shadow::{ShadowMap, Shadows};
pub use view::View;
