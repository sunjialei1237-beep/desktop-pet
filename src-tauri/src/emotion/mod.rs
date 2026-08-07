pub mod state;
pub mod needs;
pub mod pace;
pub mod react;

pub use state::{EmotionState, derive_mood_label};
pub use needs::{tick_loneliness, tick_needs, tick_rest_need};
pub use pace::pace_increment;
