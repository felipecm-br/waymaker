//! Input routing state shared by the event loop and the render loop.
//!
//! The event loop runs on its own task and decides, per key, whether a key is
//! looked up in the binds or sent to a text input as a plain character. That
//! decision depends on state the render loop owns (which input box is open, the
//! bind mode). One [`InputState`] per picker carries it between the two, instead
//! of process-wide statics that leaked between pickers and tests.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

/// Shared handle to a picker's [`InputState`].
pub type SharedInputState = Arc<InputState>;

#[derive(Debug, Default)]
pub struct InputState {
    /// Bind mode: binds whose trigger names this mode win over mode-less ones.
    mode: Mutex<String>,
    /// The action box (fm rename/create, ...) takes typed characters.
    action_box: AtomicBool,
    /// The navigation-mode prompt input takes typed characters.
    prompt_input: AtomicBool,
    /// A live input (query typed into a running command) takes typed characters.
    live_input: AtomicBool,
}

impl InputState {
    pub fn shared() -> SharedInputState {
        Arc::new(Self::default())
    }

    pub fn mode(&self) -> String {
        self.mode.lock().map(|m| m.clone()).unwrap_or_default()
    }

    pub fn set_mode(&self, mode: String) {
        if let Ok(mut m) = self.mode.lock() {
            *m = mode;
        }
    }

    pub fn action_box_active(&self) -> bool {
        self.action_box.load(Ordering::Relaxed)
    }

    pub fn set_action_box_active(&self, active: bool) {
        self.action_box.store(active, Ordering::Relaxed);
    }

    pub fn prompt_input_active(&self) -> bool {
        self.prompt_input.load(Ordering::Relaxed)
    }

    pub fn set_prompt_input_active(&self, active: bool) {
        self.prompt_input.store(active, Ordering::Relaxed);
    }

    pub fn live_input_active(&self) -> bool {
        self.live_input.load(Ordering::Relaxed)
    }

    pub fn set_live_input_active(&self, active: bool) {
        self.live_input.store(active, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_pickers_do_not_share_flags() {
        let a = InputState::shared();
        let b = InputState::shared();
        a.set_mode("t0".into());
        a.set_action_box_active(true);
        a.set_prompt_input_active(true);
        a.set_live_input_active(true);
        assert_eq!(b.mode(), "");
        assert!(!b.action_box_active());
        assert!(!b.prompt_input_active());
        assert!(!b.live_input_active());

        let a2 = Arc::clone(&a);
        assert_eq!(a2.mode(), "t0");
        assert!(a2.action_box_active() && a2.prompt_input_active() && a2.live_input_active());
    }
}
