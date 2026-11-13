#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("没有可用的 Codex 打开方式。{0}")]
    Unsupported(String),
}

pub type Launcher = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct CodexOpener {
    launcher: Launcher,
}

impl Default for CodexOpener {
    fn default() -> Self {
        Self::with_launcher(Box::new(|_| {
            Err("当前 Codex 桌面版没有受支持的第三方 deep link 或启动接口".into())
        }))
    }
}

impl CodexOpener {
    pub fn with_launcher(launcher: Launcher) -> Self {
        Self { launcher }
    }

    pub fn open_thread(&self, local_thread_id: &str) -> Result<(), OpenError> {
        (self.launcher)(local_thread_id).map_err(|reason| {
            OpenError::Unsupported(format!(
                "无法自动打开会话。请重启 Codex 并在历史列表中查找会话标识：{local_thread_id}；原因：{reason}"
            ))
        })
    }
}
