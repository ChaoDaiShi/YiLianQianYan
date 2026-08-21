use std::process::Command as StdCommand;

use tokio::process::Command as TokioCommand;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) const fn background_creation_flags() -> u32 {
    #[cfg(windows)]
    {
        CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    {
        0
    }
}

pub(crate) fn hide_std_command_window(command: &mut StdCommand) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(background_creation_flags());
    }
    #[cfg(not(windows))]
    let _ = command;
}

pub(crate) fn hide_tokio_command_window(command: &mut TokioCommand) {
    hide_std_command_window(command.as_std_mut());
}
