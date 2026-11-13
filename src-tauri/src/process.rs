use std::process::Command;

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn configure_quiet_command(command: &mut Command) {
    let creation_flags = quiet_creation_flags();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(creation_flags);
    }
    #[cfg(not(windows))]
    {
        let _ = (command, creation_flags);
    }
}

fn quiet_creation_flags() -> u32 {
    CREATE_NO_WINDOW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_commands_use_windows_create_no_window() {
        assert_eq!(quiet_creation_flags(), CREATE_NO_WINDOW);
    }
}
