pub(crate) mod process;
pub mod text;

#[cfg(test)]
mod process_tests {
    #[test]
    fn background_commands_use_the_windows_no_window_flag() {
        #[cfg(windows)]
        assert_eq!(super::process::background_creation_flags(), 0x0800_0000);

        #[cfg(not(windows))]
        assert_eq!(super::process::background_creation_flags(), 0);
    }
}
