use std::fmt::Debug;

pub trait MidiDevice: Sized + Send + 'static {
    type Input: Send + Debug;
    type Output: Send + Debug;

    fn process_input(&mut self, data: &[u8]) -> Option<Self::Input>;
    fn process_output(&mut self, output: Self::Output) -> Vec<u8>;

    /// Outputs sent on every (re)connect.
    fn init(&mut self) -> Vec<Self::Output> {
        vec![]
    }
}

pub mod launch_control_xl;
pub mod launchpad_x;
pub mod worlde_easycontrol9;
