use super::error::CodexError;
use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const EXECUTABLE_ENV: &str = "AGENTBRIDGE_CODEX_EXECUTABLE";

pub fn resolve_codex_executable() -> Result<PathBuf, CodexError> {
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"));
    resolve_with(
        env::var_os(EXECUTABLE_ENV),
        env::var_os("PATH"),
        home.map(PathBuf::from),
        platform_candidates(),
    )
    .ok_or_else(|| CodexError::NotInstalled(not_installed_message_for(cfg!(windows))))
}

pub fn configure_codex_command(command: &mut Command) {
    crate::process::configure_quiet_command(command);
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"));
    if let Some(path) = codex_command_path(env::var_os("PATH"), home.map(PathBuf::from)) {
        command.env("PATH", path);
    }
}

fn codex_command_path(path_value: Option<OsString>, home: Option<PathBuf>) -> Option<OsString> {
    let mut directories = Vec::new();
    if let Some(home) = home {
        directories.extend([
            home.join(".local").join("bin"),
            home.join(".npm-global").join("bin"),
            home.join(".volta").join("bin"),
            home.join(".cargo").join("bin"),
            home.join(".bun").join("bin"),
        ]);
        directories.extend(nvm_bin_directories(&home));
    }
    #[cfg(target_os = "macos")]
    directories.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    #[cfg(target_os = "windows")]
    {
        if let Some(app_data) = env::var_os("APPDATA") {
            directories.push(PathBuf::from(app_data).join("npm"));
        }
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            let local_app_data = PathBuf::from(local_app_data);
            directories.extend([
                local_app_data.join("Volta").join("bin"),
                local_app_data
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Links"),
            ]);
        }
    }
    if let Some(path_value) = path_value {
        directories.extend(env::split_paths(&path_value));
    }
    let mut seen = std::collections::HashSet::new();
    directories.retain(|directory| {
        !directory.as_os_str().is_empty()
            && directory.exists()
            && seen.insert(directory.clone())
    });
    env::join_paths(directories).ok()
}

fn resolve_with(
    override_executable: Option<OsString>,
    path_value: Option<OsString>,
    home: Option<PathBuf>,
    platform_candidates: Vec<PathBuf>,
) -> Option<PathBuf> {
    if let Some(executable) = override_executable.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(executable));
    }
    if let Some(path) = path_value.as_deref().and_then(find_on_path) {
        return Some(path);
    }
    if let Some(home) = home {
        for candidate in home_candidates(&home) {
            if is_executable_candidate(&candidate) {
                return Some(candidate);
            }
        }
        if let Some(candidate) = find_nvm_install(&home) {
            return Some(candidate);
        }
    }
    platform_candidates
        .into_iter()
        .find(|candidate| is_executable_candidate(candidate))
}

fn find_on_path(path_value: &OsStr) -> Option<PathBuf> {
    env::split_paths(path_value)
        .filter(|directory| !directory.as_os_str().is_empty())
        .flat_map(|directory| {
            executable_names()
                .iter()
                .map(move |name| directory.join(name))
        })
        .find(|candidate| is_executable_candidate(candidate))
}

fn home_candidates(home: &Path) -> Vec<PathBuf> {
    let directories = [
        home.join(".local").join("bin"),
        home.join(".npm-global").join("bin"),
        home.join(".volta").join("bin"),
        home.join(".cargo").join("bin"),
        home.join(".bun").join("bin"),
    ];
    directories
        .into_iter()
        .flat_map(|directory| {
            executable_names()
                .iter()
                .map(move |name| directory.join(name))
        })
        .collect()
}

fn find_nvm_install(home: &Path) -> Option<PathBuf> {
    nvm_bin_directories(home)
        .into_iter()
        .flat_map(|directory| {
            executable_names()
                .iter()
                .map(move |name| directory.join(name))
        })
        .find(|candidate| is_executable_candidate(candidate))
}

fn nvm_bin_directories(home: &Path) -> Vec<PathBuf> {
    let versions = home.join(".nvm").join("versions").join("node");
    let Ok(entries) = fs::read_dir(versions) else {
        return Vec::new();
    };
    let mut directories: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    directories.sort();
    directories
        .into_iter()
        .rev()
        .map(|directory| directory.join("bin"))
        .collect()
}

