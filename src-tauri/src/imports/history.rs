use super::ImportRecord;
use std::fmt::Write as _;

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

    pub fn open_thread(
        &self,
        local_thread_id: &str,
        continuation_prompt: &str,
    ) -> Result<(), OpenError> {
        (self.launcher)(local_thread_id).map_err(|reason| {
            let mut message = format!(
                "请打开 Codex 并新建会话，然后粘贴「复制续接提示」按钮中保留的不可信历史上下文。会话标识：{local_thread_id}；原因：{reason}"
            );
            if !continuation_prompt.is_empty() {
                let _ = write!(
                    message,
                    "；已保留 {0} 字符可直接粘贴的续接提示",
                    continuation_prompt.chars().count()
                );
            }
            OpenError::Unsupported(message)
        })
    }

    pub fn continuation_prompt(record: &ImportRecord) -> &str {
        record.continuation_prompt.as_deref().unwrap_or_default()
    }
}
