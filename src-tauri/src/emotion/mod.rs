pub mod state;
pub mod homeostasis;
pub mod needs;
pub mod pace;
pub mod react;

pub use state::{EmotionState, derive_mood_label};
pub use homeostasis::apply_drift;
pub use needs::tick_needs;
pub use pace::pace_increment;