fn executable_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["codex.exe", "codex.cmd", "codex.bat", "codex"]
    } else {
        &["codex"]
    }
}

fn is_executable_candidate(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

fn platform_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(target_os = "macos")]
    {
        candidates.extend([
            PathBuf::from("/opt/homebrew/bin/codex"),
            PathBuf::from("/usr/local/bin/codex"),
            PathBuf::from(
                "/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex",
            ),
            PathBuf::from(
                "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
            ),
            PathBuf::from(
                "/Applications/Codex.app/Contents/Resources/codex-cli/bin/codex",
            ),
        ]);
    }
    #[cfg(target_os = "windows")]
    {
        let local_app_data = env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let app_data = env::var_os("APPDATA").map(PathBuf::from);
        let user_profile = env::var_os("USERPROFILE").map(PathBuf::from);
        let program_files = env::var_os("ProgramFiles").map(PathBuf::from);
        let program_files_x86 = env::var_os("ProgramFiles(x86)").map(PathBuf::from);
        let program_data = env::var_os("ProgramData").map(PathBuf::from);
        candidates.extend(windows_candidates_from_roots(
            local_app_data.as_deref(),
            app_data.as_deref(),
            user_profile.as_deref(),
            program_files.as_deref(),
            program_files_x86.as_deref(),
            program_data.as_deref(),
        ));
    }
    candidates
}

#[cfg(any(target_os = "windows", test))]
fn windows_candidates_from_roots(
    local_app_data: Option<&Path>,
    app_data: Option<&Path>,
    user_profile: Option<&Path>,
    program_files: Option<&Path>,
    program_files_x86: Option<&Path>,
    program_data: Option<&Path>,
) -> Vec<PathBuf> {
    let mut directories = Vec::new();

    if let Some(local_app_data) = local_app_data {
        directories.extend([
            local_app_data
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin"),
            local_app_data.join("OpenAI").join("Codex").join("bin"),
            local_app_data.join("Programs").join("Codex").join("bin"),
            local_app_data.join("Programs").join("codex").join("bin"),
            local_app_data.join("npm"),
            local_app_data.join("Volta").join("bin"),
            local_app_data
                .join("Microsoft")
                .join("WinGet")
                .join("Links"),
            local_app_data.join("Microsoft").join("WindowsApps"),
        ]);
    }
    if let Some(app_data) = app_data {
        directories.push(app_data.join("npm"));
    }
    if let Some(user_profile) = user_profile {
        directories.extend([
            user_profile.join(".codex").join("bin"),
            user_profile.join("scoop").join("shims"),
        ]);
    }
    if let Some(program_files) = program_files {
        directories.extend([
            program_files.join("OpenAI").join("Codex").join("bin"),
            program_files.join("Codex"),
            program_files.join("nodejs"),
        ]);
    }
    if let Some(program_files_x86) = program_files_x86 {
        directories.push(program_files_x86.join("Codex"));
    }
    if let Some(program_data) = program_data {
        directories.push(program_data.join("chocolatey").join("bin"));
    }

    directories
        .into_iter()
        .flat_map(|directory| {
            ["codex.exe", "codex.cmd", "codex.bat", "codex"]
                .into_iter()
                .map(move |name| directory.join(name))
        })
        .collect()
}

