use super::error::CodexError;
use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
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
    .ok_or_else(|| {
        CodexError::NotInstalled(format!(
            "在 PATH、~/.local/bin、~/.npm-global/bin 和系统常见安装位置均未找到 codex。\
             可在终端运行 npm install -g @openai/codex，或通过 {EXECUTABLE_ENV} 指定完整路径。"
        ))
    })
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
    let versions = home.join(".nvm").join("versions").join("node");
    let mut directories: Vec<PathBuf> = fs::read_dir(versions)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    directories.sort();
    directories
        .into_iter()
        .rev()
        .flat_map(|directory| {
            executable_names()
                .iter()
                .map(move |name| directory.join("bin").join(name))
        })
        .find(|candidate| is_executable_candidate(candidate))
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
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
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
        if let Some(app_data) = env::var_os("APPDATA") {
            let npm = PathBuf::from(app_data).join("npm");
            candidates.extend(executable_names().iter().map(|name| npm.join(name)));
        }
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            let npm = PathBuf::from(local_app_data).join("npm");
            candidates.extend(executable_names().iter().map(|name| npm.join(name)));
        }
    }
    candidates
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
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))
                .expect("设置可执行权限");
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

    fn executable_name() -> &'static str {
        if cfg!(windows) { "codex.exe" } else { "codex" }
    }
}