fn not_installed_message_for(windows: bool) -> String {
    if windows {
        format!(
            "在 PATH、%LOCALAPPDATA%\\Programs\\OpenAI\\Codex\\bin、%APPDATA%\\npm、\
             WinGet、Volta、Scoop 等常见位置均未找到 codex.exe 或 codex.cmd。\
             可在 PowerShell 运行官方安装命令 \
             `powershell -ExecutionPolicy ByPass -c \"irm https://chatgpt.com/codex/install.ps1 | iex\"`，\
             或运行 `npm install -g @openai/codex`；也可以通过 {EXECUTABLE_ENV} \
             指定 codex.exe 的完整路径。"
        )
    } else {
        format!(
            "在 PATH、~/.local/bin、~/.npm-global/bin 和系统常见安装位置均未找到 codex。\
             可在终端运行 npm install -g @openai/codex，或通过 {EXECUTABLE_ENV} 指定完整路径。"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        ffi::OsString,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("系统时间有效")
                .as_nanos();
            let path = env::temp_dir().join(format!(
                "agentbridge-executable-{name}-{}-{suffix}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("创建测试目录");
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write_executable(path: &Path) {
        fs::create_dir_all(path.parent().expect("测试文件应有父目录")).expect("创建 bin 目录");
        fs::write(path, b"#!/bin/sh\nexit 0\n").expect("写入测试可执行文件");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("设置可执行权限");
        }
    }

    #[test]
    fn explicit_override_has_highest_priority() {
        let temp = TestDir::new("override");
        let override_path = temp.0.join("custom-codex");
        write_executable(&override_path);

        let resolved = resolve_with(
            Some(override_path.clone().into_os_string()),
            Some(OsString::from("/missing")),
            Some(temp.0.clone()),
            Vec::new(),
        );

        assert_eq!(resolved, Some(override_path));
    }

    #[test]
    fn finds_codex_on_the_supplied_path() {
        let temp = TestDir::new("path");
        let executable = temp.0.join("bin").join(executable_name());
        write_executable(&executable);
        let path_value = env::join_paths([temp.0.join("bin")]).expect("构造测试 PATH");

        let resolved = resolve_with(
            None,
            Some(path_value),
            Some(temp.0.join("home")),
            Vec::new(),
        );

        assert_eq!(resolved, Some(executable));
    }

    #[test]
    fn finds_npm_global_install_when_finder_has_no_terminal_path() {
        let temp = TestDir::new("npm");
        let home = temp.0.join("home");
        let executable = home.join(".npm-global").join("bin").join(executable_name());
        write_executable(&executable);

        let resolved = resolve_with(None, Some(OsString::new()), Some(home), Vec::new());

        assert_eq!(resolved, Some(executable));
    }

    #[test]
    fn command_path_includes_user_package_manager_directories() {
        let temp = TestDir::new("command-path");
        let home = temp.0.join("home");
        let npm_bin = home.join(".npm-global").join("bin");
        let local_bin = home.join(".local").join("bin");
        let system_bin = temp.0.join("system-bin");
        fs::create_dir_all(&npm_bin).expect("创建 npm 目录");
        fs::create_dir_all(&local_bin).expect("创建本地工具目录");
        fs::create_dir_all(&system_bin).expect("创建系统工具目录");
        let existing = env::join_paths([&system_bin])
            .expect("构造测试 PATH");

        let path = codex_command_path(Some(existing), Some(home)).expect("应构造子进程 PATH");
        let directories: Vec<PathBuf> = env::split_paths(&path).collect();

        assert!(directories.contains(&local_bin));
        assert!(directories.contains(&npm_bin));
        assert!(directories.contains(&system_bin));
    }

    #[test]
    fn uses_platform_fallback_after_user_installations() {
        let temp = TestDir::new("fallback");
        let fallback = temp.0.join("Codex CLI").join(executable_name());
        write_executable(&fallback);

        let resolved = resolve_with(
            None,
            Some(OsString::new()),
            Some(temp.0.join("home")),
            vec![fallback.clone()],
        );

        assert_eq!(resolved, Some(fallback));
    }

    #[test]
    fn windows_candidates_include_the_official_standalone_installer_location() {
        let temp = TestDir::new("windows-official");
        let local_app_data = temp.0.join("Local");
        let candidates =
            windows_candidates_from_roots(Some(&local_app_data), None, None, None, None, None);
        let expected = local_app_data
            .join("Programs")
            .join("OpenAI")
            .join("Codex")
            .join("bin")
            .join("codex.exe");

        assert!(candidates.contains(&expected));
    }

    #[test]
    fn windows_not_installed_message_names_the_official_installer_location() {
        let message = not_installed_message_for(true);

        assert!(message.contains(r"%LOCALAPPDATA%\Programs\OpenAI\Codex\bin"));
        assert!(message.contains("codex.exe"));
        assert!(message.contains("install.ps1"));
    }

    fn executable_name() -> &'static str {
        if cfg!(windows) { "codex.exe" } else { "codex" }
    }
}
